//! Voice snippets: say a trigger phrase, get a saved block of text.
//!
//! "Insert my address" becomes three lines of address; "sign off" becomes a
//! signature. This looks like a dictionary entry with a long replacement, and
//! the dictionary can in fact hold one — but it is the wrong tool for it, in
//! three ways that each matter:
//!
//! 1. **Matching.** The dictionary replaces a phrase *anywhere*, mid-word
//!    included, because it fixes mishearings ("jeera" → "Jira") and those
//!    happen inside sentences. A snippet trigger is an ordinary phrase. "Sign
//!    off" said inside "I need to sign off early" must not drop a signature
//!    into the middle of an email.
//! 2. **Position.** The dictionary runs before formatting, so its output gets
//!    spaced and capitalised. A snippet body is text the user already wrote
//!    exactly as they want it; the number formatter must not touch the house
//!    number in an address, and tidy must not re-punctuate a signature.
//! 3. **Prompting.** Dictionary replacements are fed to whisper as vocabulary.
//!    A 200-character address would spend the whole prompt budget biasing the
//!    decoder toward words nobody is about to say.
//!
//! So snippets are their own small table and this is their own matcher.
//!
//! # The matching rule
//!
//! A trigger matches only when it is the **whole utterance**. Case, punctuation
//! and spacing are ignored — the formatter may have produced "Sign off." from
//! "sign off" — but every word of the utterance has to be a word of the
//! trigger, in order, and nothing else.
//!
//! Matching a trigger inside a longer utterance was considered and rejected.
//! Triggers are phrases people also say for their plain meaning, and the two
//! mistakes are not symmetrical: a missed expansion types "sign off", which
//! the user sees and says again after a pause; a false one pastes a block of
//! personal details into whatever they were writing, possibly a chat box that
//! sends on paste. The pause that ends an utterance is already the gesture that
//! separates a command from a sentence, so a snippet asks for nothing new.

use crate::storage::models::Snippet;

/// Lowercased words, with anything that is not a letter or digit treated as a
/// separator. "Sign-off." and "sign off" both come out as `["sign", "off"]`,
/// and "addresses" stays one word, so a match can never end inside a word.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The body of the first enabled snippet whose trigger is the whole of any of
/// `utterances`, verbatim.
///
/// Several versions of the same utterance can be offered — the recording
/// pipeline passes the text before formatting and after it. The formatter can
/// turn a spoken "one" into "1" or drop a leading "um", and a trigger written
/// either way should still fire.
pub fn expand<'a>(snippets: &'a [Snippet], utterances: &[&str]) -> Option<&'a str> {
    let spoken: Vec<Vec<String>> = utterances.iter().map(|u| words(u)).collect();
    snippets
        .iter()
        .filter(|s| s.enabled)
        .find(|s| {
            let trigger = words(&s.trigger);
            // A trigger of pure punctuation normalises to nothing, and nothing
            // equals the empty utterance a stray noise decodes to.
            !trigger.is_empty() && spoken.contains(&trigger)
        })
        .map(|s| s.body.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(trigger: &str, body: &str) -> Snippet {
        Snippet {
            id: None,
            trigger: trigger.into(),
            body: body.into(),
            enabled: true,
        }
    }

    #[test]
    fn the_formatters_capital_and_full_stop_do_not_stop_a_match() {
        let s = [snippet("sign off", "Best,\nVedant")];
        assert_eq!(expand(&s, &["Sign off."]), Some("Best,\nVedant"));
        assert_eq!(expand(&s, &["  SIGN OFF!  "]), Some("Best,\nVedant"));
        assert_eq!(expand(&s, &["Sign-off?"]), Some("Best,\nVedant"));
    }

    #[test]
    fn a_trigger_inside_a_longer_sentence_is_left_alone() {
        let s = [snippet("sign off", "Best,\nVedant")];
        assert_eq!(expand(&s, &["I need to sign off early today."]), None);
        assert_eq!(expand(&s, &["Sign off, and then call me."]), None);
    }

    #[test]
    fn never_matches_part_of_a_word() {
        let s = [snippet("address", "1 Main St")];
        assert_eq!(expand(&s, &["Addresses."]), None);
        assert_eq!(expand(&s, &["my address"]), None);
        let s = [snippet("sign off", "x")];
        assert_eq!(expand(&s, &["signoff"]), None);
    }

    #[test]
    fn the_body_comes_back_byte_for_byte() {
        // Numbers, punctuation and line breaks are exactly what formatting
        // would otherwise reshape; none of them may move.
        let body = "Vedant Nimbarte\n221B Baker St., Apt. 4\nLondon NW1 6XE\n\n+44 20 7946 0958";
        let s = [snippet("insert my address", body)];
        assert_eq!(expand(&s, &["Insert my address."]), Some(body));
    }

    #[test]
    fn either_version_of_the_utterance_can_match() {
        // The formatter wrote "one" as a digit; the trigger was typed in words.
        let s = [snippet("reply template one", "Thanks for reaching out.")];
        assert_eq!(
            expand(&s, &["reply template one", "Reply template 1."]),
            Some("Thanks for reaching out.")
        );
    }

    #[test]
    fn disabled_and_empty_triggers_never_fire() {
        let mut off = snippet("sign off", "x");
        off.enabled = false;
        assert_eq!(expand(&[off], &["sign off"]), None);
        assert_eq!(expand(&[snippet("...", "x")], &[""]), None);
        assert_eq!(expand(&[snippet("", "x")], &["."]), None);
    }

    #[test]
    fn non_ascii_triggers_match_without_case() {
        let s = [snippet("Grüße senden", "Viele Grüße")];
        assert_eq!(expand(&s, &["grüße senden."]), Some("Viele Grüße"));
    }
}
