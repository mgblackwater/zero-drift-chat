use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::core::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub mock_provider: MockProviderConfig,
    #[serde(default)]
    pub whatsapp: WhatsAppConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub ai: AiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_tick_rate")]
    pub tick_rate_ms: u64,
    #[serde(default = "default_render_rate")]
    pub render_rate_ms: u64,
    #[serde(default = "default_chat_list_width")]
    pub chat_list_width_percent: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockProviderConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_chat_count")]
    pub chat_count: usize,
    #[serde(default = "default_message_interval")]
    pub message_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub phone_number: Option<String>,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            phone_number: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_id: i32,
    #[serde(default)]
    pub api_hash: String,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_id: 0,
            api_hash: String::new(),
        }
    }
}

fn default_data_dir() -> String {
    dirs::home_dir()
        .map(|h| h.join(".zero-drift-chat").to_string_lossy().to_string())
        .unwrap_or_else(|| ".zero-drift-chat".to_string())
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_tick_rate() -> u64 {
    250
}

fn default_render_rate() -> u64 {
    33
}

fn default_chat_list_width() -> u16 {
    30
}

fn default_true() -> bool {
    true
}

fn default_chat_count() -> usize {
    5
}

fn default_message_interval() -> u64 {
    3
}

fn default_ai_provider() -> String {
    "ollama".to_string()
}
fn default_ai_base_url() -> String {
    "http://localhost:11434".to_string()
}
fn default_ai_model() -> String {
    "qwen2.5:1.5b-instruct".to_string()
}
fn default_context_messages() -> usize {
    10
}
fn default_summary_threshold() -> usize {
    50
}
fn default_debounce_ms() -> u64 {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ai_provider")]
    pub provider: String,
    #[serde(default = "default_ai_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_ai_model")]
    pub model: String,
    #[serde(default = "default_context_messages")]
    pub context_messages: usize,
    #[serde(default = "default_summary_threshold")]
    pub summary_threshold: usize,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default)]
    pub debug: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_ai_provider(),
            base_url: default_ai_base_url(),
            api_key: None,
            model: default_ai_model(),
            context_messages: default_context_messages(),
            summary_threshold: default_summary_threshold(),
            debounce_ms: default_debounce_ms(),
            debug: false,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            tui: TuiConfig::default(),
            mock_provider: MockProviderConfig::default(),
            whatsapp: WhatsAppConfig::default(),
            telegram: TelegramConfig::default(),
            ai: AiConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            log_level: default_log_level(),
        }
    }
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            tick_rate_ms: default_tick_rate(),
            render_rate_ms: default_render_rate(),
            chat_list_width_percent: default_chat_list_width(),
        }
    }
}

impl Default for MockProviderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            chat_count: default_chat_count(),
            message_interval_secs: default_message_interval(),
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: AppConfig = toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse config {}: {}", path.display(), e))?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize config: {}", e))?;
        let content = format!(
            "# zero-drift-chat configuration\n# Edit manually or via in-app settings (s)\n\n{}",
            toml_str
        );
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_telegram_config() {
        let toml = r#"
[telegram]
enabled = true
api_id = 123456
api_hash = "abc123def456"
"#;
        let result = toml::from_str::<AppConfig>(toml);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let cfg = result.unwrap();
        assert!(cfg.telegram.enabled);
        assert_eq!(cfg.telegram.api_id, 123456);
        assert_eq!(cfg.telegram.api_hash, "abc123def456");
    }

    #[test]
    fn test_telegram_config_defaults() {
        let cfg = AppConfig::default();
        assert!(!cfg.telegram.enabled, "telegram disabled by default");
        assert_eq!(cfg.telegram.api_id, 0);
        assert!(cfg.telegram.api_hash.is_empty());
    }

    #[test]
    fn test_parse_ai_config() {
        let toml = r#"
[ai]
enabled = true
provider = "ollama"
base_url = "http://localhost:8080"
model = "qwen2.5-1.5b-instruct-q4_k_m"
context_messages = 10
summary_threshold = 50
debounce_ms = 500
debug = true
"#;
        let result = toml::from_str::<AppConfig>(toml);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let cfg = result.unwrap();
        assert!(cfg.ai.enabled, "ai.enabled should be true");
        assert_eq!(cfg.ai.base_url, "http://localhost:8080");
        assert_eq!(cfg.ai.model, "qwen2.5-1.5b-instruct-q4_k_m");
        assert!(cfg.ai.debug);
    }
}
