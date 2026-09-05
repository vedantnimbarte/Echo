use serde::{Deserialize, Serialize};

pub mod learn;

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

/// Cap on the initial prompt handed to whisper.
///
/// whisper.cpp accepts `n_text_ctx/2` tokens (~224 on the standard models) and
/// silently drops the overflow, so a long dictionary would quietly lose the
/// entries at one end. ~800 characters is a comfortable margin at the ~4
/// chars/token that short English terms average.
const MAX_PROMPT_CHARS: usize = 800;

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

    /// Vocabulary hint for a local whisper decoder, or `None` when there is
    /// nothing worth hinting.
    ///
    /// This is the other half of the dictionary, and the better half:
    /// [`Self::process_for`] repairs a mishearing *after* the fact, which can
    /// only fix mistakes that happen to map cleanly back onto the phrase.
    /// Feeding the same vocabulary to whisper as an initial prompt biases the
    /// decoder toward those spellings while it is still choosing tokens, so the
    /// word tends to come out right the first time. The two compose: the prompt
    /// reduces how often the repair pass is needed, and the repair pass still
    /// catches what the prompt misses.
    ///
    /// The *replacements* are what gets hinted — those are the spellings we
    /// want produced. Entries scoped to another profile are left out so the
    /// hint matches the replacements that will actually be applied.
    pub fn prompt_terms(&self, profile: Option<i64>) -> Option<String> {
        let mut prompt = String::new();
        let mut seen: Vec<&str> = Vec::new();

        for entry in &self.entries {
            if !entry.enabled {
                continue;
            }
            if entry.profile_id.is_some() && entry.profile_id != profile {
                continue;
            }
            let term = entry.replacement.trim();
            // A blank replacement is a deletion rule, and hinting a duplicate
            // spends prompt budget without adding any bias.
            if term.is_empty() || seen.contains(&term) {
                continue;
            }
            // Budget check before the push, so the prompt never overruns.
            let addition = if prompt.is_empty() { term.len() } else { term.len() + 2 };
            if prompt.len() + addition > MAX_PROMPT_CHARS {
                break;
            }
            if !prompt.is_empty() {
                prompt.push_str(", ");
            }
            prompt.push_str(term);
            seen.push(term);
        }

        (!prompt.is_empty()).then_some(prompt)
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
    fn prompt_hints_the_replacement_not_the_mishearing() {
        // The point of the hint is to make whisper produce "Kubernetes"; hinting
        // "cooper netties" would bias it toward the error we are trying to fix.
        let e = DictionaryEngine::new(vec![entry("cooper netties", "Kubernetes", None)]);
        assert_eq!(e.prompt_terms(None).as_deref(), Some("Kubernetes"));
    }

    #[test]
    fn prompt_is_scoped_the_same_way_replacements_are() {
        let e = DictionaryEngine::new(vec![
            entry("k8s", "Kubernetes", None),
            entry("pr", "pull request", Some(1)),
        ]);
        // Only the global term when no profile is active...
        assert_eq!(e.prompt_terms(None).as_deref(), Some("Kubernetes"));
        // ...and both once the profile matches, so the hint agrees with the
        // replacements that will actually run.
        assert_eq!(
            e.prompt_terms(Some(1)).as_deref(),
            Some("Kubernetes, pull request")
        );
    }

    #[test]
    fn empty_dictionary_produces_no_prompt() {
        assert_eq!(DictionaryEngine::new(vec![]).prompt_terms(None), None);
        // A disabled entry is not vocabulary either.
        let mut disabled = entry("k8s", "Kubernetes", None);
        disabled.enabled = false;
        assert_eq!(DictionaryEngine::new(vec![disabled]).prompt_terms(None), None);
    }

    #[test]
    fn prompt_stays_within_whispers_context_budget() {
        // Overrunning it makes whisper.cpp silently drop the head of the
        // prompt, so the cap has to hold however many entries exist.
        let entries: Vec<_> = (0..500)
            .map(|i| entry(&format!("p{i}"), &format!("replacement-number-{i}"), None))
            .collect();
        let prompt = DictionaryEngine::new(entries).prompt_terms(None).unwrap();
        assert!(prompt.len() <= MAX_PROMPT_CHARS, "len was {}", prompt.len());
    }

    #[test]
    fn duplicate_replacements_are_hinted_once() {
        let e = DictionaryEngine::new(vec![
            entry("k8s", "Kubernetes", None),
            entry("kube", "Kubernetes", None),
        ]);
        assert_eq!(e.prompt_terms(None).as_deref(), Some("Kubernetes"));
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
