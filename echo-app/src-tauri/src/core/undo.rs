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

/// Phrases that mean "take that back", by language.
///
/// Two or three per language, kept to what people actually say. A longer list is
/// a bigger surface for deleting text because someone dictated a sentence
/// *about* undoing something.
///
/// **Why it is safe to add languages without a native speaker to hand.** The
/// match is on the entire utterance, so a phrase nobody would say simply never
/// fires and costs nothing — the same asymmetry that lets spoken punctuation
/// ship language by language. The risk runs the other way: a phrase so ordinary
/// that someone might dictate it as a sentence. That is why these are
/// imperative and terse, which is how a command is said and not how prose runs.
///
/// Entries here want a native speaker's eye all the same, and corrections are a
/// one-line change. Where dropping an accent is plausible in a transcript, both
/// spellings are listed rather than relying on the decoder.
const SCRATCH_PHRASES: &[(&str, &[&str])] = &[
    ("en", &["scratch that", "undo that"]),
    ("es", &["borra eso", "anula eso", "olvida eso"]),
    ("fr", &["annule ça", "annule ca", "oublie ça", "oublie ca"]),
    ("de", &["streich das", "lösch das", "losch das", "vergiss das"]),
    ("it", &["cancella quello", "annulla quello"]),
    ("pt", &["apaga isso", "anula isso", "esquece isso"]),
    ("nl", &["wis dat", "vergeet dat"]),
];

/// Language codes with undo phrases, for the settings screen.
pub fn supported_languages() -> Vec<&'static str> {
    SCRATCH_PHRASES.iter().map(|(code, _)| *code).collect()
}

/// Phrases for a language code, matched on the leading subtag so "en-GB" and
/// "pt-BR" find their lists.
///
/// English is the fallback rather than nothing, because the alternative is that
/// a user who never set a language loses the feature silently. Unlike
/// punctuation — where applying English rules to French fires mid-sentence —
/// an English phrase in a French utterance cannot match by accident.
fn phrases_for(language: Option<&str>) -> &'static [&'static str] {
    let raw = language.unwrap_or("en").to_lowercase();
    let code = raw.split(['-', '_']).next().unwrap_or("en");
    SCRATCH_PHRASES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, p)| *p)
        .unwrap_or(SCRATCH_PHRASES[0].1)
}

/// True when a whole transcript is nothing but an undo request.
///
/// The match is deliberately on the *entire* utterance, not a prefix: "scratch
/// that itch on my back" is dictation, and treating it as a command would
/// silently delete the sentence before it. Trailing punctuation is ignored
/// because the decoder adds it ("Scratch that.").
///
/// `language` is what the decoder reported for this utterance, so auto-detect
/// works: a German sentence is checked against the German phrases even when the
/// configured language is English.
pub fn is_scratch_phrase(text: &str, language: Option<&str>) -> bool {
    // Only ASCII punctuation is trimmed, which is enough: the marks a decoder
    // adds to the end of a sentence are `.`, `!` and `?` in every language here,
    // and Spanish opening marks sit at the *start* where `trim` handles them.
    let cleaned: String = text
        .trim()
        .trim_start_matches(|c: char| c == '\u{a1}' || c == '\u{bf}')
        .trim_end_matches(|c: char| c.is_ascii_punctuation())
        .trim()
        .to_lowercase();
    phrases_for(language).contains(&cleaned.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spoken_phrases_are_recognized_however_they_are_punctuated() {
        for said in ["scratch that", "Scratch that.", "  SCRATCH THAT!  ", "Undo that"] {
            assert!(is_scratch_phrase(said, None), "{said:?} should undo");
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
            assert!(!is_scratch_phrase(said, None), "{said:?} should be typed, not obeyed");
        }
    }

    /// The defect this fixes: punctuation shipped in seven languages while undo
    /// answered only to English, so a German user got "streich das" typed into
    /// their document instead of an undo.
    #[test]
    fn every_language_with_punctuation_rules_can_also_undo() {
        for code in crate::core::format::punctuation::supported_languages() {
            assert!(
                supported_languages().contains(&code),
                "{code} has spoken punctuation but no undo phrase"
            );
        }
    }

    #[test]
    fn each_language_answers_to_its_own_phrases() {
        for (code, phrases) in SCRATCH_PHRASES {
            for said in *phrases {
                assert!(
                    is_scratch_phrase(said, Some(code)),
                    "{said:?} should undo in {code}"
                );
                // Decoders capitalise and punctuate; the spoken form is the same.
                let dressed = format!("{}{}.", said[..1].to_uppercase(), &said[1..]);
                assert!(
                    is_scratch_phrase(&dressed, Some(code)),
                    "{dressed:?} should undo in {code}"
                );
            }
        }
    }

    /// Regional tags must find their language rather than falling back.
    #[test]
    fn a_regional_tag_finds_its_language() {
        assert!(is_scratch_phrase("apaga isso", Some("pt-BR")));
        assert!(is_scratch_phrase("scratch that", Some("en_GB")));
    }

    /// An unconfigured language still gets English rather than nothing.
    #[test]
    fn an_unknown_language_falls_back_to_english() {
        assert!(is_scratch_phrase("scratch that", Some("ja")));
        assert!(!is_scratch_phrase("streich das", Some("ja")));
    }
}
