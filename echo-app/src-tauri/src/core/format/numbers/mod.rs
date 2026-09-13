//! Spelled-out numbers, one parser per language.
//!
//! Number words are grammar rather than a lookup — "quatre-vingt-dix-sept",
//! "einundzwanzig", "twenty five" — so a language is a module of its own, not
//! another row in a shared table. Each module exposes the same `apply`, and the
//! table below is the whole of what decides which languages this stage touches.
//!
//! A language with no module is left exactly as spoken. Falling back to the
//! English parser would turn a French "one" homograph into a digit, and a
//! number converted wrongly is worse than one left as words: the reader cannot
//! tell it was Echo that changed it.

mod de;
mod en;
mod es;
mod fr;
mod it;
mod nl;
mod pt;

/// A language's whole number grammar: transcript in, transcript out.
type Parser = fn(&str) -> String;

/// Language code → its parser. Matched on the leading subtag, so "en-GB" and
/// "pt-BR" find their rules.
const PARSERS: &[(&str, Parser)] = &[
    ("en", en::apply),
    ("de", de::apply),
    ("es", es::apply),
    ("fr", fr::apply),
    ("it", it::apply),
    ("nl", nl::apply),
    ("pt", pt::apply),
];

/// Language codes number conversion has rules for, so the settings screen can
/// say which languages this stage applies to.
pub fn supported_languages() -> Vec<&'static str> {
    PARSERS.iter().map(|(code, _)| *code).collect()
}

fn parser_for(language: Option<&str>) -> Option<Parser> {
    let raw = language.unwrap_or("en").to_lowercase();
    let code = raw.split(['-', '_']).next().unwrap_or("en");
    PARSERS.iter().find(|(c, _)| *c == code).map(|(_, f)| *f)
}

/// Whether number conversion exists for `language`.
pub fn covers(language: Option<&str>) -> bool {
    parser_for(language).is_some()
}

/// Convert spelled-out numbers in `text` using `language`'s rules, or return it
/// untouched when there are none.
pub fn apply(text: &str, language: Option<&str>) -> String {
    let Some(parse) = parser_for(language) else {
        return text.to_string();
    };
    // One line at a time. Several parsers rebuild a line from its words with
    // single spaces, which is harmless inside a line and fatal across one: the
    // punctuation stage has already turned "new paragraph" into line breaks,
    // and they were being flattened back into spaces. A number never spans a
    // line break anyway.
    text.split('\n').map(parse).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_language_is_claimed_only_with_a_parser() {
        assert!(covers(None));
        assert!(covers(Some("en")));
        assert!(covers(Some("en-GB")));
        assert!(!covers(Some("ja")), "ja has no number rules");
        assert_eq!(apply("twenty five", Some("ja")), "twenty five");
    }

    /// The spoken-punctuation stage runs first and leaves real line breaks;
    /// every parser must hand them back.
    #[test]
    fn line_breaks_survive_every_parser() {
        for code in supported_languages() {
            let text = "a\n\nb";
            assert_eq!(
                apply(text, Some(code)),
                text,
                "{code} flattened a paragraph"
            );
        }
        assert_eq!(apply("twenty five\n\nthirty two", Some("en")), "25\n\n32");
    }

    #[test]
    fn every_listed_language_resolves() {
        for code in supported_languages() {
            assert!(covers(Some(code)), "{code} is listed but has no parser");
        }
    }

    /// Each language gets its own parser, including through a region subtag —
    /// the English rules must not be what reads a Brazilian "vinte e cinco".
    #[test]
    fn spanish_and_portuguese_use_their_own_rules() {
        assert_eq!(apply("veinticinco", Some("es-MX")), "25");
        assert_eq!(apply("vinte e cinco", Some("pt-BR")), "25");
        assert_eq!(apply("twenty five", Some("es")), "twenty five");
    }
}
