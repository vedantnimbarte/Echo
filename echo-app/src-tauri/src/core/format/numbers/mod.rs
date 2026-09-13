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

mod en;
mod fr;
mod it;

/// Language code → its parser. Matched on the leading subtag, so "en-GB" and
/// "pt-BR" find their rules.
const PARSERS: &[(&str, fn(&str) -> String)] =
    &[("en", en::apply), ("fr", fr::apply), ("it", it::apply)];

/// Language codes number conversion has rules for, so the settings screen can
/// say which languages this stage applies to.
pub fn supported_languages() -> Vec<&'static str> {
    PARSERS.iter().map(|(code, _)| *code).collect()
}

fn parser_for(language: Option<&str>) -> Option<fn(&str) -> String> {
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
    match parser_for(language) {
        Some(parse) => parse(text),
        None => text.to_string(),
    }
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

    #[test]
    fn every_listed_language_resolves() {
        for code in supported_languages() {
            assert!(covers(Some(code)), "{code} is listed but has no parser");
        }
    }
}
