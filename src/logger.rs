use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::config::APP_DIR;
use crate::provider::{ChatMessage, MessageRole};

const HISTORY_SUBDIR: &str = "history";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryFormat {
    Json,
    Markdown,
}

impl HistoryFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            HistoryFormat::Json => "json",
            HistoryFormat::Markdown => "md",
        }
    }
}

#[derive(Serialize)]
struct SerializableMessage<'a> {
    role: &'a str,
    content: &'a str,
}

pub fn save_history(
    path: &Path,
    format: HistoryFormat,
    system: Option<&str>,
    messages: &[ChatMessage],
) -> Result<()> {
    let payload = match format {
        HistoryFormat::Json => build_json_payload(system, messages)?,
        HistoryFormat::Markdown => render_markdown_payload(system, messages),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory {}", parent.display()))?;
    }
    fs::write(path, payload)
        .with_context(|| format!("failed to write log to {}", path.display()))?;
    Ok(())
}

pub fn default_history_dir() -> Option<PathBuf> {
    let base = dirs::data_local_dir().or_else(|| dirs::config_dir())?;
    Some(base.join(APP_DIR).join(HISTORY_SUBDIR))
}

pub fn timestamped_history_path(base_dir: &Path, provider: &str, format: HistoryFormat) -> PathBuf {
    timestamped_history_path_internal(base_dir, provider, format, Utc::now())
}

fn timestamped_history_path_internal(
    base_dir: &Path,
    provider: &str,
    format: HistoryFormat,
    now: DateTime<Utc>,
) -> PathBuf {
    let stamp = now.format("%Y%m%d-%H%M%S");
    let provider_chunk = sanitized_provider(provider);
    let filename = format!("{stamp}-{provider_chunk}.{}", format.extension());
    base_dir.join(filename)
}

fn sanitized_provider(provider: &str) -> String {
    let mut sanitized = provider
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' => c,
            'A'..='Z' => c.to_ascii_lowercase(),
            _ => '-',
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        sanitized.clear();
        sanitized.push_str("session");
        sanitized
    } else {
        trimmed.to_string()
    }
}

fn build_json_payload(system: Option<&str>, messages: &[ChatMessage]) -> Result<String> {
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
    Ok(json)
}

fn render_markdown_payload(system: Option<&str>, messages: &[ChatMessage]) -> String {
    let mut buf = String::with_capacity(128);
    buf.push_str("# Chat Transcript\n\n");
    if let Some(system_text) = system {
        append_markdown_entry(&mut buf, "system", system_text);
    }
    for message in messages {
        append_markdown_entry(&mut buf, &message.role.to_string(), &message.content);
    }
    buf
}

fn append_markdown_entry(buf: &mut String, role: &str, content: &str) {
    let _ = writeln!(buf, "## {role}\n\n{content}\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::Value;

    #[test]
    fn json_payload_includes_system() {
        let messages = vec![
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi there"),
        ];
        let json = build_json_payload(Some("Stay helpful"), &messages).expect("json payload");
        let value: Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(value[0]["role"], "system");
        assert_eq!(value[1]["role"], "user");
        assert_eq!(value[2]["role"], "assistant");
    }

    #[test]
    fn markdown_payload_captures_roles() {
        let messages = vec![
            ChatMessage::user("Ping"),
            ChatMessage::assistant("Pong"),
        ];
        let md = render_markdown_payload(None, &messages);
        assert!(md.contains("## user"));
        assert!(md.contains("## assistant"));
        assert!(md.contains("Pong"));
    }

    #[test]
    fn timestamped_path_is_deterministic() {
        let base = PathBuf::from("/tmp/history");
        let now = Utc
            .with_ymd_and_hms(2024, 5, 1, 12, 30, 45)
            .single()
            .expect("valid timestamp");
        let path = timestamped_history_path_internal(
            &base,
            "Prod#Provider",
            HistoryFormat::Markdown,
            now,
        );
        assert_eq!(
            path.file_name().unwrap().to_str().unwrap(),
            "20240501-123045-prod-provider.md"
        );
    }
}
