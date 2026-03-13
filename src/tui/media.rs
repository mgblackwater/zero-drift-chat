use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use futures::TryStreamExt;
use tokio_util::io::StreamReader;

/// Generate a deterministic temp file path for a given image URL.
/// Pattern: /tmp/zero-drift-<u64_hash>.jpg
///
/// Note: DefaultHasher is not stable across Rust versions; cached files may become
/// orphans after toolchain upgrades. cleanup_temp_images() handles this after 24h.
pub fn temp_path_for_url(url: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();
    std::env::temp_dir().join(format!("zero-drift-{}.jpg", hash))
}

/// Stream-download `url` to a temp file and open it with the OS default viewer.
///
/// Image bytes flow: network → kernel buffer → disk.
/// The Rust heap holds only the ~8 KB read buffer inside `tokio::io::copy` —
/// never the full image.
pub async fn open_image(url: String) -> anyhow::Result<()> {
    let path = temp_path_for_url(&url);

    // Download only if not already cached
    if !tokio::fs::try_exists(&path).await? {
        let tmp_path = path.with_extension("tmp");
        let download_result = async {
            let response = reqwest::get(&url).await?.error_for_status()?;
            let byte_stream = response
                .bytes_stream()
                .map_err(std::io::Error::other);
            let mut reader = StreamReader::new(byte_stream);
            let mut file = tokio::fs::File::create(&tmp_path).await?;
            tokio::io::copy(&mut reader, &mut file).await?;
            tokio::fs::rename(&tmp_path, &path).await?;
            Ok::<_, anyhow::Error>(())
        }.await;
        if let Err(e) = download_result {
            // Clean up partial download if any
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(e);
        }
    }

    spawn_os_opener(&path);
    Ok(())
}

/// Spawn the OS default image viewer for `path`. Fire-and-forget.
fn spawn_os_opener(path: &std::path::Path) {
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();

    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();

    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/c", "start", "", &path.to_string_lossy()])
        .spawn();

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let result: std::io::Result<std::process::Child> =
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "unsupported OS"));

    if let Err(e) = result {
        tracing::error!("Failed to spawn OS opener for {:?}: {}", path, e);
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
    use super::temp_path_for_url;

    #[test]
    fn test_temp_path_same_url_gives_same_path() {
        let a = temp_path_for_url("https://cdn.example.com/img.jpg");
        let b = temp_path_for_url("https://cdn.example.com/img.jpg");
        assert_eq!(a, b);
    }

    #[test]
    fn test_temp_path_different_urls_give_different_paths() {
        let a = temp_path_for_url("https://cdn.example.com/img1.jpg");
        let b = temp_path_for_url("https://cdn.example.com/img2.jpg");
        assert_ne!(a, b);
    }

    #[test]
    fn test_temp_path_starts_with_tmp_prefix() {
        let p = temp_path_for_url("https://cdn.example.com/img.jpg");
        let s = p.to_string_lossy();
        assert!(s.contains("zero-drift-"), "path should contain zero-drift-: {}", s);
        assert!(s.ends_with(".jpg"), "path should end with .jpg: {}", s);
    }
}
