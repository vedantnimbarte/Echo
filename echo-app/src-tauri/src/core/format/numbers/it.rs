//! Spelled-out numbers in Italian.
//!
//! Italian writes a number as one fused word: "venticinque",
//! "trecentoquarantadue", "duemilaventisei". That makes the English rule easy
//! to state: **a compound is a number, a simple word is a word.** "ventuno"
//! cannot be anything but 21, while "tre", "dieci", "cento" and "mille" stay as
//! spoken, and so do the words that merely are number words ("sei" is "you
//! are", "venti" is also "winds").
//!
//! **Compounds are parsed by morpheme, not looked up.** A word is split into
//! the pieces Italian builds numbers from (units, teens, tens, "cento",
//! "mille"/"mila") and is a number only if the pieces use up the whole word. So
//! "quarantena", "treno" and "venticinquenne" fail to parse rather than needing
//! a list of exceptions. Elision is part of the grammar: tens drop their vowel
//! before "uno" and "otto" ("ventuno", "trentotto") and "cento" may drop its
//! own ("centottanta"). A final "tré" is folded to "tre" before any of this.
//!
//! **"un", "uno" and "una" are the article.** A lone "uno" never converts, and
//! never lets a unit settle it either: "uno per cento" stays words. As the
//! morpheme inside a compound ("ventuno", "centouno") there is no article to
//! confuse it with.
//!
//! **A capitalised compound stays a word.** "il Novecento", "la Cinquecento",
//! "il Sessantotto" are centuries, a car and 1968, and they are compounds like
//! any other. The capital is the only tell, and it is also what a sentence
//! start looks like, where style guides spell a number out anyway. A unit
//! behind still converts it: "Venti per cento" is a quantity, not a name.
//!
//! **Typography is Italian, per CLDR.** Thousands are grouped with a dot from
//! five digits ("25.000", but "2026"), "%" and "°" attach to the number, and
//! "€", "km" and "kg" follow it after a no-break space (U+00A0) so a line never
//! breaks between the two.
//!
//! ponytail: numbers spelled across several words ("due mila", "mille e
//! cinquecento", "cento venti") are left as spoken — Whisper fuses them, and
//! joining words on "e" is where a conjunction would get swallowed. Also left
//! alone: "milioni"/"miliardi" (Italian writes "2 milioni", which is what
//! leaving them gives), ordinals ("venticinquesimo"), "l'una" and "meno un
//! quarto" in times, and decimals said with "virgola".

use crate::core::format::{key, words};

/// Units that settle a lone number, as spoken → what follows the digits.
const UNIT_WORDS: &[(&str, &str)] = &[
    ("per cento", "%"),
    ("percento", "%"),
    ("euro", "\u{a0}\u{20ac}"),
    ("gradi", "\u{b0}"),
    ("grado", "\u{b0}"),
    ("chilometri", "\u{a0}km"),
    ("chilogrammi", "\u{a0}kg"),
    ("chili", "\u{a0}kg"),
];

/// The words before a clock time that make it one: "alle quindici e trenta".
const CLOCK_ARTICLES: &[&str] = &["le", "alle", "dalle", "sulle"];

const UNITS: &[(&str, u64)] = &[
    ("uno", 1),
    ("due", 2),
    ("tre", 3),
    ("quattro", 4),
    ("cinque", 5),
    ("sei", 6),
    ("sette", 7),
    ("otto", 8),
    ("nove", 9),
];

const TEENS: &[(&str, u64)] = &[
    ("dieci", 10),
    ("undici", 11),
    ("dodici", 12),
    ("tredici", 13),
    ("quattordici", 14),
    ("quindici", 15),
    ("sedici", 16),
    ("diciassette", 17),
    ("diciotto", 18),
    ("diciannove", 19),
];

const TENS: &[(&str, u64)] = &[
    ("venti", 20),
    ("trenta", 30),
    ("quaranta", 40),
    ("cinquanta", 50),
    ("sessanta", 60),
    ("settanta", 70),
    ("ottanta", 80),
    ("novanta", 90),
];

/// Convert spelled-out numbers, times and units to their written forms.
pub fn apply(text: &str) -> String {
    // Line by line, so the "a capo" the punctuation stage inserted survives.
    text.split('\n')
        .map(apply_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_line(line: &str) -> String {
    let words = words(line);
    let keys: Vec<String> = words
        .iter()
        .map(|w| key(w).replace(['é', 'è'], "e"))
        .collect();

    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        let found = match_time(&words, &keys, i).or_else(|| {
            let (value, morphemes) = parse_word(&keys[i])?;
            match_unit(&words, &keys, i, value)
                .or_else(|| (morphemes > 1 && !capitalised(words[i])).then(|| (1, grouped(value))))
        });
        match found {
            Some((len, written)) => {
                out.push(format!(
                    "{}{written}{}",
                    lead(words[i]),
                    trail(words[i + len - 1])
                ));
                i += len;
            }
            None => {
                out.push(words[i].to_string());
                i += 1;
            }
        }
    }

    out.join(" ")
}

/// The value of `word` and how many morphemes it was built from, if the whole
/// word is a number.
fn parse_word(word: &str) -> Option<(u64, usize)> {
    number(word)
        .into_iter()
        .find(|(_, _, rest)| rest.is_empty())
        .map(|(value, morphemes, _)| (value, morphemes))
}

// Each parser below returns every way the front of the word can be read as a
// number: (value, morphemes, what is left). "tre" is a prefix of "tredici" and
// "trenta" alike, so only the caller that insists on an empty remainder can
// tell which reading was right.

type Readings<'a> = Vec<(u64, usize, &'a str)>;

/// Up to 999 999: "mille", "duemila", "duemilaventisei".
fn number(s: &str) -> Readings<'_> {
    let mut out = below_1000(s);
    // "mille" stands alone; "mila" needs a multiplier of two or more.
    let mut thousands: Readings = s
        .strip_prefix("mille")
        .map(|rest| (1000, 1, rest))
        .into_iter()
        .collect();
    for (v, n, r) in below_1000(s) {
        if let Some(rest) = r.strip_prefix("mila").filter(|_| v >= 2) {
            thousands.push((v * 1000, n + 1, rest));
        }
    }
    for (v, n, rest) in thousands {
        out.push((v, n, rest));
        out.extend(
            below_1000(rest)
                .into_iter()
                .map(|(w, m, r)| (v + w, n + m, r)),
        );
    }
    out
}

/// Hundreds: "cento", "trecento", "centoventi", and "centottanta" with the
/// vowel elided.
fn below_1000(s: &str) -> Readings<'_> {
    let mut out = below_100(s);
    let multipliers =
        std::iter::once(("", 1)).chain(UNITS.iter().copied().filter(|(_, v)| *v >= 2));
    for (m, mv) in multipliers {
        let Some(r) = s.strip_prefix(m) else { continue };
        let n = usize::from(!m.is_empty()) + 1;
        let hundreds = mv * 100;
        if let Some(rest) = r.strip_prefix("cento") {
            out.push((hundreds, n, rest));
            out.extend(
                below_100(rest)
                    .into_iter()
                    .map(|(v, k, r)| (hundreds + v, n + k, r)),
            );
        }
        // "cent" before a vowel: centuno, centotto, centottanta.
        if let Some(rest) = r.strip_prefix("cent").filter(|r| r.starts_with(['o', 'u'])) {
            out.extend(
                below_100(rest)
                    .into_iter()
                    .map(|(v, k, r)| (hundreds + v, n + k, r)),
            );
        }
    }
    out
}

/// 1 to 99.
fn below_100(s: &str) -> Readings<'_> {
    let mut out: Readings = UNITS
        .iter()
        .chain(TEENS)
        .filter_map(|&(w, v)| s.strip_prefix(w).map(|rest| (v, 1, rest)))
        .collect();
    for &(w, tens) in TENS {
        if let Some(rest) = s.strip_prefix(w) {
            out.push((tens, 1, rest));
            // The tens keep their vowel before every unit but "uno" and "otto".
            for &(u, v) in UNITS.iter().filter(|(u, _)| !matches!(*u, "uno" | "otto")) {
                if let Some(r) = rest.strip_prefix(u) {
                    out.push((tens + v, 2, r));
                }
            }
        }
        // ...and drop it before those two: ventuno, ventun, ventuna, trentotto.
        if let Some(rest) = s.strip_prefix(&w[..w.len() - 1]) {
            for (u, v) in [("uno", 1), ("una", 1), ("un", 1), ("otto", 8)] {
                if let Some(r) = rest.strip_prefix(u) {
                    out.push((tens + v, 2, r));
                }
            }
        }
    }
    out
}

/// A clock time after an article: "alle quindici e trenta", "le tre e mezza".
///
/// The article is the evidence. "quindici e trenta" alone is as likely "fifteen
/// and thirty"; "alle quindici e trenta" is only ever a time. The article
/// itself is not part of the match and stays as spoken, giving "alle 15:30".
fn match_time(words: &[&str], keys: &[String], i: usize) -> Option<(usize, String)> {
    if i == 0 || !CLOCK_ARTICLES.contains(&keys[i - 1].as_str()) {
        return None;
    }
    let (hour, _) = parse_word(&keys[i]).filter(|(h, _)| *h <= 24)?;
    if keys.get(i + 1)?.as_str() != "e" {
        return None;
    }

    let after: Vec<&str> = keys
        .iter()
        .skip(i + 2)
        .take(3)
        .map(String::as_str)
        .collect();
    let (minute_len, minute) = match after.as_slice() {
        ["mezza" | "mezzo", ..] => (1, 30),
        ["un", "quarto", ..] => (2, 15),
        ["tre", "quarti", ..] => (2, 45),
        [m, rest @ ..] => {
            let (minute, _) = parse_word(m)?;
            // "uno" is the article; "le due e dieci minuti" spelled a duration.
            if *m == "uno"
                || !(1..=59).contains(&minute)
                || matches!(rest.first(), Some(&("minuti" | "minuto")))
            {
                return None;
            }
            (1, minute)
        }
        [] => return None,
    };

    let len = 2 + minute_len;
    joined(&words[i - 1..i + len]).then(|| (len, format!("{hour}:{minute:02}")))
}

/// A unit following the number at `i`, returning the words spanned by both and
/// the written form.
fn match_unit(words: &[&str], keys: &[String], i: usize, value: u64) -> Option<(usize, String)> {
    // "uno per cento" — the lone article gets no help from a unit.
    if keys[i] == "uno" {
        return None;
    }
    UNIT_WORDS.iter().find_map(|(phrase, suffix)| {
        let end = i + 1 + phrase.split(' ').count();
        let spoken = keys.get(i + 1..end)?.join(" ");
        // A number after the unit means a larger amount was spoken: "dieci
        // euro cinquanta" is 10,50 €, and "10 € cinquanta" would be wrong.
        let more = keys.get(end).is_some_and(|k| parse_word(k).is_some());
        (spoken == *phrase && joined(&words[i..end]) && !more)
            .then(|| (end - i, format!("{}{suffix}", grouped(value))))
    })
}

/// Whether the word was written with a capital, which for a compound means a
/// name or a sentence start. See the module docs.
fn capitalised(word: &str) -> bool {
    word.chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(char::is_uppercase)
}

/// Digits grouped the Italian way: a dot every three, from five digits up.
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    if digits.len() < 5 {
        return digits;
    }
    let mut out = String::new();
    for (n, c) in digits.chars().enumerate() {
        if n > 0 && (digits.len() - n).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Punctuation the decoder put in front of a word, which [`key`] strips for
/// matching but the output has to keep.
fn lead(word: &str) -> &str {
    let rest = word.trim_start_matches(|c: char| c.is_ascii_punctuation());
    &word[..word.len() - rest.len()]
}

/// Punctuation the decoder put after a word: "venticinque." must stay "25.".
fn trail(word: &str) -> &str {
    &word[word
        .trim_end_matches(|c: char| c.is_ascii_punctuation())
        .len()..]
}

/// Whether `span` reads as one unbroken run: punctuation may sit before its
/// first word and after its last, never in between.
fn joined(span: &[&str]) -> bool {
    span.iter().enumerate().all(|(n, w)| {
        (n == 0 || lead(w).is_empty()) && (n + 1 == span.len() || trail(w).is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fused_compounds_become_digits() {
        assert_eq!(apply("ne servono venticinque"), "ne servono 25");
        assert_eq!(apply("trecentoquarantadue"), "342");
        assert_eq!(apply("nel duemilaventisei"), "nel 2026");
        assert_eq!(apply("millenovecentonovantanove"), "1999");
        assert_eq!(apply("centouno"), "101");
        assert_eq!(apply("trecento persone"), "300 persone");
    }

    /// Elision is grammar, so it is parsed, not listed.
    #[test]
    fn elided_vowels_are_understood() {
        assert_eq!(apply("ventuno"), "21");
        assert_eq!(apply("trentotto"), "38");
        assert_eq!(apply("ventun anni"), "21 anni");
        assert_eq!(apply("centottanta"), "180");
        assert_eq!(apply("ventitré"), "23");
        assert_eq!(apply("trentatre"), "33");
        // The unelided spellings are not Italian.
        assert_eq!(apply("ventiuno"), "ventiuno");
    }

    #[test]
    fn large_numbers_are_grouped_with_a_dot() {
        assert_eq!(apply("venticinquemila"), "25.000");
        assert_eq!(apply("centomila"), "100.000");
        assert_eq!(apply("duemila"), "2000");
    }

    /// The rule the whole module hangs on.
    #[test]
    fn a_simple_number_word_stays_a_word() {
        for said in [
            "tre",
            "dieci",
            "cento",
            "mille",
            "diciotto",
            "tu sei bravo",
            "i venti del nord",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn articles_are_not_one() {
        for said in [
            "un libro",
            "uno dei migliori",
            "una volta",
            "uno per cento",
            "mille e una notte",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    /// Centuries, a car and a year of protest are compounds too.
    #[test]
    fn capitalised_compounds_are_names() {
        for said in [
            "il Novecento",
            "la Cinquecento",
            "il Sessantotto",
            "Duemila",
        ] {
            assert_eq!(apply(said), said);
        }
        // A unit still makes it a quantity.
        assert_eq!(apply("Venti per cento"), "20%");
    }

    /// Words that start like numbers must use up the whole word to count.
    #[test]
    fn words_that_only_begin_like_numbers_are_left_alone() {
        for said in [
            "la quarantena",
            "il ventilatore",
            "prendo il treno",
            "un venticinquenne",
            "settembre",
            "Milano",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn a_unit_settles_a_lone_number() {
        assert_eq!(apply("tre per cento"), "3%");
        assert_eq!(apply("venticinque percento"), "25%");
        assert_eq!(apply("dieci euro"), "10\u{a0}\u{20ac}");
        assert_eq!(apply("venti gradi"), "20\u{b0}");
        assert_eq!(apply("cinque chilometri"), "5\u{a0}km");
        assert_eq!(apply("due chili"), "2\u{a0}kg");
        // Cents after the unit: the amount is left whole rather than split.
        assert_eq!(apply("dieci euro cinquanta"), "dieci euro cinquanta");
    }

    #[test]
    fn clock_times_after_an_article() {
        assert_eq!(apply("alle quindici e trenta"), "alle 15:30");
        assert_eq!(apply("le tre e mezza"), "le 3:30");
        assert_eq!(apply("dalle nove e un quarto"), "dalle 9:15");
        assert_eq!(apply("le ventitré e cinque"), "le 23:05");
    }

    /// "e" is "and" everywhere a time is not plainly meant.
    #[test]
    fn e_is_only_a_clock_join_after_an_article() {
        for said in [
            "tre e quattro",
            "tra le due e le tre",
            "le due e dieci minuti",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn punctuation_around_a_number_is_kept() {
        assert_eq!(apply("ne ho venticinque."), "ne ho 25.");
        assert_eq!(apply("(trentuno)"), "(31)");
        assert_eq!(apply("tre, per cento"), "tre, per cento");
    }

    #[test]
    fn ordinary_sentences_are_left_alone() {
        for said in ["il punto è chiaro", "ci vediamo domani", "sono le tre"] {
            assert_eq!(apply(said), said);
        }
    }

    /// Through the whole pipeline, with every stage on.
    #[test]
    fn output_survives_every_stage() {
        use crate::core::format::FormatOptions;
        let all = FormatOptions {
            cleanup: true,
            spoken_punctuation: true,
            numbers: true,
            tidy: true,
        };
        assert_eq!(
            crate::core::format::apply(
                "abbiamo venduto venticinque pezzi virgola il tre per cento a capo alle quindici e trenta",
                all,
                Some("it")
            ),
            "Abbiamo venduto 25 pezzi, il 3%\nAlle 15:30"
        );
    }
}
