use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "rustchat-cli",
    version,
    author,
    about = "Multi-provider AI chat CLI"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage configuration files
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Start an interactive chat session (REPL)
    Chat(ChatCommand),
    /// Send a single message and print the response
    Message(MessageCommand),
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Persist provider credentials and defaults
    Set(ConfigSetArgs),
    /// Print the active configuration
    Show,
    /// Remove a provider entry
    Remove {
        /// Provider name to remove
        provider: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ProviderKindArg {
    Google,
    Anthropic,
    Openai,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum SaveFormatArg {
    Json,
    Markdown,
}

impl ProviderKindArg {
    pub fn infer_from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "google" => Some(Self::Google),
            "anthropic" => Some(Self::Anthropic),
            "openai" => Some(Self::Openai),
            _ => None,
        }
    }
}

#[derive(Args, Debug)]
pub struct ConfigSetArgs {
    /// Unique provider label (e.g. google, work-google, openai)
    pub provider: String,
    /// Provider kind, defaults to name-based inference
    #[arg(long = "kind", value_enum)]
    pub provider_kind: Option<ProviderKindArg>,
    /// Mark this provider as the default for chat/message commands
    #[arg(long)]
    pub default: bool,
    #[command(flatten)]
    pub google: GoogleSetArgs,
    #[command(flatten)]
    pub shared_api: ApiKeySetArgs,
}

#[derive(Args, Debug, Default, Clone)]
pub struct GoogleSetArgs {
    /// Path to Google service account JSON file
    #[arg(long = "service-account")]
    pub service_account: Option<PathBuf>,
    /// GCP project identifier (optional)
    #[arg(long = "project-id")]
    pub project_id: Option<String>,
    /// Regional endpoint / location hint
    #[arg(long)]
    pub location: Option<String>,
    /// Default model for this provider (e.g. gemini-pro)
    #[arg(long = "default-model")]
    pub default_model: Option<String>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct ApiKeySetArgs {
    /// API key / bearer token used by providers that require it
    #[arg(long = "api-key")]
    pub api_key: Option<String>,
    /// Custom base URL (Anthropic / OpenAI enterprise deployments)
    #[arg(long = "base-url")]
    pub base_url: Option<String>,
    /// Optional default model override
    #[arg(long = "shared-default-model")]
    pub shared_default_model: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct CommonChatArgs {
    /// Provider to use. Falls back to config default when omitted
    #[arg(short, long)]
    pub provider: Option<String>,
    /// Model identifier (e.g. gemini-pro)
    #[arg(short, long)]
    pub model: Option<String>,
    /// Optional system prompt / persona
    #[arg(long)]
    pub system: Option<String>,
    /// Path to save chat history (respects --save-format). When omitted, no persistence
    #[arg(long = "save")]
    pub save_path: Option<PathBuf>,
    /// Directory used when --auto-save is enabled
    #[arg(long = "history-dir")]
    pub history_dir: Option<PathBuf>,
    /// Automatically write each session to a timestamped file under --history-dir
    #[arg(long = "auto-save")]
    pub auto_save: bool,
    /// File format to use for history exports
    #[arg(long = "save-format", value_enum, default_value_t = SaveFormatArg::Json)]
    pub save_format: SaveFormatArg,
    /// Optional temperature override
    #[arg(long)]
    pub temperature: Option<f32>,
    /// Optional max output tokens
    #[arg(long = "max-tokens")]
    pub max_output_tokens: Option<u32>,
}

#[derive(Args, Debug)]
pub struct ChatCommand {
    #[command(flatten)]
    pub common: CommonChatArgs,
    /// Enable streaming output (MVP+ placeholder)
    #[arg(long)]
    pub stream: bool,
}

#[derive(Args, Debug)]
pub struct MessageCommand {
    #[command(flatten)]
    pub common: CommonChatArgs,
    /// Prompt text to send
    #[arg(required = true)]
    pub prompt: Vec<String>,
}
