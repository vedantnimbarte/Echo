//! Spoken punctuation: saying "comma" and getting `,`.
//!
//! **The hard part is not the mapping, it is knowing when you meant the word.**
//! "Period" is a span of time, "colon" is an organ, "dash" is something you do
//! to a train. A naive replacement turns "a period of time" into "a . of time",
//! and because it runs on every utterance it does that forever.
//!
//! Two rules keep it honest, and between them they cover the mistakes people
//! actually hit:
//!
//! 1. **A determiner in front means it is a noun.** "a period", "the colon",
//!    "this dash", "one hyphen" — nobody speaks punctuation that way.
//! 2. **"of" behind means it is a noun.** "period of time", "colon of the
//!    patient". Punctuation is never followed by "of".
//!
//! ponytail: this is a heuristic, not a parser, and it will be wrong on a
//! sentence that genuinely needs the word in some other position ("the word
//! period is ambiguous"). The upgrade path is the standard one — require a
//! prefix word, "press period" — and it costs users a word on every mark, which
//! is why it is not the default. The feature is opt-in, and the escape hatch is
//! turning it off for the app where it bites.

use super::{key, words};

/// Spoken phrase → what it becomes. Multi-word phrases are matched first, so
/// "exclamation mark" never resolves as "exclamation" plus a stray word.
///
/// Deliberately not exhaustive. Every entry is a word that stops being usable
/// as a word, so the list holds the marks people dictate often enough to be
/// worth that trade.
const MARKS: &[(&str, &str)] = &[
    // Sentence enders.
    ("period", "."),
    ("full stop", "."),
    ("question mark", "?"),
    ("exclamation mark", "!"),
    ("exclamation point", "!"),
    // Separators.
    ("comma", ","),
    ("semicolon", ";"),
    ("colon", ":"),
    ("ellipsis", "…"),
    // Paired.
    ("open paren", "("),
    ("open parenthesis", "("),
    ("close paren", ")"),
    ("close parenthesis", ")"),
    ("open quote", "\u{201c}"),
    ("close quote", "\u{201d}"),
    ("open bracket", "["),
    ("close bracket", "]"),
    // Inline marks.
    ("hyphen", "-"),
    ("dash", "\u{2014}"),
    ("em dash", "\u{2014}"),
    ("ampersand", "&"),
    ("asterisk", "*"),
    ("at sign", "@"),
    ("hash sign", "#"),
    ("percent sign", "%"),
    ("dollar sign", "$"),
    ("forward slash", "/"),
    ("backslash", "\\"),
    ("underscore", "_"),
];

/// Line breaks, which are marks too but produce whitespace rather than a
/// character glued to the previous word.
const BREAKS: &[(&str, &str)] = &[
    ("new line", "\n"),
    ("newline", "\n"),
    ("new paragraph", "\n\n"),
    ("tab key", "\t"),
];

/// Words that mean the next word is a noun, not a command.
const DETERMINERS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "one", "each", "every",
    "any", "some", "no", "another", "my", "your", "our", "their", "his", "her",
    "its", "which", "what", "whose",
];

/// Longest phrase in either table, so the matcher knows how far to look ahead
/// without hard-coding a number that drifts when the tables change.
fn max_phrase_words() -> usize {
    MARKS
        .iter()
        .chain(BREAKS.iter())
        .map(|(phrase, _)| phrase.split_whitespace().count())
        .max()
        .unwrap_or(1)
}

/// One resolved token on its way out.
enum Piece {
    /// An ordinary word, kept verbatim.
    Word(String),
    /// A mark that attaches to whatever came before it, with no space.
    Attached(&'static str),
    /// A mark that attaches to whatever comes *after* it — an opening bracket
    /// or quote. Getting this backwards produces "( aside)".
    Open(&'static str),
    /// Whitespace that replaces the space around it.
    Break(&'static str),
}

/// Marks that lead rather than follow.
const OPENING: &[&str] = &["(", "[", "\u{201c}"];

/// Replace spoken punctuation with the marks themselves.
pub fn apply(text: &str) -> String {
    let words = words(text);
    let keys: Vec<String> = words.iter().map(|w| key(w)).collect();
    let lookahead = max_phrase_words();

    let mut pieces: Vec<Piece> = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        match longest_match(&keys, i, lookahead) {
            // A phrase matched, and context says it was meant as a command.
            Some((len, piece)) if is_command(&keys, i, len) => {
                pieces.push(piece);
                i += len;
            }
            _ => {
                pieces.push(Piece::Word(words[i].to_string()));
                i += 1;
            }
        }
    }

    assemble(pieces)
}

/// The longest phrase from either table starting at `i`, if any.
fn longest_match(keys: &[String], i: usize, lookahead: usize) -> Option<(usize, Piece)> {
    let limit = lookahead.min(keys.len() - i);
    // Longest first: "exclamation mark" must win over any shorter prefix.
    for len in (1..=limit).rev() {
        let phrase = keys[i..i + len].join(" ");
        if let Some((_, mark)) = MARKS.iter().find(|(p, _)| *p == phrase) {
            return Some((
                len,
                if OPENING.contains(mark) {
                    Piece::Open(mark)
                } else {
                    Piece::Attached(mark)
                },
            ));
        }
        if let Some((_, brk)) = BREAKS.iter().find(|(p, _)| *p == phrase) {
            return Some((len, Piece::Break(brk)));
        }
    }
    None
}

/// Whether the phrase at `i..i + len` was spoken as a command rather than used
/// as an ordinary word. See the two rules in the module docs.
fn is_command(keys: &[String], i: usize, len: usize) -> bool {
    // Multi-word phrases ("new paragraph", "question mark") are unambiguous:
    // nobody says them meaning something else.
    if len > 1 {
        return true;
    }
    if i > 0 && DETERMINERS.contains(&keys[i - 1].as_str()) {
        return false;
    }
    if keys.get(i + len).map(String::as_str) == Some("of") {
        return false;
    }
    true
}

/// Join the resolved pieces, attaching marks to the previous word and letting
/// breaks swallow the whitespace on either side.
fn assemble(pieces: Vec<Piece>) -> String {
    let mut out = String::new();
    // Whether the next word needs a space in front of it. False at the start of
    // the text and immediately after a line break.
    let mut space_before = false;

    for piece in pieces {
        match piece {
            Piece::Word(w) => {
                if space_before {
                    out.push(' ');
                }
                out.push_str(&w);
                space_before = true;
            }
            Piece::Attached(mark) => {
                // Attaches to the previous word with no space — that is the
                // whole difference between "word," and "word ,".
                out.push_str(mark);
                space_before = true;
            }
            Piece::Open(mark) => {
                // Takes the space in front and gives none behind.
                if space_before {
                    out.push(' ');
                }
                out.push_str(mark);
                space_before = false;
            }
            Piece::Break(brk) => {
                // A break replaces the surrounding spaces rather than adding to
                // them, so "one new line two" is two lines, not two lines with
                // a trailing and leading space.
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push_str(brk);
                space_before = false;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_attach_to_the_previous_word() {
        assert_eq!(apply("hello comma world period"), "hello, world.");
        assert_eq!(apply("really question mark"), "really?");
        assert_eq!(apply("stop exclamation point"), "stop!");
    }

    #[test]
    fn line_breaks_replace_the_space_around_them() {
        assert_eq!(apply("one new line two"), "one\ntwo");
        assert_eq!(apply("one new paragraph two"), "one\n\ntwo");
        assert_eq!(apply("comma new line after"), ",\nafter");
    }

    /// The failure this module exists to avoid. A determiner in front means the
    /// speaker used the word as a word.
    #[test]
    fn a_determiner_in_front_keeps_the_word() {
        assert_eq!(apply("after a period of time"), "after a period of time");
        assert_eq!(apply("the colon is inflamed"), "the colon is inflamed");
        assert_eq!(apply("draw one dash here"), "draw one dash here");
    }

    /// "of" behind means the same thing: punctuation is never followed by it.
    #[test]
    fn of_behind_keeps_the_word() {
        assert_eq!(apply("period of grace"), "period of grace");
        assert_eq!(apply("colon of the patient"), "colon of the patient");
    }

    /// Multi-word phrases are never ambiguous, so the guards do not apply and
    /// must not accidentally suppress them.
    #[test]
    fn multi_word_phrases_are_always_commands() {
        assert_eq!(apply("the question mark"), "the?");
        assert_eq!(apply("a new paragraph"), "a\n\n");
    }

    /// Longest match wins: "exclamation mark" must not resolve as some shorter
    /// prefix plus a stray word.
    #[test]
    fn the_longest_phrase_wins() {
        assert_eq!(apply("wow exclamation mark"), "wow!");
        assert_eq!(apply("open parenthesis aside close parenthesis"), "(aside)");
    }

    /// The decoder punctuates its own output, so the spoken word arrives with a
    /// comma or capital already attached.
    #[test]
    fn decoder_punctuation_around_the_spoken_word_is_ignored() {
        assert_eq!(apply("hello Comma, world"), "hello, world");
        assert_eq!(apply("done Period."), "done.");
    }

    #[test]
    fn text_without_spoken_marks_is_untouched() {
        let said = "the quick brown fox jumped over the lazy dog";
        assert_eq!(apply(said), said);
    }

    #[test]
    fn empty_input_is_empty_output() {
        assert_eq!(apply(""), "");
        assert_eq!(apply("   "), "");
    }
}
