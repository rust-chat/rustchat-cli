use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use futures::StreamExt;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::logger::{self, HistoryFormat};
use crate::provider::{ChatMessage, ChatRequestOptions, DynProvider};

pub struct ReplOptions {
    pub provider_name: String,
    pub model: String,
    pub system: Option<String>,
    pub save_path: Option<PathBuf>,
    pub history_dir: Option<PathBuf>,
    pub auto_save: bool,
    pub save_format: HistoryFormat,
    pub request_options: ChatRequestOptions,
    pub stream: bool,
}

pub async fn run_chat_repl(provider: DynProvider, opts: ReplOptions) -> Result<()> {
    println!("Type /reset to clear history, blank line to exit.");

    let mut rl = DefaultEditor::new().context("failed to start line editor")?;
    let mut messages: Vec<ChatMessage> = Vec::new();

    loop {
        match rl.readline("you> ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    break;
                }
                if trimmed == "/reset" {
                    messages.clear();
                    println!("[history reset]");
                    continue;
                }

                rl.add_history_entry(trimmed).ok();
                messages.push(ChatMessage::user(line.clone()));

                if opts.stream {
                    let mut stream = provider
                        .stream_chat(
                            &opts.model,
                            opts.system.as_deref(),
                            &messages,
                            &opts.request_options,
                        )
                        .await?;
                    print!("bot> ");
                    io::stdout().flush().ok();
                    let mut assistant_response = String::new();
                    while let Some(chunk) = stream.next().await {
                        let token = chunk?;
                        print!("{token}");
                        io::stdout().flush().ok();
                        assistant_response.push_str(&token);
                    }
                    println!();
                    messages.push(ChatMessage::assistant(assistant_response));
                } else {
                    let response = provider
                        .chat(
                            &opts.model,
                            opts.system.as_deref(),
                            &messages,
                            &opts.request_options,
                        )
                        .await?;
                    println!("bot> {response}");
                    messages.push(ChatMessage::assistant(response));
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(err) => return Err(err.into()),
        }
    }

    match resolve_history_target(&opts) {
        Some(path) => {
            logger::save_history(&path, opts.save_format, opts.system.as_deref(), &messages)?;
            println!("[saved chat history to {}]", path.display());
        }
        None if opts.auto_save => {
            eprintln!("[warn] auto-save requested but no history directory is available");
        }
        _ => {}
    }

    Ok(())
}

fn resolve_history_target(opts: &ReplOptions) -> Option<PathBuf> {
    if let Some(path) = opts.save_path.as_ref() {
        return Some(path.clone());
    }
    if opts.auto_save {
        if let Some(dir) = opts.history_dir.as_ref() {
            return Some(logger::timestamped_history_path(
                dir,
                &opts.provider_name,
                opts.save_format,
            ));
        }
    }
    None
}
