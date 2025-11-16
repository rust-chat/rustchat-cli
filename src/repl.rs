use std::path::PathBuf;

use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::logger;
use crate::provider::{ChatMessage, ChatRequestOptions, DynProvider};

pub struct ReplOptions {
    pub model: String,
    pub system: Option<String>,
    pub save_path: Option<PathBuf>,
    pub request_options: ChatRequestOptions,
    pub stream: bool,
}

pub async fn run_chat_repl(provider: DynProvider, opts: ReplOptions) -> Result<()> {
    if opts.stream {
        eprintln!("[info] streaming not yet supported; falling back to blocking responses");
    }

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

                let response = provider
                    .chat(&opts.model, opts.system.as_deref(), &messages, &opts.request_options)
                    .await?;
                println!("bot> {response}");
                messages.push(ChatMessage::assistant(response));
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(err) => return Err(err.into()),
        }
    }

    if let Some(path) = opts.save_path.as_ref() {
        logger::save_history(path, opts.system.as_deref(), &messages)?;
        println!("[saved chat history to {}]", path.display());
    }

    Ok(())
}
