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
//! 2. **A genitive behind means the same.** "period *of* time", "punto *de*
//!    partida". Punctuation is never followed by it.
//!
//! **Languages are opt-in, one table at a time.** A language with no table does
//! nothing, rather than falling back to the English words — silently applying
//! English rules to Spanish is how "punto" goes unrecognised while "point"
//! fires in the middle of a French sentence. [`supported_languages`] reports which
//! languages have tables so the settings screen can say so, instead of leaving
//! the user to discover it by being ignored.
//!
//! An unidiomatic phrase in a table is harmless: it simply never matches. A
//! *missing determiner* is not — it is how a false positive gets through. That
//! asymmetry is why the determiner lists are the longest part of each table.
//!
//! ponytail: this is a heuristic, not a parser, and it will be wrong on a
//! sentence that genuinely needs the word in some other position ("the word
//! period is ambiguous"). The upgrade path is the standard one — require a
//! prefix word, "press period" — and it costs users a word on every mark, which
//! is why it is not the default. The feature is opt-in, and the escape hatch is
//! turning it off for the app where it bites.

use super::{key, words};

/// The spoken forms for one language.
struct Rules {
    /// Phrase → the mark it becomes.
    marks: &'static [(&'static str, &'static str)],
    /// Phrases that produce whitespace rather than a character.
    breaks: &'static [(&'static str, &'static str)],
    /// Words meaning the next word is a noun, not a command.
    determiners: &'static [&'static str],
    /// The word that, following a mark word, proves it was a noun — English
    /// "of", Spanish "de". Empty where the language has no such tell.
    genitive: &'static str,
}

/// Marks whose *spoken form* is the same borrowed word everywhere here, so
/// they live once rather than in every table.
const SHARED: &[(&str, &str)] = &[
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

const EN_MARKS: &[(&str, &str)] = &[
    ("period", "."),
    ("full stop", "."),
    ("question mark", "?"),
    ("exclamation mark", "!"),
    ("exclamation point", "!"),
    ("comma", ","),
    ("semicolon", ";"),
    ("colon", ":"),
    ("ellipsis", "\u{2026}"),
    ("open paren", "("),
    ("open parenthesis", "("),
    ("close paren", ")"),
    ("close parenthesis", ")"),
    ("open quote", "\u{201c}"),
    ("close quote", "\u{201d}"),
    ("open bracket", "["),
    ("close bracket", "]"),
    ("hyphen", "-"),
    ("dash", "\u{2014}"),
    ("em dash", "\u{2014}"),
];
const EN_BREAKS: &[(&str, &str)] = &[
    ("new line", "\n"),
    ("newline", "\n"),
    ("new paragraph", "\n\n"),
    ("tab key", "\t"),
];
const EN_DETERMINERS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "one", "each", "every", "any", "some",
    "no", "another", "my", "your", "our", "their", "his", "her", "its", "which", "what", "whose",
];

const ES_MARKS: &[(&str, &str)] = &[
    ("punto", "."),
    ("punto final", "."),
    ("signo de interrogacion", "?"),
    ("signo de exclamacion", "!"),
    ("coma", ","),
    ("punto y coma", ";"),
    ("dos puntos", ":"),
    ("puntos suspensivos", "\u{2026}"),
    ("abrir parentesis", "("),
    ("cerrar parentesis", ")"),
    ("abrir comillas", "\u{201c}"),
    ("cerrar comillas", "\u{201d}"),
    ("guion", "-"),
    ("raya", "\u{2014}"),
];
const ES_BREAKS: &[(&str, &str)] = &[("nueva linea", "\n"), ("nuevo parrafo", "\n\n")];
const ES_DETERMINERS: &[&str] = &[
    "el",
    "la",
    "los",
    "las",
    "un",
    "una",
    "unos",
    "unas",
    "este",
    "esta",
    "esos",
    "ese",
    "esa",
    "mi",
    "tu",
    "su",
    "cada",
    "otro",
    "otra",
    "algun",
    "cualquier",
    "ningun",
];

const FR_MARKS: &[(&str, &str)] = &[
    ("point", "."),
    ("point d'interrogation", "?"),
    ("point d'exclamation", "!"),
    ("virgule", ","),
    ("point virgule", ";"),
    ("deux points", ":"),
    ("points de suspension", "\u{2026}"),
    ("ouvrir la parenthese", "("),
    ("fermer la parenthese", ")"),
    ("ouvrir les guillemets", "\u{ab}"),
    ("fermer les guillemets", "\u{bb}"),
    ("trait d'union", "-"),
    ("tiret", "\u{2014}"),
];
const FR_BREAKS: &[(&str, &str)] = &[("nouvelle ligne", "\n"), ("nouveau paragraphe", "\n\n")];
const FR_DETERMINERS: &[&str] = &[
    "le", "la", "les", "un", "une", "des", "du", "ce", "cet", "cette", "ces", "mon", "ma", "mes",
    "ton", "ta", "son", "sa", "chaque", "quel", "quelle", "aucun", "certain",
];

const DE_MARKS: &[(&str, &str)] = &[
    ("punkt", "."),
    ("fragezeichen", "?"),
    ("ausrufezeichen", "!"),
    ("komma", ","),
    ("semikolon", ";"),
    ("doppelpunkt", ":"),
    ("auslassungspunkte", "\u{2026}"),
    ("klammer auf", "("),
    ("klammer zu", ")"),
    ("bindestrich", "-"),
    ("gedankenstrich", "\u{2014}"),
];
const DE_BREAKS: &[(&str, &str)] = &[("neue zeile", "\n"), ("neuer absatz", "\n\n")];
const DE_DETERMINERS: &[&str] = &[
    "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem", "einer", "eines",
    "dieser", "diese", "dieses", "jeder", "jede", "jedes", "mein", "dein", "sein", "ihr", "kein",
    "keine",
];

const IT_MARKS: &[(&str, &str)] = &[
    ("punto", "."),
    ("punto interrogativo", "?"),
    ("punto esclamativo", "!"),
    ("virgola", ","),
    ("punto e virgola", ";"),
    ("due punti", ":"),
    ("puntini di sospensione", "\u{2026}"),
    ("aprire parentesi", "("),
    ("chiudere parentesi", ")"),
    ("trattino", "-"),
    ("lineetta", "\u{2014}"),
];
const IT_BREAKS: &[(&str, &str)] = &[("a capo", "\n"), ("nuovo paragrafo", "\n\n")];
const IT_DETERMINERS: &[&str] = &[
    "il", "lo", "la", "i", "gli", "le", "un", "uno", "una", "questo", "questa", "quel", "quella",
    "ogni", "mio", "tuo", "suo", "nessun", "qualche",
];

const PT_MARKS: &[(&str, &str)] = &[
    ("ponto", "."),
    ("ponto final", "."),
    ("ponto de interrogacao", "?"),
    ("ponto de exclamacao", "!"),
    ("virgula", ","),
    ("ponto e virgula", ";"),
    ("dois pontos", ":"),
    ("reticencias", "\u{2026}"),
    ("abrir parenteses", "("),
    ("fechar parenteses", ")"),
    ("hifen", "-"),
    ("travessao", "\u{2014}"),
];
const PT_BREAKS: &[(&str, &str)] = &[("nova linha", "\n"), ("novo paragrafo", "\n\n")];
const PT_DETERMINERS: &[&str] = &[
    "o", "a", "os", "as", "um", "uma", "uns", "umas", "este", "esta", "esse", "essa", "cada",
    "meu", "minha", "seu", "sua", "outro", "outra", "algum", "nenhum", "qualquer",
];

const NL_MARKS: &[(&str, &str)] = &[
    ("punt", "."),
    ("vraagteken", "?"),
    ("uitroepteken", "!"),
    ("komma", ","),
    ("puntkomma", ";"),
    ("dubbele punt", ":"),
    ("beletselteken", "\u{2026}"),
    ("haakje openen", "("),
    ("haakje sluiten", ")"),
    ("koppelteken", "-"),
    ("gedachtestreepje", "\u{2014}"),
];
const NL_BREAKS: &[(&str, &str)] = &[("nieuwe regel", "\n"), ("nieuwe alinea", "\n\n")];
const NL_DETERMINERS: &[&str] = &[
    "de", "het", "een", "deze", "dit", "die", "dat", "elke", "elk", "ieder", "mijn", "jouw",
    "zijn", "haar", "hun", "geen", "sommige",
];

/// Which languages have rules, keyed by the code the decoder reports.
///
/// Deliberately short. The languages missing from it — Russian, Ukrainian,
/// Turkish, Arabic, Hindi, Chinese, Japanese, Korean — are missing because
/// writing their tables without a speaker to check them would be guessing, and
/// a wrong determiner list silently mangles text on every utterance. Adding one
/// is a table and its tests, and nothing else.
const RULES: &[(&str, Rules)] = &[
    (
        "en",
        Rules {
            marks: EN_MARKS,
            breaks: EN_BREAKS,
            determiners: EN_DETERMINERS,
            genitive: "of",
        },
    ),
    (
        "es",
        Rules {
            marks: ES_MARKS,
            breaks: ES_BREAKS,
            determiners: ES_DETERMINERS,
            genitive: "de",
        },
    ),
    (
        "fr",
        Rules {
            marks: FR_MARKS,
            breaks: FR_BREAKS,
            determiners: FR_DETERMINERS,
            genitive: "de",
        },
    ),
    (
        "de",
        Rules {
            marks: DE_MARKS,
            breaks: DE_BREAKS,
            determiners: DE_DETERMINERS,
            genitive: "",
        },
    ),
    (
        "it",
        Rules {
            marks: IT_MARKS,
            breaks: IT_BREAKS,
            determiners: IT_DETERMINERS,
            genitive: "di",
        },
    ),
    (
        "pt",
        Rules {
            marks: PT_MARKS,
            breaks: PT_BREAKS,
            determiners: PT_DETERMINERS,
            genitive: "de",
        },
    ),
    (
        "nl",
        Rules {
            marks: NL_MARKS,
            breaks: NL_BREAKS,
            determiners: NL_DETERMINERS,
            genitive: "van",
        },
    ),
];

/// Language codes with spoken-punctuation rules, for the settings screen.
pub fn supported_languages() -> Vec<&'static str> {
    RULES.iter().map(|(code, _)| *code).collect()
}

/// The rules for a language code, matched on the leading subtag so "en-GB" and
/// "pt-BR" find their tables.
fn rules_for(language: Option<&str>) -> Option<&'static Rules> {
    let raw = language.unwrap_or("en").to_lowercase();
    let code = raw.split(['-', '_']).next().unwrap_or("en");
    RULES.iter().find(|(c, _)| *c == code).map(|(_, r)| r)
}

/// Marks that lead rather than follow. Getting this backwards gives "( aside)".
const OPENING: &[&str] = &["(", "[", "\u{201c}", "\u{ab}", "\u{201e}"];

/// One resolved token on its way out.
enum Piece {
    /// An ordinary word, kept verbatim.
    Word(String),
    /// A mark that attaches to whatever came before it, with no space.
    Attached(&'static str),
    /// A mark that attaches to whatever comes *after* it.
    Open(&'static str),
    /// Whitespace that replaces the space around it.
    Break(&'static str),
}

/// Replace spoken punctuation with the marks themselves.
///
/// A language with no table returns the transcript untouched.
pub fn apply(text: &str, language: Option<&str>) -> String {
    let Some(rules) = rules_for(language) else {
        return text.to_string();
    };

    let words = words(text);
    let keys: Vec<String> = words.iter().map(|w| key(w)).collect();
    let lookahead = max_phrase_words(rules);

    let mut pieces: Vec<Piece> = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        match longest_match(rules, &keys, i, lookahead) {
            Some((len, piece)) if is_command(rules, &keys, i, len) => {
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

/// Longest phrase in this language's tables, so the matcher knows how far to
/// look ahead without a hard-coded number that drifts when a table changes.
fn max_phrase_words(rules: &Rules) -> usize {
    rules
        .marks
        .iter()
        .chain(rules.breaks.iter())
        .chain(SHARED.iter())
        .map(|(phrase, _)| phrase.split_whitespace().count())
        .max()
        .unwrap_or(1)
}

/// The longest phrase from any table starting at `i`, if any.
fn longest_match(
    rules: &Rules,
    keys: &[String],
    i: usize,
    lookahead: usize,
) -> Option<(usize, Piece)> {
    let limit = lookahead.min(keys.len() - i);
    // Longest first: "exclamation mark" must win over any shorter prefix.
    for len in (1..=limit).rev() {
        let phrase = keys[i..i + len].join(" ");
        if let Some((_, mark)) = rules
            .marks
            .iter()
            .chain(SHARED.iter())
            .find(|(p, _)| *p == phrase)
        {
            return Some((
                len,
                if OPENING.contains(mark) {
                    Piece::Open(mark)
                } else {
                    Piece::Attached(mark)
                },
            ));
        }
        if let Some((_, brk)) = rules.breaks.iter().find(|(p, _)| *p == phrase) {
            return Some((len, Piece::Break(brk)));
        }
    }
    None
}

/// Whether the phrase at `i..i + len` was spoken as a command rather than used
/// as an ordinary word. See the two rules in the module docs.
fn is_command(rules: &Rules, keys: &[String], i: usize, len: usize) -> bool {
    // Multi-word phrases ("new paragraph", "question mark") are unambiguous:
    // nobody says them meaning something else.
    if len > 1 {
        return true;
    }
    if i > 0 && rules.determiners.contains(&keys[i - 1].as_str()) {
        return false;
    }
    if !rules.genitive.is_empty() && keys.get(i + len).map(String::as_str) == Some(rules.genitive) {
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

    /// Shorthand for the English path, which most of these exercise.
    fn en(text: &str) -> String {
        apply(text, Some("en"))
    }

    #[test]
    fn marks_attach_to_the_previous_word() {
        assert_eq!(en("hello comma world period"), "hello, world.");
        assert_eq!(en("really question mark"), "really?");
        assert_eq!(en("stop exclamation point"), "stop!");
    }

    #[test]
    fn line_breaks_replace_the_space_around_them() {
        assert_eq!(en("one new line two"), "one\ntwo");
        assert_eq!(en("one new paragraph two"), "one\n\ntwo");
        assert_eq!(en("comma new line after"), ",\nafter");
    }

    /// The failure this module exists to avoid. A determiner in front means the
    /// speaker used the word as a word.
    #[test]
    fn a_determiner_in_front_keeps_the_word() {
        assert_eq!(en("after a period of time"), "after a period of time");
        assert_eq!(en("the colon is inflamed"), "the colon is inflamed");
        assert_eq!(en("draw one dash here"), "draw one dash here");
    }

    /// A genitive behind means the same: punctuation is never followed by it.
    #[test]
    fn a_genitive_behind_keeps_the_word() {
        assert_eq!(en("period of grace"), "period of grace");
        assert_eq!(en("colon of the patient"), "colon of the patient");
    }

    #[test]
    fn multi_word_phrases_are_always_commands() {
        assert_eq!(en("the question mark"), "the?");
        assert_eq!(en("a new paragraph"), "a\n\n");
    }

    #[test]
    fn the_longest_phrase_wins() {
        assert_eq!(en("wow exclamation mark"), "wow!");
        assert_eq!(en("open parenthesis aside close parenthesis"), "(aside)");
    }

    #[test]
    fn decoder_punctuation_around_the_spoken_word_is_ignored() {
        assert_eq!(en("hello Comma, world"), "hello, world");
        assert_eq!(en("done Period."), "done.");
    }

    #[test]
    fn text_without_spoken_marks_is_untouched() {
        let said = "the quick brown fox jumped over the lazy dog";
        assert_eq!(en(said), said);
    }

    #[test]
    fn empty_input_is_empty_output() {
        assert_eq!(en(""), "");
        assert_eq!(en("   "), "");
    }

    // ── Other languages ──────────────────────────────────────────────────────

    #[test]
    fn spanish_marks_and_breaks_work() {
        assert_eq!(apply("hola coma mundo punto", Some("es")), "hola, mundo.");
        assert_eq!(apply("uno nueva linea dos", Some("es")), "uno\ndos");
    }

    #[test]
    fn french_german_italian_portuguese_and_dutch_have_tables() {
        assert_eq!(apply("bonjour virgule monde", Some("fr")), "bonjour, monde");
        assert_eq!(apply("hallo komma welt", Some("de")), "hallo, welt");
        assert_eq!(apply("ciao virgola mondo", Some("it")), "ciao, mondo");
        assert_eq!(apply("ola virgula mundo", Some("pt")), "ola, mundo");
        assert_eq!(apply("hallo komma wereld", Some("nl")), "hallo, wereld");
    }

    /// Each language's own determiners guard its own ambiguous words — the
    /// Spanish rule must not be doing it with English articles.
    #[test]
    fn each_language_guards_with_its_own_determiners() {
        assert_eq!(apply("el punto es claro", Some("es")), "el punto es claro");
        assert_eq!(
            apply("le point est clair", Some("fr")),
            "le point est clair"
        );
        assert_eq!(apply("punto de partida", Some("es")), "punto de partida");
    }

    /// The failure the per-language tables exist to prevent: English words must
    /// not fire inside another language's sentence.
    #[test]
    fn english_words_do_not_fire_in_another_language() {
        // "period" is not a Spanish command word, so it stays a word.
        assert_eq!(apply("un period largo", Some("es")), "un period largo");
    }

    /// A language with no table does nothing at all, rather than falling back
    /// to English and mangling the sentence.
    #[test]
    fn an_uncovered_language_is_left_completely_alone() {
        let japanese = "hello comma world period";
        assert_eq!(apply(japanese, Some("ja")), japanese);
        for absent in ["ja", "ru", "tr", "ar", "hi", "zh", "ko"] {
            assert!(
                !supported_languages().contains(&absent),
                "{absent} is listed as supported but has no table"
            );
        }
    }

    /// Region subtags must find the base language's table.
    #[test]
    fn region_subtags_resolve_to_the_base_language() {
        assert_eq!(apply("hello comma world", Some("en-GB")), "hello, world");
        assert_eq!(apply("ola virgula mundo", Some("pt_BR")), "ola, mundo");
        assert!(rules_for(Some("en-US")).is_some());
    }

    /// No language means English, which is what the decoder defaults to.
    #[test]
    fn an_absent_language_means_english() {
        assert_eq!(apply("hello comma world", None), "hello, world");
        assert!(rules_for(None).is_some());
    }

    /// Every table must be usable: a mark with no phrase, or a phrase mapping
    /// to nothing, would be a silent dead entry.
    #[test]
    fn every_table_is_well_formed() {
        for (code, rules) in RULES {
            assert!(!rules.marks.is_empty(), "{code} has no marks");
            for (phrase, mark) in rules.marks.iter().chain(rules.breaks.iter()) {
                assert!(!phrase.is_empty(), "{code} has an empty phrase");
                assert!(!mark.is_empty(), "{code}: {phrase} maps to nothing");
                assert_eq!(
                    *phrase,
                    phrase.to_lowercase(),
                    "{code}: {phrase} must be lowercase to match a key"
                );
            }
            assert!(
                !rules.determiners.is_empty(),
                "{code} has no determiners, so every ambiguous word is a false positive"
            );
        }
    }

    #[test]
    fn the_supported_list_matches_the_tables() {
        let listed = supported_languages();
        assert_eq!(listed.len(), RULES.len());
        for code in listed {
            assert!(
                rules_for(Some(code)).is_some(),
                "{code} is listed but has no rules"
            );
        }
    }
}
