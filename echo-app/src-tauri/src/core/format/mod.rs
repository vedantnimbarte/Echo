//! Turning what the decoder heard into what the user meant to write.
//!
//! Whisper punctuates a transcript the way it thinks the sentence sounded. That
//! is a guess, and it is the only punctuation Echo had until now — there was no
//! way to *say* "period", and no pass that tidied spacing or capitals.
//!
//! Three stages, each independently switchable, because they are wrong in
//! different places:
//!
//! 1. [`punctuation`] — spoken marks. "new paragraph", "question mark", and
//!    the ambiguous ones like "period" and "colon".
//! 2. [`numbers`] — spelled-out quantities, times, years and units.
//! 3. [`tidy`] — spacing and sentence capitals, cleaning up after stage 1.
//!
//! **Where this runs.** After the dictionary and before injection, so a
//! dictionary rule can still rewrite a spoken mark, and so the text that lands
//! in the app is the text stored in History. Per-app profiles can turn the
//! whole thing off: a terminal wants the words exactly as spoken, an email
//! wants sentences.

pub mod numbers;
pub mod punctuation;
pub mod tidy;

/// Which stages to run. Resolved per utterance from settings and the focused
/// app's profile, the same way delivery is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatOptions {
    /// Replace spoken marks ("comma", "new line") with the marks themselves.
    pub spoken_punctuation: bool,
    /// Convert spelled-out numbers, times and units to digits.
    pub numbers: bool,
    /// Fix spacing around punctuation and capitalise sentences.
    pub tidy: bool,
}

impl FormatOptions {
    /// True when every stage is off, so callers can skip the pass entirely
    /// rather than walking the string three times to change nothing.
    pub fn is_noop(&self) -> bool {
        !self.spoken_punctuation && !self.numbers && !self.tidy
    }
}

/// Run the enabled stages over one finished transcript.
///
/// Order matters and is not arbitrary: punctuation runs first because it is the
/// stage that *introduces* marks, numbers runs on the resulting word stream,
/// and tidy runs last because it exists to clean up after the other two.
pub fn apply(text: &str, opts: FormatOptions) -> String {
    if opts.is_noop() {
        return text.to_string();
    }
    let mut out = text.to_string();
    if opts.spoken_punctuation {
        out = punctuation::apply(&out);
    }
    if opts.numbers {
        out = numbers::apply(&out);
    }
    if opts.tidy {
        out = tidy::apply(&out);
    }
    out
}

/// Split `text` into words and the whitespace between them, preserving both.
///
/// Every stage here needs the same thing: to look at words in order, decide
/// about each one in the context of its neighbours, and put the rest back
/// untouched. Sharing one tokenizer means "word" means the same thing in all
/// three, which is what stops the stages from disagreeing about where a token
/// begins.
pub(crate) fn words(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

/// The comparison form of a word: lowercased, with surrounding punctuation
/// removed, so "Comma," matches the rule for "comma".
///
/// Returned as an owned `String` because lowercasing can change length (and in
/// Turkish, the character count), so no borrow of the original survives it.
pub(crate) fn key(word: &str) -> String {
    word.trim_matches(|c: char| c.is_ascii_punctuation())
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stage_off_returns_the_transcript_untouched() {
        let opts = FormatOptions::default();
        assert!(opts.is_noop());
        assert_eq!(apply("as spoken, exactly", opts), "as spoken, exactly");
    }

    /// The stages compose in one pass: a spoken mark introduced by stage 1 is
    /// spaced correctly by stage 3, which is the whole reason for the ordering.
    #[test]
    fn stages_compose_in_order() {
        let opts = FormatOptions {
            spoken_punctuation: true,
            numbers: true,
            tidy: true,
        };
        assert_eq!(
            apply("we shipped twenty five of them period next question", opts),
            "We shipped 25 of them. Next question"
        );
    }

    #[test]
    fn key_strips_decoder_punctuation_and_case() {
        assert_eq!(key("Comma,"), "comma");
        assert_eq!(key("PERIOD."), "period");
        assert_eq!(key("plain"), "plain");
    }
}
