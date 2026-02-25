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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            tui: TuiConfig::default(),
            mock_provider: MockProviderConfig::default(),
            whatsapp: WhatsAppConfig::default(),
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
            let config: AppConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }
}
