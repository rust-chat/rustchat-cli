mod cli;
mod config;
mod logger;
mod provider;
mod repl;
mod secrets;
mod streaming;
mod utils;

use anyhow::{anyhow, Result};
use clap::Parser;

use crate::cli::{
    ChatCommand, Cli, Commands, CommonChatArgs, ConfigCommand, MessageCommand, SaveFormatArg,
};
use crate::config::{build_provider_config, AppConfig, ProviderKind};
use crate::logger as history_logger;
use crate::logger::HistoryFormat;
use crate::provider::{build_provider, ChatMessage, ChatRequestOptions};
use crate::secrets::{optional_passphrase_from_env, DEFAULT_MASTER_ENV};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut app_config = match AppConfig::load() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("[warn] failed to load config: {err:#}. Starting with empty config.");
            AppConfig::default()
        }
    };

    match cli.command {
        Commands::Config { command } => handle_config(command, &mut app_config).await?,
        Commands::Chat(args) => run_chat(args, &app_config).await?,
        Commands::Message(args) => run_message(args, &app_config).await?,
    }

    Ok(())
}

async fn handle_config(cmd: ConfigCommand, cfg: &mut AppConfig) -> Result<()> {
    match cmd {
        ConfigCommand::Set(args) => {
            let kind = args
                .provider_kind
                .map(ProviderKind::from)
                .or_else(|| ProviderKind::infer(&args.provider))
                .ok_or_else(|| anyhow!("unable to infer provider kind - use --kind"))?;
            let provider_cfg = build_provider_config(kind, &args)?;
            cfg.upsert_provider(args.provider.clone(), provider_cfg);
            if args.default {
                cfg.default_provider = Some(args.provider.clone());
            }
            cfg.save()?;
            if let Ok(path) = config::config_path() {
                let _ = config::ensure_permissions(&path);
            }
            println!("Saved provider '{}'", args.provider);
        }
        ConfigCommand::Show => {
            let serialized = toml::to_string_pretty(cfg)?;
            println!("{serialized}");
        }
        ConfigCommand::Remove { provider } => {
            if cfg.remove_provider(&provider) {
                if cfg.default_provider.as_deref() == Some(provider.as_str()) {
                    cfg.default_provider = None;
                }
                cfg.save()?;
                println!("Removed provider '{provider}'");
            } else {
                println!("Provider '{provider}' not found");
            }
        }
    }
    Ok(())
}

async fn run_chat(args: ChatCommand, cfg: &AppConfig) -> Result<()> {
    let provider_name = cfg.infer_default_provider(&args.common.provider)?;
    let provider_cfg = cfg.require_provider(&provider_name)?;
    let env_label = args
        .common
        .secret_env
        .as_deref()
        .unwrap_or(DEFAULT_MASTER_ENV);
    let passphrase =
        optional_passphrase_from_env(env_label, args.common.secret_env.is_some())?;
    let provider = build_provider(
        &provider_name,
        provider_cfg,
        passphrase.as_deref(),
        env_label,
    )
    .await?;
    let model = args
        .common
        .model
        .clone()
        .or_else(|| provider_cfg.default_model().map(|m| m.to_string()))
        .unwrap_or_else(|| "gemini-pro".to_string());
    let request_options = ChatRequestOptions {
        temperature: args.common.temperature,
        max_output_tokens: args.common.max_output_tokens,
    };
    let history = build_history_config(&args.common);
    if history.auto_save_request_failed {
        eprintln!("[warn] auto-save requested but no history directory is available");
    }

    repl::run_chat_repl(
        provider,
        repl::ReplOptions {
            provider_name,
            model,
            system: args.common.system.clone(),
            save_path: history.explicit_path.clone(),
            history_dir: history.history_dir.clone(),
            auto_save: history.auto_save,
            save_format: history.format,
            webhook_url: args.common.webhook_url.clone(),
            request_options,
            stream: args.stream,
        },
    )
    .await
}

async fn run_message(args: MessageCommand, cfg: &AppConfig) -> Result<()> {
    let provider_name = cfg.infer_default_provider(&args.common.provider)?;
    let provider_cfg = cfg.require_provider(&provider_name)?;
    let env_label = args
        .common
        .secret_env
        .as_deref()
        .unwrap_or(DEFAULT_MASTER_ENV);
    let passphrase =
        optional_passphrase_from_env(env_label, args.common.secret_env.is_some())?;
    let provider = build_provider(
        &provider_name,
        provider_cfg,
        passphrase.as_deref(),
        env_label,
    )
    .await?;
    let model = args
        .common
        .model
        .clone()
        .or_else(|| provider_cfg.default_model().map(|m| m.to_string()))
        .unwrap_or_else(|| "gemini-pro".to_string());
    let request_options = ChatRequestOptions {
        temperature: args.common.temperature,
        max_output_tokens: args.common.max_output_tokens,
    };
    let prompt = args.prompt.join(" ");
    let mut messages = vec![ChatMessage::user(prompt.clone())];
    let response = provider
        .chat(
            &model,
            args.common.system.as_deref(),
            &messages,
            &request_options,
        )
        .await?;
    println!("{response}");
    messages.push(ChatMessage::assistant(response.clone()));

    let history = build_history_config(&args.common);
    if let Some(path) = history.resolve_path(&provider_name) {
        history_logger::save_history(
            &path,
            history.format,
            args.common.system.as_deref(),
            &messages,
        )?;
        println!("[saved chat history to {}]", path.display());
    } else if history.auto_save_request_failed {
        eprintln!("[warn] auto-save requested but no history directory is available");
    }

    if let Some(url) = args.common.webhook_url.as_deref() {
        if let Err(err) = history_logger::send_history_webhook(
            url,
            history.format,
            args.common.system.as_deref(),
            &messages,
        )
        .await
        {
            eprintln!("[warn] failed to POST chat history: {err:#}");
        } else {
            println!("[pushed chat history to webhook]");
        }
    }

    Ok(())
}

struct HistoryConfig {
    explicit_path: Option<std::path::PathBuf>,
    history_dir: Option<std::path::PathBuf>,
    auto_save: bool,
    format: HistoryFormat,
    auto_save_request_failed: bool,
}

impl HistoryConfig {
    fn resolve_path(&self, provider_name: &str) -> Option<std::path::PathBuf> {
        if let Some(path) = &self.explicit_path {
            return Some(path.clone());
        }
        if self.auto_save {
            if let Some(dir) = &self.history_dir {
                return Some(history_logger::timestamped_history_path(
                    dir,
                    provider_name,
                    self.format,
                ));
            }
        }
        None
    }
}

fn build_history_config(args: &CommonChatArgs) -> HistoryConfig {
    let format = match args.save_format {
        SaveFormatArg::Json => HistoryFormat::Json,
        SaveFormatArg::Markdown => HistoryFormat::Markdown,
    };
    let history_dir = args
        .history_dir
        .clone()
        .or_else(|| history_logger::default_history_dir());
    let mut auto_save = args.auto_save;
    let mut auto_save_request_failed = false;
    if auto_save && history_dir.is_none() {
        auto_save = false;
        auto_save_request_failed = true;
    }
    HistoryConfig {
        explicit_path: args.save_path.clone(),
        history_dir,
        auto_save,
        format,
        auto_save_request_failed,
    }
}
