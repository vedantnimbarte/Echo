//! Command mode: speak an instruction instead of dictating text.
//!
//! A final transcript that starts with the configured prefix word ("command
//! make this more formal") is routed here instead of being injected verbatim.
//! If the focused app has a selection, the instruction is applied to it and the
//! result replaces it; otherwise the answer is inserted at the cursor.
//!
//! The backend defaults to a local Ollama server so selected text never leaves
//! the machine — the cloud path is opt-in and reuses the OpenAI key already in
//! the keychain.

use serde_json::json;

use crate::error::{EchoError, Result};

/// How command mode is configured, read from settings per utterance so changes
/// take effect without a restart.
#[derive(Debug, Clone)]
pub struct CommandConfig {
    pub enabled: bool,
    /// Word that marks a transcript as an instruction rather than dictation.
    pub prefix: String,
    /// `"ollama"` (local, default) or `"openai"`.
    pub provider: String,
    pub model: String,
    /// Base URL of the local Ollama server.
    pub endpoint: String,
}

impl Default for CommandConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            prefix: "command".into(),
            provider: "ollama".into(),
            model: "llama3.2".into(),
            endpoint: "http://localhost:11434".into(),
        }
    }
}

/// True for characters that may sit between the prefix word and the
/// instruction — ASR output often punctuates ("Command, make this formal.").
fn is_separator(c: char) -> bool {
    c.is_whitespace() || c.is_ascii_punctuation()
}

/// Extract the instruction from a transcript that opens with `prefix`.
///
/// Returns `None` when the transcript is ordinary dictation, so the caller
/// falls through to normal text injection. The prefix must be followed by a
/// separator, so dictating "commander" is not mistaken for a command.
///
/// Matching is ASCII-case-insensitive, which covers the intended English
/// trigger words; a non-ASCII prefix must be spoken with matching case.
pub fn parse_command<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return None;
    }

    let start = text.trim_start_matches(is_separator);
    // `get` returns None on a non-char-boundary, so this is slice-safe.
    let head = start.get(..prefix.len())?;
    if !head.eq_ignore_ascii_case(prefix) {
        return None;
    }

    let rest = &start[prefix.len()..];
    if !rest.is_empty() && !rest.starts_with(is_separator) {
        return None;
    }

    let instruction = rest.trim_start_matches(is_separator).trim_end();
    (!instruction.is_empty()).then_some(instruction)
}

/// Build the system/user prompt pair for an instruction, with or without a
/// selection to operate on.
fn prompt(instruction: &str, selection: Option<&str>) -> (String, String) {
    match selection {
        Some(sel) if !sel.trim().is_empty() => (
            "You edit text. Apply the user's instruction to the text below and reply with \
             ONLY the resulting text — no preamble, no quotes, no explanation."
                .to_string(),
            format!("Instruction: {instruction}\n\nText:\n{sel}"),
        ),
        _ => (
            "You are a concise assistant embedded in a text field. Reply with ONLY the text \
             to insert — no preamble, no explanation."
                .to_string(),
            instruction.to_string(),
        ),
    }
}

/// Run an instruction through the configured LLM and return the text to inject.
pub async fn run(
    cfg: &CommandConfig,
    api_key: Option<&str>,
    instruction: &str,
    selection: Option<&str>,
) -> Result<String> {
    let (system, user) = prompt(instruction, selection);
    let messages = json!([
        { "role": "system", "content": system },
        { "role": "user", "content": user },
    ]);

    let client = reqwest::Client::new();
    // The URL is carried out of the match so the egress log can name the host
    // that was actually contacted.
    let (request, pointer, endpoint) = match cfg.provider.as_str() {
        "openai" => {
            let key = api_key.ok_or_else(|| {
                EchoError::Config(
                    "Command mode is set to OpenAI but no API key is stored".into(),
                )
            })?;
            let url = "https://api.openai.com/v1/chat/completions".to_string();
            (
                client
                    .post(&url)
                    .bearer_auth(key)
                    .json(&json!({ "model": cfg.model, "messages": messages })),
                "/choices/0/message/content",
                url,
            )
        }
        "ollama" => {
            let url = format!("{}/api/chat", cfg.endpoint.trim_end_matches('/'));
            (
                client
                    .post(&url)
                    .json(&json!({
                        "model": cfg.model,
                        "messages": messages,
                        "stream": false,
                    })),
                "/message/content",
                url,
            )
        }
        other => {
            return Err(EchoError::NotFound(format!(
                "Unknown command-mode provider '{other}'"
            )))
        }
    };

    crate::core::egress::record(&endpoint, "command mode");

    let resp = request.send().await.map_err(|e| {
        if cfg.provider == "ollama" && e.is_connect() {
            EchoError::Config(format!(
                "Could not reach Ollama at {}. Start it with `ollama serve`, or switch \
                 command mode to OpenAI in Settings.",
                cfg.endpoint
            ))
        } else {
            EchoError::AsrProvider(e.to_string())
        }
    })?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| EchoError::AsrProvider(format!("command response: {e}")))?;

    if !status.is_success() {
        let detail = body
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("error").and_then(|v| v.as_str()))
            .unwrap_or("unknown error");
        return Err(EchoError::AsrProvider(format!(
            "Command mode failed ({status}): {detail}"
        )));
    }

    body.pointer(pointer)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| EchoError::AsrProvider("Command mode returned no text".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_dictation_is_not_a_command() {
        assert_eq!(parse_command("make this more formal", "command"), None);
        // A word that merely starts with the prefix must not trigger.
        assert_eq!(parse_command("commander of the fleet", "command"), None);
        // The prefix alone carries no instruction.
        assert_eq!(parse_command("command", "command"), None);
        assert_eq!(parse_command("Command.", "command"), None);
    }

    #[test]
    fn prefix_is_stripped_from_the_instruction() {
        assert_eq!(
            parse_command("command make this more formal", "command"),
            Some("make this more formal")
        );
        // ASR punctuates and capitalises; both are tolerated.
        assert_eq!(
            parse_command("Command, summarise this.", "command"),
            Some("summarise this.")
        );
        assert_eq!(
            parse_command("  COMMAND: translate to French  ", "command"),
            Some("translate to French")
        );
    }

    #[test]
    fn an_empty_prefix_never_matches() {
        // Guards against a blank setting turning every transcript into a command.
        assert_eq!(parse_command("anything at all", ""), None);
        assert_eq!(parse_command("anything at all", "   "), None);
    }

    #[test]
    fn multibyte_transcripts_do_not_panic() {
        // `get(..len)` must reject a non-char-boundary rather than slicing it.
        assert_eq!(parse_command("émigré story", "com"), None);
    }
}
