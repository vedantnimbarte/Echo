//! Spelled-out numbers in German.
//!
//! German writes a number as one word, units before tens: "einundzwanzig",
//! "dreihundertfünfundvierzig", "zweitausendsechsundzwanzig". That is the easy
//! half. Such a word is built from a few dozen morphemes by a small grammar, so
//! it is parsed by morpheme rather than listed, and a word that parses is
//! unambiguous: nobody says "dreihundertfünfundvierzig" meaning anything else.
//!
//! **The rule from the English module holds: never convert a lone simple
//! word.** A compound of two or more number morphemes becomes digits; "drei",
//! "zehn", "zwanzig", "hundert" stay words, exactly as "three" and "hundred"
//! do. The teens count as one morpheme ("dreizehn" is as simple as
//! "thirteen"), so they stay too. A unit behind settles a lone number: "fünf
//! Prozent".
//!
//! **Articles are the trap.** "ein" is both "a" and "one", and it is the article
//! overwhelmingly more often. A standalone "ein" is therefore never a number —
//! not before a unit ("ein Prozent" stays), not before a scale word ("ein
//! hundert" stays). Inside a fused word it is unambiguous ("einundzwanzig",
//! "einhundert"), and the single standalone exception is "ein Uhr", where the
//! article would have to be "eine". "eine", "einen", "einem" and "einer" are not
//! number morphemes at all, so they never parse. Matching is on whole tokens,
//! which is what keeps "einfach", "keinen", "Viertel" and "Achtung" out: a word
//! must split into morphemes *completely*, and "fach" is not one.
//!
//! **Whisper is untidy with compounds.** It sometimes splits them ("zwei
//! tausend", "drei hundert") and sometimes drops umlauts ("funf", "zwolf") or
//! writes the Swiss "dreissig". Umlauts and ß are folded before matching. Split
//! words are rejoined only across a scale word — "hundert" or "tausend" on one
//! side of the gap — because that is where Whisper splits, and because "vier
//! zehn" is more likely two numbers read out than a mangled "vierzehn". A gap
//! with punctuation or a line break in it is never bridged. Nor is a standalone
//! "und": "zwei und zwanzig" may well be a list.
//!
//! **Years need no special case.** German says "neunzehnhundertneunundneunzig"
//! and "zweitausendsechsundzwanzig", both of which the grammar reads as the
//! plain values 1999 and 2026. Digits are written without grouping, which is
//! right for years and acceptable for quantities.
//!
//! **Ordinals are for dates.** "am fünfundzwanzigsten Juni" is written "am 25.
//! Juni" — the full stop *is* the ordinal. Two things make that dangerous, and
//! the conversion only happens when neither applies:
//!
//! - The tidy stage treats "." as a sentence end and capitalises the next
//!   letter. So the next word must already start with a capital, which it does
//!   when it is a month or any other noun ("zum zweiundzwanzigsten Mal"), and
//!   then capitalising it changes nothing. "am fünfundzwanzigsten des Monats"
//!   stays words rather than becoming "am 25. Des Monats".
//! - An ordinal at the end of a sentence would write "25.." or swallow the full
//!   stop, so an ordinal carrying punctuation stays words.
//!
//! As in English only compounds convert: "ersten", "zweiten", "zwanzigsten" stay
//! words. Only the "-st-" ordinals are read ("fünfundzwanzigsten",
//! "hundertsten"), which covers every compound day of the month; the "-t-" ones
//! after a unit ("hundertdritten") are left alone.
//!
//! **Units follow German typography.** DIN 5008 puts a space between a number
//! and its unit, "%" and "€" included: "25 %", "5 €", "10 km". That space is
//! written as a no-break space (U+00A0), for two reasons. It is what the
//! standard recommends, so the number never wraps away from its unit; and the
//! tidy stage strips an ordinary space before "%", so an ordinary space would
//! not survive the pipeline. Degrees are the exception and attach: "30°".
//!
//! **Clock times only in the unambiguous shape.** "drei Uhr zwanzig" is "3:20
//! Uhr", "drei Uhr" is "3 Uhr". The minute must be ten or more, because "um drei
//! Uhr zwei Kollegen treffen" is a count of colleagues, not 3:02.
//!
//! ponytail: "halb drei" (2:30, not 3:30), "Viertel nach drei" and "zehn vor
//! drei" are left as words. They need the hour arithmetic *and* a guess at
//! whether "Viertel" is a time at all, and a wrong clock time is exactly the
//! kind of error a reader cannot see. Millions ("drei Millionen") are left too —
//! German writes "3 Millionen", which needs the noun kept, not multiplied out.

use crate::core::format::{key, words};

/// One morpheme of a fused number word. The joining "und" carries no value but
/// decides what the parts around it may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Part {
    Num(u64),
    And,
}

use Part::{And, Num};

/// German number morphemes, spelled as [`fold`] leaves them: no umlauts, "ss"
/// for "ß". Teens are whole morphemes rather than unit plus "zehn", because
/// "siebzehn" is not "sieben" plus anything.
const MORPHEMES: &[(&str, Part)] = &[
    ("null", Num(0)),
    ("ein", Num(1)),
    ("eins", Num(1)),
    ("zwei", Num(2)),
    ("drei", Num(3)),
    ("vier", Num(4)),
    ("funf", Num(5)),
    ("sechs", Num(6)),
    ("sieben", Num(7)),
    ("acht", Num(8)),
    ("neun", Num(9)),
    ("zehn", Num(10)),
    ("elf", Num(11)),
    ("zwolf", Num(12)),
    ("dreizehn", Num(13)),
    ("vierzehn", Num(14)),
    ("funfzehn", Num(15)),
    ("sechzehn", Num(16)),
    ("siebzehn", Num(17)),
    ("achtzehn", Num(18)),
    ("neunzehn", Num(19)),
    ("zwanzig", Num(20)),
    ("dreissig", Num(30)),
    ("vierzig", Num(40)),
    ("funfzig", Num(50)),
    ("sechzig", Num(60)),
    ("siebzig", Num(70)),
    ("achtzig", Num(80)),
    ("neunzig", Num(90)),
    ("hundert", Num(100)),
    ("tausend", Num(1_000)),
    ("und", And),
];

/// Standalone words that look like a number but are the indefinite article.
/// Only "ein" is a morpheme; the inflected forms never parse in the first place.
const ARTICLES: &[&str] = &["ein"];

/// Units written as a symbol: spoken form, symbol, and whether a no-break space
/// separates them. See the module docs for why the space is a no-break one.
const UNITS: &[(&str, &str, bool)] = &[
    ("prozent", "%", true),
    ("euro", "\u{20ac}", true),
    ("euros", "\u{20ac}", true),
    ("grad", "\u{b0}", false),
    ("kilometer", "km", true),
    ("kilometern", "km", true),
    ("kilogramm", "kg", true),
    ("kilo", "kg", true),
];

/// Inflected endings of an "-st-" ordinal: "fünfundzwanzigste", "-sten",
/// "-ster", "-stes", "-stem".
const ORDINAL_ENDINGS: &[&str] = &["ste", "sten", "ster", "stes", "stem"];

/// Convert spelled-out German numbers, dates, times and units to their written
/// forms.
pub fn apply(text: &str) -> String {
    let w = Words::new(text);
    let mut edits = Vec::new();
    let mut i = 0;

    while i < w.words.len() {
        // A time first, so "drei Uhr zwanzig" is not read as a lone "drei".
        // Then an ordinal, which a cardinal would never match anyway, but which
        // is cheaper to rule out on its own.
        match match_time(&w, i)
            .or_else(|| match_ordinal(&w, i))
            .or_else(|| match_number(&w, i))
        {
            Some((len, written)) => {
                edits.push((i, len, written));
                i += len;
            }
            None => i += 1,
        }
    }
    w.rewrite(&edits)
}

/// A cardinal at `i`, with the unit behind it if there is one.
fn match_number(w: &Words, i: usize) -> Option<(usize, String)> {
    let (len, value, pieces) = number_at(w, i, MORPHEMES, ARTICLES)?;
    let last = i + len - 1;

    if w.joined(last) {
        if let Some((_, symbol, spaced)) = UNITS.iter().find(|(u, ..)| w.key(last + 1) == Some(*u))
        {
            let gap = if *spaced { "\u{a0}" } else { "" };
            return Some((len + 1, format!("{value}{gap}{symbol}")));
        }
    }
    (pieces > 1).then(|| (len, value.to_string()))
}

/// A compound "-st-" ordinal at `i` that is safe to write as "25." — see the
/// module docs for the two conditions.
fn match_ordinal(w: &Words, i: usize) -> Option<(usize, String)> {
    let key = w.key(i)?;
    let stem = ORDINAL_ENDINGS
        .iter()
        .find_map(|ending| key.strip_suffix(ending))?;
    let parts = split(stem, MORPHEMES)?;
    if pieces(&parts) < 2 || !w.joined(i) || !w.starts_upper(i + 1) {
        return None;
    }
    Some((1, format!("{}.", value(&parts)?)))
}

/// "drei Uhr" or "drei Uhr zwanzig".
fn match_time(w: &Words, i: usize) -> Option<(usize, String)> {
    // "ein Uhr" is the one place a standalone "ein" is a number: the article
    // before "Uhr" would be "eine".
    let hour = if w.raw(i)? == "ein" {
        1
    } else {
        value(&parts_at(w, i, MORPHEMES, ARTICLES)?)?
    };
    if hour > 24 || !w.joined(i) || w.key(i + 1)? != "uhr" {
        return None;
    }

    let minute = w
        .joined(i + 1)
        .then(|| parts_at(w, i + 2, MORPHEMES, ARTICLES))
        .flatten()
        .and_then(|parts| value(&parts))
        .filter(|m| (10..60).contains(m));

    Some(match minute {
        Some(m) => (3, format!("{hour}:{m:02} Uhr")),
        None => (2, format!("{hour} Uhr")),
    })
}

// ---------------------------------------------------------------------------
// Shared with Dutch.
//
// Dutch builds numbers exactly as German does — units before tens joined by
// "en", fused with "honderd" and "duizend" — so the grammar and the word
// bookkeeping live here once and `nl` supplies its own morphemes, articles and
// written forms.
// ---------------------------------------------------------------------------

/// Lowercase-and-trimmed words with diacritics folded away, so "fünf", "funf",
/// "tweeëntwintig" and "tweeentwintig" all compare equal to the morpheme tables.
pub(super) fn fold(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for c in key.chars() {
        match c {
            'ö' => out.push('o'),
            'ü' => out.push('u'),
            'ß' => out.push_str("ss"),
            'ë' | 'é' => out.push('e'),
            _ => out.push(c),
        }
    }
    out
}

/// Split a whole word into morphemes, or `None` if any of it is not one.
///
/// Longest match first, which is what reads "vierzehn" as one morpheme rather
/// than "vier" + "zehn", and "negentien" rather than "negen" + "tien". It never
/// has to backtrack: wherever the longer morpheme wins, the shorter reading
/// would not have been a valid number anyway.
pub(super) fn split(word: &str, table: &[(&str, Part)]) -> Option<Vec<Part>> {
    let mut parts = Vec::new();
    let mut rest = word;
    while !rest.is_empty() {
        let (text, part) = table
            .iter()
            .filter(|(m, _)| rest.starts_with(m))
            .max_by_key(|(m, _)| m.len())?;
        parts.push(*part);
        rest = &rest[text.len()..];
    }
    (!parts.is_empty()).then_some(parts)
}

/// How many number morphemes a word has — the "is it a compound" test. The
/// joining "und"/"en" does not count: "einundzwanzig" is two numbers fused.
pub(super) fn pieces(parts: &[Part]) -> usize {
    parts.iter().filter(|p| matches!(p, Num(_))).count()
}

/// The value of a well-formed sequence of morphemes, or `None` when the
/// sequence is not a number ("dreivier", "undzwanzig", "tausendtausend").
///
/// `[below-thousand] tausend [below-thousand]`, where each below-thousand is
/// `[1-19] hundert [below-hundred]` or a below-hundred. The multiplier before
/// "hundert" runs to nineteen because that is how years are said:
/// "neunzehnhundert".
pub(super) fn value(parts: &[Part]) -> Option<u64> {
    if parts == [Num(0)] {
        return Some(0);
    }
    let Some(at) = parts.iter().position(|p| *p == Num(1_000)) else {
        return below_thousand(parts);
    };
    let (before, after) = (&parts[..at], &parts[at + 1..]);
    let thousands = if before.is_empty() {
        1
    } else {
        below_thousand(before)?
    };
    let rest = if after.is_empty() {
        0
    } else {
        below_thousand(after)?
    };
    Some(thousands * 1_000 + rest)
}

fn below_thousand(parts: &[Part]) -> Option<u64> {
    let Some(at) = parts.iter().position(|p| *p == Num(100)) else {
        return below_hundred(parts);
    };
    let hundreds = if at == 0 {
        1
    } else {
        below_hundred(&parts[..at]).filter(|h| *h < 20)?
    };
    let rest = &parts[at + 1..];
    let rest = if rest.is_empty() {
        0
    } else {
        below_hundred(rest)?
    };
    Some(hundreds * 100 + rest)
}

/// One to ninety-nine: a single morpheme, or a unit joined to a tens.
fn below_hundred(parts: &[Part]) -> Option<u64> {
    match *parts {
        [Num(n)] if (1..100).contains(&n) => Some(n),
        [Num(unit), And, Num(tens)]
            if (1..10).contains(&unit) && (20..100).contains(&tens) && tens % 10 == 0 =>
        {
            Some(unit + tens)
        }
        _ => None,
    }
}

fn is_scale(part: Part) -> bool {
    matches!(part, Num(100) | Num(1_000))
}

/// The morphemes of the word at `i`, unless it is a standalone article.
pub(super) fn parts_at(
    w: &Words,
    i: usize,
    table: &[(&str, Part)],
    articles: &[&str],
) -> Option<Vec<Part>> {
    if articles.contains(&w.raw(i)?) {
        return None;
    }
    split(w.key(i)?, table)
}

/// The cardinal starting at word `i`: how many words it spans, its value, and
/// how many number morphemes it was built from.
///
/// Usually one fused word. A compound Whisper split ("zwei tausend", "twee
/// honderd") is rejoined across a scale word, and the longest run of words that
/// still parses wins.
pub(super) fn number_at(
    w: &Words,
    i: usize,
    table: &[(&str, Part)],
    articles: &[&str],
) -> Option<(usize, u64, usize)> {
    let first = parts_at(w, i, table, articles)?;
    let mut left = *first.last()?;
    let mut run = vec![first];

    while w.joined(i + run.len() - 1) {
        let Some(next) = parts_at(w, i + run.len(), table, articles) else {
            break;
        };
        if !(is_scale(left) || is_scale(next[0])) {
            break;
        }
        left = *next.last()?;
        run.push(next);
    }

    (1..=run.len()).rev().find_map(|len| {
        let parts = run[..len].concat();
        Some((len, value(&parts)?, pieces(&parts)))
    })
}

/// The transcript as words, remembering where each one sits so that a rewrite
/// only replaces the words it converts. Everything else — line breaks from the
/// punctuation stage, double spaces, punctuation stuck to a word — comes back
/// byte for byte.
pub(super) struct Words<'a> {
    text: &'a str,
    pub(super) words: Vec<&'a str>,
    /// [`key`]: lowercased, surrounding punctuation trimmed.
    raw: Vec<String>,
    /// `raw` with diacritics folded, for the morpheme tables.
    keys: Vec<String>,
}

impl<'a> Words<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        let words = words(text);
        let raw: Vec<String> = words.iter().map(|w| key(w)).collect();
        let keys = raw.iter().map(|k| fold(k)).collect();
        Self {
            text,
            words,
            raw,
            keys,
        }
    }

    /// The word's comparison key, before folding. Articles are checked here, so
    /// Dutch "één" (the numeral) is not mistaken for "een" (the article).
    pub(super) fn raw(&self, i: usize) -> Option<&str> {
        self.raw.get(i).map(String::as_str)
    }

    pub(super) fn key(&self, i: usize) -> Option<&str> {
        self.keys.get(i).map(String::as_str)
    }

    /// Byte offset of word `i`. `words` hands back slices of `text` itself, so
    /// the offset is the distance between the two.
    fn start(&self, i: usize) -> usize {
        self.words[i].as_ptr() as usize - self.text.as_ptr() as usize
    }

    fn end(&self, i: usize) -> usize {
        self.start(i) + self.words[i].len()
    }

    /// Whether words `i` and `i + 1` belong to the same phrase: nothing but
    /// spaces between them. A comma or a line break means the speaker moved on,
    /// so "drei, hundert" is two things and is never joined.
    pub(super) fn joined(&self, i: usize) -> bool {
        let (Some(a), Some(b)) = (self.words.get(i), self.words.get(i + 1)) else {
            return false;
        };
        let bare = |c: char| !c.is_ascii_punctuation();
        a.ends_with(bare)
            && b.starts_with(bare)
            && !self.text[self.end(i)..self.start(i + 1)].contains('\n')
    }

    /// Whether word `i` starts with a capital letter.
    pub(super) fn starts_upper(&self, i: usize) -> bool {
        self.words
            .get(i)
            .and_then(|w| w.chars().next())
            .is_some_and(char::is_uppercase)
    }

    /// Replace each `(first word, word count, written form)` and keep the rest.
    /// Punctuation outside the replaced words is carried over, so
    /// "fünfundzwanzig." becomes "25." rather than losing its full stop.
    pub(super) fn rewrite(&self, edits: &[(usize, usize, String)]) -> String {
        let punct = |c: char| c.is_ascii_punctuation();
        let mut out = String::with_capacity(self.text.len());
        let mut copied = 0;

        for (i, len, written) in edits {
            let (first, last) = (self.words[*i], self.words[i + len - 1]);
            out.push_str(&self.text[copied..self.start(*i)]);
            out.push_str(&first[..first.len() - first.trim_start_matches(punct).len()]);
            out.push_str(written);
            out.push_str(&last[last.trim_end_matches(punct).len()..]);
            copied = self.end(i + len - 1);
        }
        out.push_str(&self.text[copied..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::format::FormatOptions;

    #[test]
    fn fused_compounds_become_digits() {
        assert_eq!(
            apply("wir brauchen fünfundzwanzig Stück"),
            "wir brauchen 25 Stück"
        );
        assert_eq!(apply("einundzwanzig"), "21");
        assert_eq!(apply("dreihundertfünfundvierzig Seiten"), "345 Seiten");
        assert_eq!(apply("einhundert"), "100");
        assert_eq!(apply("zweitausendvierhundert"), "2400");
        assert_eq!(apply("hunderttausend"), "100000");
        // Capitalised at the start of a sentence, still a number.
        assert_eq!(apply("Siebenundsiebzig Leute"), "77 Leute");
    }

    /// The rule the module hangs on, as in English: one simple word stays one.
    #[test]
    fn a_lone_simple_word_stays_a_word() {
        for said in [
            "drei Tage",
            "zehn Minuten",
            "hundert Mal",
            "tausend Dank",
            "zwanzig",
            "dreizehn",
            "eins nach dem anderen",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    /// "ein" is the article far more often than the number, so a standalone
    /// one never becomes 1 — not even where a unit or scale word follows.
    #[test]
    fn articles_are_not_numbers() {
        for said in [
            "ein Hund und eine Katze",
            "einen Moment bitte",
            "mit einem Freund",
            "ein Prozent mehr",
            "ein hundert",
            "ein tausend Mal",
            "einer von vielen",
        ] {
            assert_eq!(apply(said), said);
        }
        // Fused, it is unambiguous.
        assert_eq!(apply("eintausend"), "1000");
    }

    /// Whole tokens only: a word must be made of number morphemes end to end.
    #[test]
    fn words_containing_number_morphemes_are_left_alone() {
        for said in [
            "das ist einfach",
            "ich habe keinen",
            "Acht geben",
            "ein Viertel davon",
            "Achtung",
            "dreimal",
            "Tausende kamen",
            "die erste und die besten",
            "am nächsten Tag",
            "Dreieck",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn split_compounds_are_rejoined_across_a_scale_word() {
        assert_eq!(apply("zwei tausend"), "2000");
        assert_eq!(apply("drei hundert Leute"), "300 Leute");
        assert_eq!(apply("zweitausend sechsundzwanzig"), "2026");
        assert_eq!(apply("zwei tausend drei hundert"), "2300");
    }

    #[test]
    fn splits_that_are_not_compounds_stay_apart() {
        // No scale word at the gap: more likely two numbers read out.
        assert_eq!(apply("vier zehn"), "vier zehn");
        assert_eq!(apply("zwei und zwanzig"), "zwei und zwanzig");
        // Punctuation or a line break means the speaker moved on.
        assert_eq!(apply("drei, hundert"), "drei, hundert");
        assert_eq!(apply("drei\nhundert"), "drei\nhundert");
    }

    #[test]
    fn missing_umlauts_and_swiss_spelling_are_accepted() {
        assert_eq!(apply("funfundzwanzig"), "25");
        assert_eq!(apply("zwolfhundert"), "1200");
        assert_eq!(apply("dreissig Grad"), "30\u{b0}");
        assert_eq!(apply("funfzig Prozent"), "50\u{a0}%");
    }

    #[test]
    fn years_are_plain_values() {
        assert_eq!(apply("seit neunzehnhundertneunundneunzig"), "seit 1999");
        assert_eq!(apply("im Jahr zweitausendsechsundzwanzig"), "im Jahr 2026");
    }

    /// The full stop is the ordinal, so it is only written where the tidy
    /// stage cannot misread it — see the module docs.
    #[test]
    fn compound_ordinals_become_dotted_digits_before_a_noun() {
        assert_eq!(apply("am fünfundzwanzigsten Juni"), "am 25. Juni");
        assert_eq!(apply("zum zweiundzwanzigsten Mal"), "zum 22. Mal");
        assert_eq!(apply("der einunddreißigste Dezember"), "der 31. Dezember");
    }

    #[test]
    fn ordinals_that_are_not_safe_stay_words() {
        for said in [
            // Lone ordinals are prose, as in English.
            "am ersten Juni",
            "zum zweiten Mal",
            "am zwanzigsten Juni",
            // Followed by a lowercase word, tidy would capitalise it.
            "am fünfundzwanzigsten des Monats",
            // At the end of a sentence it would collide with the full stop.
            "wir treffen uns am fünfundzwanzigsten.",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    /// DIN 5008: a (no-break) space before "%" and "€", none before "°".
    #[test]
    fn a_unit_settles_a_lone_number() {
        assert_eq!(apply("um fünf Prozent"), "um 5\u{a0}%");
        assert_eq!(apply("kostet zwanzig Euro"), "kostet 20\u{a0}\u{20ac}");
        assert_eq!(apply("dreißig Grad heute"), "30\u{b0} heute");
        assert_eq!(apply("zehn Kilometer"), "10\u{a0}km");
        assert_eq!(apply("fünf Kilo"), "5\u{a0}kg");
        assert_eq!(apply("zweihundertfünfzig Euro"), "250\u{a0}\u{20ac}");
    }

    #[test]
    fn clock_times_in_the_unambiguous_shape() {
        assert_eq!(apply("um drei Uhr zwanzig"), "um 3:20 Uhr");
        assert_eq!(apply("um fünfzehn Uhr fünfundvierzig"), "um 15:45 Uhr");
        assert_eq!(apply("um drei Uhr"), "um 3 Uhr");
        // "ein Uhr" is a time: the article would be "eine".
        assert_eq!(apply("um ein Uhr"), "um 1 Uhr");
        // A small number after "Uhr" is a count, not minutes.
        assert_eq!(
            apply("um drei Uhr zwei Kollegen treffen"),
            "um 3 Uhr zwei Kollegen treffen"
        );
    }

    #[test]
    fn relative_clock_times_are_left_alone() {
        // "halb drei" is 2:30, and "Viertel" may not be a time at all.
        for said in ["um halb drei", "Viertel nach drei", "zehn vor acht"] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn punctuation_and_line_breaks_survive() {
        assert_eq!(apply("es waren fünfundzwanzig."), "es waren 25.");
        assert_eq!(apply("(dreihundert)"), "(300)");
        assert_eq!(apply("fünf Prozent, sagt er"), "5\u{a0}%, sagt er");
        assert_eq!(apply("einundzwanzig\n\nzweiundzwanzig"), "21\n\n22");
    }

    #[test]
    fn ordinary_sentences_are_untouched() {
        for said in [
            "Das ist ein ganz normaler Satz.",
            "Wir sehen uns morgen  früh",
            "Kannst du mir einen Gefallen tun?",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    /// The written forms have to survive every later stage, not just this one:
    /// tidy strips a space before "%" and capitalises after ".".
    #[test]
    fn output_survives_the_whole_pipeline() {
        let all = FormatOptions {
            cleanup: true,
            spoken_punctuation: true,
            numbers: true,
            tidy: true,
        };
        let run = |said| crate::core::format::apply(said, all, Some("de"));
        assert_eq!(
            run("die Preise steigen um fünf Prozent punkt"),
            "Die Preise steigen um 5\u{a0}%."
        );
        assert_eq!(
            run("das kostet zwanzig Euro komma sagt er"),
            "Das kostet 20\u{a0}\u{20ac}, sagt er"
        );
        assert_eq!(
            run("am fünfundzwanzigsten Juni um drei Uhr zwanzig punkt"),
            "Am 25. Juni um 3:20 Uhr."
        );
        assert_eq!(
            run("seit zweitausendsechsundzwanzig dreißig Grad"),
            "Seit 2026 30\u{b0}"
        );
    }
}
