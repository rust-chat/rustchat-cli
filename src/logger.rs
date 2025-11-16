use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::provider::{ChatMessage, MessageRole};

#[derive(Serialize)]
struct SerializableMessage<'a> {
    role: &'a str,
    content: &'a str,
}

pub fn save_history(path: &Path, system: Option<&str>, messages: &[ChatMessage]) -> Result<()> {
    let serializable: Vec<SerializableMessage<'_>> = messages
        .iter()
        .map(|message| SerializableMessage {
            role: match message.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
            },
            content: message.content.as_str(),
        })
        .collect();

    let mut payload = serde_json::to_value(&serializable)?;
    if let (serde_json::Value::Array(array), Some(system_text)) = (&mut payload, system) {
        array.insert(
            0,
            serde_json::json!({
                "role": "system",
                "content": system_text,
            }),
        );
    }
    let json = serde_json::to_string_pretty(&payload)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory {}", parent.display()))?;
    }
    fs::write(path, json).with_context(|| format!("failed to write log to {}", path.display()))?;
    Ok(())
}
