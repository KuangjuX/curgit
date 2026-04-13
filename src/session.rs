use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SESSION_TOKEN_LIMIT: u64 = 80_000;
const SESSION_TURN_LIMIT: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub session_id: Option<String>,
    pub context_tokens: u64,
    pub turn_count: u64,
    pub last_used: u64,
}

impl SessionState {
    fn session_file() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("curgit").join("session.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::session_file() else {
            return Self::default();
        };
        if !path.exists() {
            return Self::default();
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::session_file().context("Could not determine config directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    pub fn invalidate(&mut self) {
        self.session_id = None;
        self.context_tokens = 0;
        self.turn_count = 0;
        self.last_used = 0;
    }

    pub fn is_valid(&self) -> bool {
        if self.session_id.is_none() {
            return false;
        }
        if self.context_tokens > SESSION_TOKEN_LIMIT {
            return false;
        }
        if self.turn_count >= SESSION_TURN_LIMIT {
            return false;
        }
        true
    }

    pub fn update_from_response(&mut self, session_id: &str, usage: &CursorUsage) {
        self.session_id = Some(session_id.to_string());
        self.context_tokens = usage.cache_read_tokens + usage.cache_write_tokens;
        self.turn_count += 1;
        self.last_used = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct CursorJsonResponse {
    #[serde(rename = "type")]
    pub resp_type: Option<String>,
    pub subtype: Option<String>,
    pub is_error: Option<bool>,
    pub result: Option<String>,
    pub session_id: Option<String>,
    pub usage: Option<CursorUsage>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct CursorUsage {
    #[serde(default, rename = "inputTokens")]
    pub input_tokens: u64,
    #[serde(default, rename = "outputTokens")]
    pub output_tokens: u64,
    #[serde(default, rename = "cacheReadTokens")]
    pub cache_read_tokens: u64,
    #[serde(default, rename = "cacheWriteTokens")]
    pub cache_write_tokens: u64,
}
