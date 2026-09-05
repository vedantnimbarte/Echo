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

    fn apply_replacements(&self, text: String, profile: Option<i64>) -> String {
        let mut text = text;
        for entry in &self.entries {
            if !entry.enabled {
                continue;
            }
            if entry.profile_id.is_some() && entry.profile_id != profile {
                continue;
            }
            text = replace_all_ci(&text, &entry.phrase, &entry.replacement);
        }
        text
    }
}

/// Replace every case-insensitive occurrence of `phrase` in `text`.
///
/// The obvious implementation — search `text.to_lowercase()` and slice `text`
/// at the offset it returns — is wrong twice over, and both ways bite only on
/// input the author is unlikely to type:
///
/// 1. Lowercasing can change a string's *length*. `İ` is two bytes but
///    lowercases to three, so every offset past it is shifted; slicing the
///    original at one lands mid-character or off the end and panics. Since
///    this runs inside the transcript task, that panic took the transcript
///    with it and showed the user nothing.
/// 2. `str::find` returns the first match only, so "teh cat and teh dog"
///    kept the second "teh" — a replacement rule that fixes one of two
///    identical mistakes.
///
/// So the match is done against the original text, comparing lowercased
/// characters, and the range it reports is always a valid range *of the
/// original*.
fn replace_all_ci(text: &str, phrase: &str, replacement: &str) -> String {
    // An empty phrase matches everywhere and would never advance the cursor.
    if phrase.is_empty() {
        return text.to_string();
    }
    let phrase_lower: Vec<char> = phrase.chars().flat_map(char::to_lowercase).collect();

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;

    while let Some((start, end)) = find_from(text, &phrase_lower, cursor) {
        out.push_str(&text[cursor..start]);
        out.push_str(replacement);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Byte range of the next case-insensitive match at or after `from`.
fn find_from(text: &str, phrase_lower: &[char], from: usize) -> Option<(usize, usize)> {
    if from > text.len() {
        return None;
    }
    text[from..]
        .char_indices()
        .find_map(|(offset, _)| {
            let start = from + offset;
            match_at(text, start, phrase_lower).map(|end| (start, end))
        })
}

/// If `phrase_lower` matches `text` at `start`, the byte offset just past it.
fn match_at(text: &str, start: usize, phrase_lower: &[char]) -> Option<usize> {
    let mut matched = 0usize;
    let mut end = start;

    for c in text[start..].chars() {
        if matched == phrase_lower.len() {
            break;
        }
        // One source character can lowercase to several. If the phrase runs
        // out partway through one, the match would have to end mid-character —
        // there is no range that expresses that, so it is not a match.
        for lowered in c.to_lowercase() {
            if matched >= phrase_lower.len() || phrase_lower[matched] != lowered {
                return None;
            }
            matched += 1;
        }
        end += c.len_utf8();
    }

    (matched == phrase_lower.len()).then_some(end)
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

    /// Regression: this panicked with "start byte index 7 is out of bounds for
    /// string of length 6". `İ` occupies two bytes and lowercases to three, so
    /// an offset found in a lowercased copy does not address the original.
    #[test]
    fn a_character_that_grows_when_lowercased_does_not_panic() {
        let e = DictionaryEngine::new(vec![entry("teh", "the", None)]);
        assert_eq!(e.process_for("\u{130} teh", None), "\u{130} the");
    }

    /// Regression: only the first occurrence used to be replaced.
    #[test]
    fn every_occurrence_is_replaced() {
        let e = DictionaryEngine::new(vec![entry("teh", "the", None)]);
        assert_eq!(
            e.process_for("teh cat and teh dog and teh bird", None),
            "the cat and the dog and the bird"
        );
    }

    #[test]
    fn matching_ignores_case_but_the_replacement_is_verbatim() {
        let e = DictionaryEngine::new(vec![entry("github", "GitHub", None)]);
        assert_eq!(
            e.process_for("GITHUB and GitHub and github", None),
            "GitHub and GitHub and GitHub"
        );
    }

    #[test]
    fn adjacent_matches_are_both_replaced() {
        let e = DictionaryEngine::new(vec![entry("ab", "X", None)]);
        assert_eq!(e.process_for("ababab", None), "XXX");
    }

    /// A replacement that contains its own phrase must not be rescanned, or the
    /// loop would never terminate.
    #[test]
    fn a_self_containing_replacement_terminates() {
        let e = DictionaryEngine::new(vec![entry("the", "the very", None)]);
        assert_eq!(e.process_for("the cat", None), "the very cat");
    }

    /// An empty phrase matches at every position and would never advance.
    #[test]
    fn an_empty_phrase_leaves_the_text_alone() {
        let e = DictionaryEngine::new(vec![entry("", "X", None)]);
        assert_eq!(e.process_for("unchanged", None), "unchanged");
    }

    #[test]
    fn multibyte_text_around_a_match_survives_intact() {
        let e = DictionaryEngine::new(vec![entry("cafe", "caf\u{e9}", None)]);
        assert_eq!(
            e.process_for("\u{4f60}\u{597d} cafe \u{1f600} cafe", None),
            "\u{4f60}\u{597d} caf\u{e9} \u{1f600} caf\u{e9}"
        );
    }

    /// The phrase itself may be multi-byte.
    #[test]
    fn a_multibyte_phrase_is_matched_and_replaced() {
        let e = DictionaryEngine::new(vec![entry("\u{e9}l\u{e8}ve", "student", None)]);
        assert_eq!(e.process_for("un \u{e9}l\u{e8}ve ici", None), "un student ici");
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
