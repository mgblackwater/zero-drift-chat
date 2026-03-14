use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use futures::TryStreamExt;
use tokio::io::AsyncWriteExt;
use tokio_util::io::StreamReader;

/// Compute a stable u64 hash of `url` used for cache file naming.
fn url_hash(url: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish()
}

/// Derive a file extension from a URL path component.
/// Returns `None` when the URL has no path extension or an unrecognised one.
#[allow(dead_code)]
fn ext_from_url(url: &str) -> Option<&'static str> {
    // Strip query/fragment before inspecting the path
    let path_part = url.split('?').next().unwrap_or(url);
    let path_part = path_part.split('#').next().unwrap_or(path_part);
    let lower = path_part.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("jpg")
    } else if lower.ends_with(".png") {
        Some("png")
    } else if lower.ends_with(".gif") {
        Some("gif")
    } else if lower.ends_with(".webp") {
        Some("webp")
    } else if lower.ends_with(".bmp") {
        Some("bmp")
    } else {
        None
    }
}

/// Derive a file extension from a `Content-Type` header value.
fn ext_from_content_type(ct: &str) -> Option<&'static str> {
    // Content-Type may contain parameters: "image/jpeg; charset=..."
    let mime = ct.split(';').next().unwrap_or(ct).trim();
    match mime {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

/// Generate a deterministic temp file path for a given image URL.
/// Pattern: /tmp/zero-drift-<u64_hash>.<ext>
///
/// `ext` should be derived from the URL path or the HTTP `Content-Type` header
/// so the saved file has the correct format extension (not always `.jpg`).
///
/// Note: DefaultHasher is not stable across Rust versions; cached files may become
/// orphans after toolchain upgrades. cleanup_temp_images() handles this after 24h.
pub fn temp_path_for_url(url: &str, ext: &str) -> PathBuf {
    std::env::temp_dir().join(format!("zero-drift-{}.{}", url_hash(url), ext))
}

/// Write `bytes` to a deterministic temp file (keyed on `cache_key`) and open
/// it with the OS default viewer.
///
/// `cache_key` is any string that uniquely identifies this media (e.g. the CDN
/// URL or the WhatsApp direct_path).  `mime_type` is used to derive the file
/// extension; falls back to `jpg` when `None` or unrecognised.
pub async fn open_image_from_bytes(
    bytes: Vec<u8>,
    cache_key: &str,
    mime_type: Option<&str>,
) -> anyhow::Result<()> {
    let ext = mime_type
        .and_then(ext_from_content_type)
        .unwrap_or("jpg");

    let path = temp_path_for_url(cache_key, ext);

    if !tokio::fs::try_exists(&path).await? {
        let tmp_path = path.with_extension("tmp");
        let write_result = async {
            let mut file = tokio::fs::File::create(&tmp_path).await?;
            file.write_all(&bytes).await?;
            file.flush().await?;
            drop(file);
            tokio::fs::rename(&tmp_path, &path).await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(e) = write_result {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(e);
        }
    }

    spawn_os_opener(&path).await;
    Ok(())
}

/// Stream-download `url` to a temp file and open it with the OS default viewer.
///
/// The correct file extension is determined by:
///   1. The URL path (e.g. `.../photo.png` → `png`)
///   2. The HTTP `Content-Type` response header
///   3. Fallback to `jpg` when neither is conclusive
///
/// Image bytes flow: network → kernel buffer → disk.
/// The Rust heap holds only the ~8 KB read buffer inside `tokio::io::copy` —
/// never the full image.
#[allow(dead_code)]
pub async fn open_image(url: String) -> anyhow::Result<()> {
    // Try to determine the extension from the URL before making a request.
    // If successful we can check the cache immediately; otherwise we must
    // wait for the response headers to know the real extension.
    let url_ext = ext_from_url(&url);

    // Fast path: URL extension known and file is already cached.
    if let Some(ext) = url_ext {
        let path = temp_path_for_url(&url, ext);
        if tokio::fs::try_exists(&path).await? {
            spawn_os_opener(&path).await;
            return Ok(());
        }
    }

    // Issue the HTTP request (needed either because:
    //   - URL had no recognisable extension, or
    //   - the cached file does not exist yet).
    let response = reqwest::get(&url).await?.error_for_status()?;

    // Resolve extension: URL path wins; fall back to Content-Type; default jpg.
    let ext = url_ext
        .or_else(|| {
            response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .and_then(ext_from_content_type)
        })
        .unwrap_or("jpg");

    let path = temp_path_for_url(&url, ext);

    // Re-check cache now that we have the definitive extension.
    if !tokio::fs::try_exists(&path).await? {
        let tmp_path = path.with_extension("tmp");
        let download_result = async {
            let byte_stream = response.bytes_stream().map_err(std::io::Error::other);
            let mut reader = StreamReader::new(byte_stream);
            let mut file = tokio::fs::File::create(&tmp_path).await?;
            tokio::io::copy(&mut reader, &mut file).await?;
            file.flush().await?;
            drop(file); // close before rename
            tokio::fs::rename(&tmp_path, &path).await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(e) = download_result {
            // Clean up partial download if any
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(e);
        }
    }

    spawn_os_opener(&path).await;
    Ok(())
}

/// Spawn the OS default image viewer for `path` and wait for it to exit.
/// Uses `spawn_blocking` so the `.wait()` call does not block the async executor.
/// Waiting prevents zombie processes in the kernel process table.
///
/// stdout/stderr of the child process are redirected to /dev/null (Null on
/// Windows) so that noisy viewers (e.g. Chromium launched via xdg-open) do not
/// corrupt the TUI terminal output.
async fn spawn_os_opener(path: &std::path::Path) {
    let path = path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "linux")]
        let spawn_result = std::process::Command::new("xdg-open")
            .arg(&path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        #[cfg(target_os = "macos")]
        let spawn_result = std::process::Command::new("open")
            .arg(&path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        #[cfg(target_os = "windows")]
        let spawn_result = std::process::Command::new("cmd")
            .args(["/c", "start", "", &path.to_string_lossy()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let spawn_result: std::io::Result<std::process::Child> =
            Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "unsupported OS"));

        match spawn_result {
            Ok(mut child) => {
                // Wait for the opener process to exit so it doesn't become a zombie.
                // xdg-open / open fork and exit quickly; wait() returns in milliseconds.
                let _ = child.wait();
            }
            Err(e) => {
                tracing::error!("Failed to spawn OS opener for {:?}: {}", path, e);
            }
        }
    })
    .await;

    if let Err(e) = result {
        tracing::error!("spawn_os_opener task panicked: {}", e);
    }
}

/// Delete `/tmp/zero-drift-*` files older than 24 hours.
/// Best-effort: all errors are silently logged.
pub fn cleanup_temp_images() {
    let cutoff = match SystemTime::now().checked_sub(Duration::from_secs(24 * 3600)) {
        Some(t) => t,
        None => return,
    };

    let tmp = std::env::temp_dir();
    let entries = match std::fs::read_dir(&tmp) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("cleanup_temp_images: cannot read {:?}: {}", tmp, e);
            return;
        }
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("zero-drift-") {
            continue;
        }
        let path = entry.path();
        let mtime = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if mtime < cutoff {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!("cleanup_temp_images: failed to remove {:?}: {}", path, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ext_from_content_type, ext_from_url, temp_path_for_url};

    // --- temp_path_for_url ---

    #[test]
    fn test_temp_path_same_url_gives_same_path() {
        let a = temp_path_for_url("https://cdn.example.com/img.jpg", "jpg");
        let b = temp_path_for_url("https://cdn.example.com/img.jpg", "jpg");
        assert_eq!(a, b);
    }

    #[test]
    fn test_temp_path_different_urls_give_different_paths() {
        let a = temp_path_for_url("https://cdn.example.com/img1.jpg", "jpg");
        let b = temp_path_for_url("https://cdn.example.com/img2.jpg", "jpg");
        assert_ne!(a, b);
    }

    #[test]
    fn test_temp_path_uses_provided_extension() {
        let jpg = temp_path_for_url("https://cdn.example.com/img", "jpg");
        let png = temp_path_for_url("https://cdn.example.com/img", "png");
        // Same URL → same hash, but different extension
        let jpg_s = jpg.to_string_lossy();
        let png_s = png.to_string_lossy();
        assert!(jpg_s.ends_with(".jpg"), "expected .jpg, got: {}", jpg_s);
        assert!(png_s.ends_with(".png"), "expected .png, got: {}", png_s);
        assert!(jpg_s.contains("zero-drift-"), "path should contain zero-drift-: {}", jpg_s);
    }

    // --- ext_from_url ---

    #[test]
    fn test_ext_from_url_jpg() {
        assert_eq!(ext_from_url("https://cdn.example.com/photo.jpg"), Some("jpg"));
        assert_eq!(ext_from_url("https://cdn.example.com/photo.jpeg"), Some("jpg"));
        assert_eq!(ext_from_url("https://cdn.example.com/photo.JPG"), Some("jpg"));
    }

    #[test]
    fn test_ext_from_url_png() {
        assert_eq!(ext_from_url("https://cdn.example.com/image.png"), Some("png"));
    }

    #[test]
    fn test_ext_from_url_gif() {
        assert_eq!(ext_from_url("https://cdn.example.com/anim.gif"), Some("gif"));
    }

    #[test]
    fn test_ext_from_url_webp() {
        assert_eq!(ext_from_url("https://cdn.example.com/img.webp"), Some("webp"));
    }

    #[test]
    fn test_ext_from_url_strips_query() {
        assert_eq!(
            ext_from_url("https://cdn.example.com/photo.png?v=1&x=2"),
            Some("png")
        );
    }

    #[test]
    fn test_ext_from_url_no_extension() {
        assert_eq!(ext_from_url("https://cdn.example.com/media/abc123"), None);
    }

    #[test]
    fn test_ext_from_url_unknown_extension() {
        assert_eq!(ext_from_url("https://cdn.example.com/file.tiff"), None);
    }

    // --- ext_from_content_type ---

    #[test]
    fn test_ext_from_content_type_jpeg() {
        assert_eq!(ext_from_content_type("image/jpeg"), Some("jpg"));
        assert_eq!(ext_from_content_type("image/jpeg; charset=utf-8"), Some("jpg"));
    }

    #[test]
    fn test_ext_from_content_type_png() {
        assert_eq!(ext_from_content_type("image/png"), Some("png"));
    }

    #[test]
    fn test_ext_from_content_type_gif() {
        assert_eq!(ext_from_content_type("image/gif"), Some("gif"));
    }

    #[test]
    fn test_ext_from_content_type_webp() {
        assert_eq!(ext_from_content_type("image/webp"), Some("webp"));
    }

    #[test]
    fn test_ext_from_content_type_unknown() {
        assert_eq!(ext_from_content_type("application/octet-stream"), None);
        assert_eq!(ext_from_content_type("text/html"), None);
    }
}
