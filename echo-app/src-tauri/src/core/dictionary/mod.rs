use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub id: Option<i64>,
    pub phrase: String,
    pub replacement: String,
    pub enabled: bool,
}

/// Applies dictionary replacements to a transcript.
pub struct DictionaryEngine {
    entries: Vec<DictionaryEntry>,
}

impl DictionaryEngine {
    pub fn new(entries: Vec<DictionaryEntry>) -> Self {
        Self { entries }
    }

    pub fn update_entries(&mut self, entries: Vec<DictionaryEntry>) {
        self.entries = entries;
    }

    /// Normalize → replace → return processed text.
    pub fn process(&self, text: &str) -> String {
        let normalized = text.trim().to_string();
        self.apply_replacements(normalized)
    }

    fn apply_replacements(&self, mut text: String) -> String {
        for entry in &self.entries {
            if !entry.enabled {
                continue;
            }
            // Case-insensitive whole-phrase match using a simple replace.
            let lower_text = text.to_lowercase();
            let lower_phrase = entry.phrase.to_lowercase();
            if let Some(pos) = lower_text.find(&lower_phrase) {
                text = format!(
                    "{}{}{}",
                    &text[..pos],
                    &entry.replacement,
                    &text[pos + entry.phrase.len()..]
                );
            }
        }
        text
    }
}
