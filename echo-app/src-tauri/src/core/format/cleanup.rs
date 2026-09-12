//! Removing what you said but did not mean to write.
//!
//! Speech has hesitations in it. "Um", a word said twice while the sentence
//! catches up, a false start abandoned mid-phrase — none of that is meant for
//! the page, and a faithful transcript full of it reads as though the writer
//! were unwell.
//!
//! **This stage is in tension with the rest of Echo, and the tension is
//! deliberate.** Everything else here works to reproduce what was said. This
//! throws some of it away. So the rules are narrow, and each one only fires
//! where the alternative reading is not English anybody writes:
//!
//! - **Fillers** are removed only from a fixed list of non-words. "Um" is never
//!   a word someone meant. "Like" and "actually" are, so they are not on it,
//!   however often they are used as filler.
//! - **Doubled words** collapse only for function words, and only for the ones
//!   that cannot legitimately double. "The the" is a stutter; "had had" and
//!   "that that" are grammar, so they are excluded by name.
//!
//! **What rules cannot do is self-correction.** "Send it Tuesday, no, Wednesday"
//! needs to become "Send it Wednesday", and deciding how far back to delete is a
//! judgement about meaning, not a pattern. A rule aggressive enough to catch it
//! would eat clauses people meant to keep. That half is left to the optional
//! local-LLM pass in [`crate::core::command`], which is off by default because
//! it rewrites your words.
//!
//! English only, and stated as such by [`covers`] — the filler list and the
//! doubling exceptions are both facts about one language.

use super::{key, words};

/// Sounds that are never words. Removing one cannot destroy meaning, which is
/// the entire criterion for being on this list.
///
/// Deliberately excludes "like", "so", "right", "well", "actually", "basically"
/// and "literally". They are used as filler constantly and they are also
/// ordinary words; no rule can tell which without understanding the sentence,
/// and deleting one that was meant changes what was said.
const FILLERS: &[&str] = &["um", "uh", "erm", "uhh", "umm", "hmm", "mmm", "mm", "eh"];

/// Function words that can double in real English, so a repeat is not a stutter.
///
/// "I had had enough", "the thing that that rule covers", "what it is is
/// complicated". Short, because it only needs the words that both appear in
/// [`is_collapsible`] and can legitimately repeat.
const LEGITIMATE_DOUBLES: &[&str] = &["had", "that", "is", "was", "will"];

/// Whether a repeated word is safe to collapse.
///
/// Only function words: repeating a *content* word is often deliberate ("very
/// very good", "no no no"), and collapsing that changes emphasis the speaker
/// chose. Restricting to grammar words keeps the rule to the case where the
/// repeat is meaningless.
fn is_collapsible(word: &str) -> bool {
    const FUNCTION_WORDS: &[&str] = &[
        "the", "a", "an", "and", "or", "but", "to", "of", "in", "on", "at", "for", "with", "from",
        "by", "as", "if", "it", "i", "we", "you", "he", "she", "they", "this", "there", "then",
        "so",
    ];
    FUNCTION_WORDS.contains(&word) && !LEGITIMATE_DOUBLES.contains(&word)
}

/// Whether the cleanup rules exist for `language`.
///
/// English only: the filler list and the doubling exceptions are both facts
/// about one language, and guessing them for another is how a rule starts
/// deleting words that were meant.
pub fn covers(language: Option<&str>) -> bool {
    let raw = language.unwrap_or("en").to_lowercase();
    raw.split(['-', '_']).next() == Some("en")
}

/// Strip hesitations and stutters from a transcript.
pub fn apply(text: &str) -> String {
    let words = words(text);
    let mut out: Vec<&str> = Vec::with_capacity(words.len());

    for word in words {
        let k = key(word);

        // A filler carries no meaning, so it goes — unless dropping it would
        // leave nothing at all. "Um." on its own was still an utterance, and
        // returning an empty transcript would look like a failed recording.
        if FILLERS.contains(&k.as_str()) {
            continue;
        }

        // A word repeated immediately after itself, where the repeat cannot be
        // grammar. Compared on the key so "the The" still collapses.
        if out
            .last()
            .is_some_and(|prev| key(prev) == k && is_collapsible(&k))
        {
            continue;
        }

        out.push(word);
    }

    if out.is_empty() {
        // Everything was filler. Better to hand back what was said than an
        // empty string the user cannot tell from a broken microphone.
        return text.trim().to_string();
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hesitations_are_removed() {
        assert_eq!(apply("um so I was uh thinking"), "so I was thinking");
        assert_eq!(apply("Um, hello"), "hello");
        assert_eq!(apply("well hmm maybe"), "well maybe");
    }

    /// The rule that keeps this honest. These are filler constantly, and they
    /// are also words — no rule can tell which without understanding the
    /// sentence, so they stay.
    #[test]
    fn words_that_are_only_sometimes_filler_are_kept() {
        for said in [
            "I like this actually",
            "so basically it works",
            "well that is literally right",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn stuttered_function_words_collapse() {
        assert_eq!(apply("the the cat sat"), "the cat sat");
        assert_eq!(apply("I I think so"), "I think so");
        assert_eq!(apply("go to to the shop"), "go to the shop");
    }

    /// English doubles some words legitimately, and collapsing those is a
    /// grammar error the user then has to fix by hand.
    #[test]
    fn legitimate_doubles_survive() {
        assert_eq!(apply("I had had enough"), "I had had enough");
        assert_eq!(
            apply("the rule that that covers it"),
            "the rule that that covers it"
        );
        assert_eq!(
            apply("what it is is complicated"),
            "what it is is complicated"
        );
    }

    /// Repeating a content word is emphasis the speaker chose.
    #[test]
    fn repeated_content_words_are_emphasis_not_stutter() {
        assert_eq!(apply("very very good"), "very very good");
        assert_eq!(apply("no no no"), "no no no");
    }

    /// An utterance that was nothing but filler still happened. Returning an
    /// empty string would be indistinguishable from a failed recording.
    #[test]
    fn an_utterance_of_pure_filler_is_not_erased() {
        assert_eq!(apply("um"), "um");
        assert_eq!(apply("uh um uh"), "uh um uh");
    }

    /// Only the immediate repeat collapses; the same word twice in a sentence
    /// is ordinary English.
    #[test]
    fn a_word_used_twice_in_a_sentence_is_left_alone() {
        assert_eq!(apply("the cat sat on the mat"), "the cat sat on the mat");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        let said = "the quick brown fox jumped over the lazy dog";
        assert_eq!(apply(said), said);
    }

    #[test]
    fn only_english_is_claimed() {
        assert!(covers(None));
        assert!(covers(Some("en-GB")));
        for other in ["fr", "de", "es", "ja"] {
            assert!(!covers(Some(other)), "{other} has no cleanup rules");
        }
    }
}
