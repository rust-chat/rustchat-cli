mod cli;
mod config;
mod logger;
mod provider;
mod repl;
mod streaming;
mod utils;

use anyhow::{anyhow, Result};
use clap::Parser;

use crate::cli::{ChatCommand, Cli, Commands, ConfigCommand, MessageCommand};
use crate::config::{build_provider_config, AppConfig, ProviderKind};
use crate::provider::{build_provider, ChatMessage, ChatRequestOptions};

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
    let provider = build_provider(&provider_name, provider_cfg).await?;
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
    repl::run_chat_repl(
        provider,
        repl::ReplOptions {
            model,
            system: args.common.system.clone(),
            save_path: args.common.save_path.clone(),
            request_options,
            stream: args.stream,
        },
    )
    .await
}

async fn run_message(args: MessageCommand, cfg: &AppConfig) -> Result<()> {
    let provider_name = cfg.infer_default_provider(&args.common.provider)?;
    let provider_cfg = cfg.require_provider(&provider_name)?;
    let provider = build_provider(&provider_name, provider_cfg).await?;
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
        .chat(&model, args.common.system.as_deref(), &messages, &request_options)
        .await?;
    println!("{response}");
    messages.push(ChatMessage::assistant(response.clone()));

    if let Some(path) = args.common.save_path.as_ref() {
        logger::save_history(path, args.common.system.as_deref(), &messages)?;
        println!("[saved chat history to {}]", path.display());
    }

    Ok(())
}
