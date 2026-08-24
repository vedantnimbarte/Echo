use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub id: Option<i64>,
    pub phrase: String,
    pub replacement: String,
    pub enabled: bool,
    /// `None` = applies everywhere. `Some(id)` = only while a per-app profile
    /// pointing at that dictionary profile is active.
    pub profile_id: Option<i64>,
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
    ///
    /// Global entries (`profile_id == None`) always apply; an entry scoped to a
    /// profile applies only when `profile` matches its own.
    pub fn process_for(&self, text: &str, profile: Option<i64>) -> String {
        let normalized = text.trim().to_string();
        self.apply_replacements(normalized, profile)
    }

    fn apply_replacements(&self, mut text: String, profile: Option<i64>) -> String {
        for entry in &self.entries {
            if !entry.enabled {
                continue;
            }
            if entry.profile_id.is_some() && entry.profile_id != profile {
                continue;
            }
            // Case-insensitive whole-phrase match using a simple replace.
            let lower_text = text.to_lowercase();
            let lower_phrase = entry.phrase.to_lowercase();
            if let Some(pos) = lower_text.find(&lower_phrase) {
                text = format!(
                    "{}{}{}",
                    &text[..pos],
                    entry.replacement,
                    &text[pos + entry.phrase.len()..]
                );
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(phrase: &str, replacement: &str, profile_id: Option<i64>) -> DictionaryEntry {
        DictionaryEntry {
            id: None,
            phrase: phrase.into(),
            replacement: replacement.into(),
            enabled: true,
            profile_id,
        }
    }

    #[test]
    fn global_entries_apply_everywhere() {
        let e = DictionaryEngine::new(vec![entry("teh", "the", None)]);
        assert_eq!(e.process_for("teh cat", None), "the cat");
        assert_eq!(e.process_for("teh cat", Some(1)), "the cat");
    }

    #[test]
    fn scoped_entries_only_apply_to_their_profile() {
        let e = DictionaryEngine::new(vec![entry("ship it", "SHIP IT", Some(1))]);
        // Wrong profile, and no profile, both leave the text alone.
        assert_eq!(e.process_for("ship it", None), "ship it");
        assert_eq!(e.process_for("ship it", Some(2)), "ship it");
        assert_eq!(e.process_for("ship it", Some(1)), "SHIP IT");
    }

    #[test]
    fn disabled_entries_never_apply() {
        let mut d = entry("teh", "the", None);
        d.enabled = false;
        let e = DictionaryEngine::new(vec![d]);
        assert_eq!(e.process_for("teh cat", None), "teh cat");
    }
}
