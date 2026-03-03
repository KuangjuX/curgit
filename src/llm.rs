use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

const CURSOR_CLI_FALLBACK_PATH: &str = "/Applications/Cursor.app/Contents/Resources/app/bin/cursor";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Cursor,
    Ollama,
    OpenAI,
    Claude,
    Kimi,
    DeepSeek,
    Custom,
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Provider::Cursor => write!(f, "cursor"),
            Provider::Ollama => write!(f, "ollama"),
            Provider::OpenAI => write!(f, "openai"),
            Provider::Claude => write!(f, "claude"),
            Provider::Kimi => write!(f, "kimi"),
            Provider::DeepSeek => write!(f, "deepseek"),
            Provider::Custom => write!(f, "custom"),
        }
    }
}

impl Provider {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "cursor" | "auto" => Ok(Provider::Cursor),
            "ollama" | "local" => Ok(Provider::Ollama),
            "openai" | "gpt" => Ok(Provider::OpenAI),
            "claude" | "anthropic" => Ok(Provider::Claude),
            "kimi" | "moonshot" => Ok(Provider::Kimi),
            "deepseek" => Ok(Provider::DeepSeek),
            "custom" => Ok(Provider::Custom),
            _ => bail!(
                "Unknown provider '{s}'. Available: cursor, ollama, openai, claude, kimi, deepseek, custom"
            ),
        }
    }

    pub fn default_base_url(&self) -> &str {
        match self {
            Provider::Cursor => "",
            Provider::Ollama => "http://localhost:11434/v1",
            Provider::OpenAI => "https://api.openai.com/v1",
            Provider::Claude => "https://api.anthropic.com",
            Provider::Kimi => "https://api.moonshot.cn/v1",
            Provider::DeepSeek => "https://api.deepseek.com/v1",
            Provider::Custom => "http://localhost:11434/v1",
        }
    }

    pub fn default_model(&self) -> &str {
        match self {
            Provider::Cursor => "cursor-auto",
            Provider::Ollama => "qwen2.5-coder:7b",
            Provider::OpenAI => "gpt-4o-mini",
            Provider::Claude => "claude-sonnet-4-20250514",
            Provider::Kimi => "moonshot-v1-8k",
            Provider::DeepSeek => "deepseek-chat",
            Provider::Custom => "gpt-4o-mini",
        }
    }

    pub fn requires_api_key(&self) -> bool {
        !matches!(self, Provider::Cursor | Provider::Ollama)
    }

    pub fn uses_anthropic_api(&self) -> bool {
        matches!(self, Provider::Claude)
    }

    pub fn uses_cursor_cli(&self) -> bool {
        matches!(self, Provider::Cursor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: Provider,
    pub api_key: Option<String>,
    pub api_base: String,
    pub model: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Cursor,
            api_key: None,
            api_base: Provider::Cursor.default_base_url().to_string(),
            model: Provider::Cursor.default_model().to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ConfigFile {
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub providers: ProviderOverrides,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ProviderOverrides {
    pub cursor: Option<ProviderConfig>,
    pub ollama: Option<ProviderConfig>,
    pub openai: Option<ProviderConfig>,
    pub claude: Option<ProviderConfig>,
    pub kimi: Option<ProviderConfig>,
    pub deepseek: Option<ProviderConfig>,
    pub custom: Option<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub model: Option<String>,
}

impl ConfigFile {
    fn provider_config(&self, provider: &Provider) -> Option<&ProviderConfig> {
        match provider {
            Provider::Cursor => self.providers.cursor.as_ref(),
            Provider::Ollama => self.providers.ollama.as_ref(),
            Provider::OpenAI => self.providers.openai.as_ref(),
            Provider::Claude => self.providers.claude.as_ref(),
            Provider::Kimi => self.providers.kimi.as_ref(),
            Provider::DeepSeek => self.providers.deepseek.as_ref(),
            Provider::Custom => self.providers.custom.as_ref(),
        }
    }
}

impl LlmConfig {
    /// Build config with priority: CLI args > env vars > config file > defaults
    pub fn resolve(
        cli_provider: Option<&str>,
        cli_model: Option<&str>,
        cli_api_base: Option<&str>,
    ) -> Result<Self> {
        let file_config = Self::load_config_file();

        let provider = cli_provider
            .map(|s| Provider::from_str(s))
            .or_else(|| std::env::var("CURGIT_PROVIDER").ok().map(|s| Provider::from_str(&s)))
            .or_else(|| {
                file_config
                    .as_ref()
                    .ok()
                    .and_then(|c| c.provider.as_deref())
                    .map(Provider::from_str)
            })
            .unwrap_or(Ok(Provider::Cursor))?;

        let provider_env = provider.env_prefix();
        let file_provider_cfg = file_config
            .as_ref()
            .ok()
            .and_then(|c| c.provider_config(&provider).cloned());

        let api_key = std::env::var(format!("CURGIT_{provider_env}_API_KEY"))
            .ok()
            .or_else(|| std::env::var("CURGIT_API_KEY").ok())
            .or_else(|| {
                if matches!(provider, Provider::OpenAI) {
                    std::env::var("OPENAI_API_KEY").ok()
                } else {
                    None
                }
            })
            .or_else(|| file_provider_cfg.as_ref().and_then(|c| c.api_key.clone()))
            .or_else(|| file_config.as_ref().ok().and_then(|c| c.api_key.clone()));

        let api_base = cli_api_base
            .map(|s| s.to_string())
            .or_else(|| std::env::var(format!("CURGIT_{provider_env}_API_BASE")).ok())
            .or_else(|| std::env::var("CURGIT_API_BASE").ok())
            .or_else(|| {
                if matches!(provider, Provider::OpenAI) {
                    std::env::var("OPENAI_API_BASE").ok()
                } else {
                    None
                }
            })
            .or_else(|| file_provider_cfg.as_ref().and_then(|c| c.api_base.clone()))
            .or_else(|| {
                file_config
                    .as_ref()
                    .ok()
                    .and_then(|c| c.api_base.clone())
            })
            .unwrap_or_else(|| provider.default_base_url().to_string());

        let model = cli_model
            .map(|s| s.to_string())
            .or_else(|| std::env::var(format!("CURGIT_{provider_env}_MODEL")).ok())
            .or_else(|| std::env::var("CURGIT_MODEL").ok())
            .or_else(|| file_provider_cfg.as_ref().and_then(|c| c.model.clone()))
            .or_else(|| {
                file_config
                    .as_ref()
                    .ok()
                    .and_then(|c| c.model.clone())
            })
            .unwrap_or_else(|| provider.default_model().to_string());

        if provider.requires_api_key() && api_key.is_none() {
            bail!(
                "Provider '{}' requires an API key.\n\
                 Set it via:\n  \
                 - CURGIT_{}_API_KEY environment variable\n  \
                 - CURGIT_API_KEY environment variable\n  \
                 - 'api_key' in ~/.config/curgit/config.toml\n  \
                 - '[providers.{}].api_key' in ~/.config/curgit/config.toml\n  \
                 - Or switch to 'cursor' (default) or 'ollama' for local inference (no key needed)",
                provider,
                provider_env,
                provider
            );
        }

        Ok(Self {
            provider,
            api_key,
            api_base,
            model,
        })
    }

    fn load_config_file() -> Result<ConfigFile> {
        let config_path = dirs::config_dir()
            .map(|d| d.join("curgit").join("config.toml"))
            .context("Could not determine config directory")?;

        if !config_path.exists() {
            return Ok(ConfigFile::default());
        }

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;

        toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", config_path.display()))
    }

    pub fn config_file_path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("curgit").join("config.toml"))
    }
}

impl Provider {
    fn env_prefix(&self) -> &'static str {
        match self {
            Provider::Cursor => "CURSOR",
            Provider::Ollama => "OLLAMA",
            Provider::OpenAI => "OPENAI",
            Provider::Claude => "CLAUDE",
            Provider::Kimi => "KIMI",
            Provider::DeepSeek => "DEEPSEEK",
            Provider::Custom => "CUSTOM",
        }
    }
}

// --- OpenAI-compatible API ---

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Deserialize)]
struct MessageContent {
    content: String,
}

// --- Anthropic (Claude) API ---

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    system: String,
    temperature: f32,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

pub async fn generate_commit_message(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let content = if config.provider.uses_cursor_cli() {
        call_cursor_cli(system_prompt, user_prompt).await?
    } else if config.provider.uses_anthropic_api() {
        call_anthropic(config, system_prompt, user_prompt).await?
    } else {
        call_openai_compatible(config, system_prompt, user_prompt).await?
    };

    Ok(clean_commit_message(&content))
}

async fn call_cursor_cli(system_prompt: &str, user_prompt: &str) -> Result<String> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let cursor_bin = find_cursor_cli()?;

    let prompt = format!(
        "{}\n\n---\n\n{}\n\nIMPORTANT: Output ONLY the raw commit message. No explanations, no markdown fences, no extra text.",
        system_prompt, user_prompt
    );

    let mut child = Command::new(&cursor_bin)
        .args(["agent", "--trust"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to launch Cursor CLI at '{}'", cursor_bin))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await?;
        stdin.shutdown().await?;
    }

    let output = child
        .wait_with_output()
        .await
        .context("Cursor agent process failed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Cursor agent exited with {}: {stderr}", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        bail!("Cursor agent returned empty output");
    }

    Ok(stdout)
}

fn find_cursor_cli() -> Result<String> {
    // Prefer auto-discovery from PATH first.
    if let Ok(output) = std::process::Command::new("which").arg("cursor").output() {
        if output.status.success() {
            let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !p.is_empty() {
                return Ok(p);
            }
        }
    }

    // Fallback: fixed macOS installation path.
    let path = std::path::Path::new(CURSOR_CLI_FALLBACK_PATH);
    if path.exists() {
        return Ok(CURSOR_CLI_FALLBACK_PATH.to_string());
    }

    bail!(
        "Cursor CLI not found in PATH or at fallback path '{CURSOR_CLI_FALLBACK_PATH}'.\n\
         Install Cursor from https://cursor.sh or switch to another provider:\n  \
         curgit --provider ollama"
    );
}

async fn call_openai_compatible(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/chat/completions",
        config.api_base.trim_end_matches('/')
    );

    let request = ChatRequest {
        model: config.model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ],
        temperature: 0.3,
    };

    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request);

    if let Some(key) = &config.api_key {
        req = req.header("Authorization", format!("Bearer {key}"));
    }

    let response = req
        .send()
        .await
        .context("Failed to connect to LLM API. Is the service running?")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("LLM API returned {status}: {body}");
    }

    let chat_response: ChatResponse = response
        .json()
        .await
        .context("Failed to parse LLM API response")?;

    chat_response
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .context("LLM returned empty response")
}

async fn call_anthropic(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/messages",
        config.api_base.trim_end_matches('/')
    );

    let api_key = config
        .api_key
        .as_deref()
        .context("Anthropic API key is required")?;

    let request = AnthropicRequest {
        model: config.model.clone(),
        max_tokens: 1024,
        system: system_prompt.to_string(),
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: user_prompt.to_string(),
        }],
        temperature: 0.3,
    };

    let response = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .context("Failed to connect to Anthropic API")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("Anthropic API returned {status}: {body}");
    }

    let anthropic_response: AnthropicResponse = response
        .json()
        .await
        .context("Failed to parse Anthropic API response")?;

    anthropic_response
        .content
        .first()
        .map(|c| c.text.trim().to_string())
        .context("Anthropic returned empty response")
}

fn clean_commit_message(msg: &str) -> String {
    let msg = msg.trim();

    let msg = if msg.starts_with("```") {
        let inner = msg
            .strip_prefix("```")
            .unwrap_or(msg)
            .trim_start_matches(|c: char| c.is_alphabetic() || c == '\n');
        inner.strip_suffix("```").unwrap_or(inner).trim()
    } else {
        msg
    };

    msg.to_string()
}
