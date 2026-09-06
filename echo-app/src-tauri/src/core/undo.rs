//! Taking back the last thing Echo typed.
//!
//! Dictation arrives as one silent burst that nobody watches land, so a wrong
//! transcript is discovered after the fact — usually with the caret already
//! somewhere else. That shapes the whole mechanism:
//!
//! **The target app's undo does the work, not a count of backspaces.** Sending
//! N backspaces is exact only while nothing has touched the field since, and a
//! process cannot observe that. When it is wrong it deletes the user's own
//! typing, which is a worse bug than the one being fixed. `Ctrl/Cmd+Z` delegates
//! to an undo stack that *does* know what happened in between.
//!
//! ponytail: the cost of delegating is that granularity belongs to the target.
//! A pasted transcript is normally one undo step; a keystroke-injected one may
//! be several in an app with per-character undo, so one press may not clear all
//! of it. Recommending paste injection covers that, and does not need code.

/// What Echo last typed into another application, kept so it can be taken back
/// or re-transcribed.
///
/// Cleared as soon as it is used: a second undo press would otherwise walk
/// backwards into the user's own edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastDelivery {
    /// Exactly what was delivered, after dictionary and spacing.
    pub text: String,
    /// Whether it went in by clipboard paste rather than keystrokes. Only used
    /// to explain the granularity caveat above in logs.
    pub used_paste: bool,
}

/// Phrases that mean "take that back", matched on a final transcript.
///
/// Kept to the two people actually say. A longer list is a bigger surface for
/// deleting text because someone dictated a sentence *about* undoing something.
const SCRATCH_PHRASES: [&str; 2] = ["scratch that", "undo that"];

/// True when a whole transcript is nothing but an undo request.
///
/// The match is deliberately on the *entire* utterance, not a prefix: "scratch
/// that itch on my back" is dictation, and treating it as a command would
/// silently delete the sentence before it. Trailing punctuation is ignored
/// because the decoder adds it ("Scratch that.").
pub fn is_scratch_phrase(text: &str) -> bool {
    let cleaned: String = text
        .trim()
        .trim_end_matches(|c: char| c.is_ascii_punctuation())
        .trim()
        .to_lowercase();
    SCRATCH_PHRASES.contains(&cleaned.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spoken_phrases_are_recognized_however_they_are_punctuated() {
        for said in ["scratch that", "Scratch that.", "  SCRATCH THAT!  ", "Undo that"] {
            assert!(is_scratch_phrase(said), "{said:?} should undo");
        }
    }

    /// The failure that matters: dictating a sentence that merely starts with
    /// the phrase must not delete the sentence before it.
    #[test]
    fn a_sentence_containing_the_phrase_is_still_dictation() {
        for said in [
            "scratch that itch for me",
            "I had to scratch that",
            "scratch",
            "",
        ] {
            assert!(!is_scratch_phrase(said), "{said:?} should be typed, not obeyed");
        }
    }
}
