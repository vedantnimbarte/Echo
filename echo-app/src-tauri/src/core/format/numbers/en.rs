//! Spelled-out numbers, times, years and units.
//!
//! Whisper already writes digits most of the time. What it does not do
//! reliably is the cases where speech and writing diverge: "twenty twenty six"
//! is a year, "three thirty pm" is a clock time, "five percent" wants a symbol.
//!
//! **The rule that keeps this safe: never convert a lone small word.** "One of
//! the best" must not become "1 of the best", and "I have won" must never be
//! touched at all. So a number is only written as digits when the speaker gave
//! more than one word of it ("twenty five"), or when a unit follows it and
//! settles the matter ("five percent"). A bare "one", "two", "nine" stays a
//! word — which is also what a style guide would say.
//!
//! Compound ordinals follow the same rule: "the twenty fifth of June" is a
//! date and becomes "the 25th of June", while a lone "fifth" stays a word —
//! "first of all" and "a fifth of the budget" are prose, not numbers.
//!
//! Fractions follow it as well. A plural denominator behind a number word is
//! evidence the way a unit is, so "two thirds" becomes "2/3" and "three eighths"
//! becomes "3/8". But only a proper fraction in lowest terms: "cut it into two
//! halves" and "all four quarters" are counting pieces, not arithmetic, and
//! nobody dictating a quantity says "two fourths". "quarters" also needs "of"
//! behind it — "three quarters of the cake" is a fraction, "the last three
//! quarters" is a financial calendar and "three quarters for the meter" is
//! change. A singular is never enough on its own: "a third of the budget",
//! "half the time", "one third" and "the second half" all stay words, because
//! "a"/"one" before a singular is the lone-small-word case again.
//!
//! "twenty fifths" is not "25th"s: [`match_ordinal`] only knows the singular,
//! so the plural never reaches it. And a number that runs into a denominator
//! without making a fraction ("twenty two thirds" — 22/3, or 20 2/3?) is left
//! as words whole, rather than as "22 thirds", which would be half a conversion.
//!
//! "second" and "seconds" are never denominators. "three seconds" is a unit of
//! time and still goes through the unit path to "3 s"; "wait a second" is prose.
//!
//! Mixed numbers are written "2 1/2", not "2½". Echo types into terminals and
//! code editors, where a vulgar-fraction glyph can be a font gap, and Unicode
//! has ½ and ⅓ but no 3/7, so the glyphs would mix notations within one
//! document. The whole number may be a lone word ("one and a half"), because
//! "and a half" settles it the way a unit does. When a unit follows and the
//! fraction is an exact decimal, the decimal wins — "two and a half percent" is
//! "2.5%", "two and a half dollars" is "$2.50" — because "2 1/2%" is not how
//! anyone writes a rate. The tail must be "a half", "a quarter", "one <nth>" or
//! a proper plural ("two thirds"), never "a third": "step two and a third
//! person checks it" is an ordinal adjective, not 2 1/3.
//!
//! ponytail: still left alone — singular fractions even with a unit ("a third
//! of a mile", "a half percent"); "and a third"/"and a fifth" tails, so "two and
//! a third cups" stays words; improper fractions ("five thirds"); hundredths and
//! smaller; and a mixed number whose fraction is not an exact decimal keeps its
//! unit as a word ("2 1/3 hours"). Each is either rarer than the prose it
//! collides with or needs more context than this word-at-a-time pass has. A
//! number converted wrongly is worse than one left as words, because the reader
//! cannot tell it was Echo that changed it.

use std::ops::RangeInclusive;

use crate::core::format::{key, words};

/// Number words below twenty, where each is its own value.
const UNITS: &[(&str, u64)] = &[
    ("zero", 0),
    ("one", 1),
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("eleven", 11),
    ("twelve", 12),
    ("thirteen", 13),
    ("fourteen", 14),
    ("fifteen", 15),
    ("sixteen", 16),
    ("seventeen", 17),
    ("eighteen", 18),
    ("nineteen", 19),
];

/// The tens, which combine with a unit ("twenty five").
const TENS: &[(&str, u64)] = &[
    ("twenty", 20),
    ("thirty", 30),
    ("forty", 40),
    ("fifty", 50),
    ("sixty", 60),
    ("seventy", 70),
    ("eighty", 80),
    ("ninety", 90),
];

/// Multipliers that scale whatever came before them.
const SCALES: &[(&str, u64)] = &[
    ("hundred", 100),
    ("thousand", 1_000),
    ("million", 1_000_000),
    ("billion", 1_000_000_000),
];

/// Ordinal words below twenty. Paired with their cardinal value, because what
/// gets written is the digits plus a suffix worked out from that value.
const UNIT_ORDINALS: &[(&str, u64)] = &[
    ("first", 1),
    ("second", 2),
    ("third", 3),
    ("fourth", 4),
    ("fifth", 5),
    ("sixth", 6),
    ("seventh", 7),
    ("eighth", 8),
    ("ninth", 9),
    ("tenth", 10),
    ("eleventh", 11),
    ("twelfth", 12),
    ("thirteenth", 13),
    ("fourteenth", 14),
    ("fifteenth", 15),
    ("sixteenth", 16),
    ("seventeenth", 17),
    ("eighteenth", 18),
    ("nineteenth", 19),
];

/// Fraction denominators, singular and plural, from halves to tenths.
///
/// "second" is absent on purpose: "three seconds" is a unit of time, and no one
/// means a half when they say "one second".
const DENOMINATORS: &[(&str, &str, u64)] = &[
    ("half", "halves", 2),
    ("third", "thirds", 3),
    ("quarter", "quarters", 4),
    ("fourth", "fourths", 4),
    ("fifth", "fifths", 5),
    ("sixth", "sixths", 6),
    ("seventh", "sevenths", 7),
    ("eighth", "eighths", 8),
    ("ninth", "ninths", 9),
    ("tenth", "tenths", 10),
];

/// Units that follow a number and are conventionally written as a symbol or
/// abbreviation. `space` says whether the written form takes one.
const UNIT_WORDS: &[(&str, &str, bool)] = &[
    ("percent", "%", false),
    ("degrees", "\u{b0}", false),
    ("dollars", "$", false), // handled as a prefix below
    ("euros", "\u{20ac}", false),
    ("pounds", "\u{a3}", false),
    ("kilometres", "km", true),
    ("kilometers", "km", true),
    ("kilograms", "kg", true),
    ("metres", "m", true),
    ("meters", "m", true),
    ("seconds", "s", true),
    ("minutes", "min", true),
    ("hours", "h", true),
];

/// Currency units are written before the number, not after it.
const CURRENCIES: &[(&str, &str)] = &[
    ("dollars", "$"),
    ("euros", "\u{20ac}"),
    ("pounds", "\u{a3}"),
];

/// Convert spelled-out numbers, times, years and units to their written forms.
pub fn apply(text: &str) -> String {
    let words = words(text);
    let keys: Vec<String> = words.iter().map(|w| key(w)).collect();

    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        // A clock time is checked first: "three thirty" is 3:30, not 330.
        if let Some((len, written)) = match_time(&keys, i) {
            out.push(written);
            i += len;
            continue;
        }
        // Then a year, which is also two numbers that must not be added up.
        if let Some((len, written)) = match_year(&keys, i) {
            out.push(written);
            i += len;
            continue;
        }
        // Before the cardinal, or "twenty fifth" would match "twenty" and
        // leave "fifth" stranded as a word beside a digit.
        if let Some((len, written)) = match_ordinal(&keys, i) {
            out.push(written);
            i += len;
            continue;
        }
        // Before the cardinal too, or "twenty five and a half" would be written
        // "25" and strand "and a half" beside it.
        if let Some((len, written)) = match_fraction(&keys, i) {
            out.push(written);
            i += len;
            continue;
        }
        if let Some((len, value)) = match_cardinal(&keys, i) {
            // A denominator behind a number that did not make a fraction
            // ("twenty two thirds") leaves both as words. Pushing only the first
            // word would let the next pass read "two thirds" as 2/3 on its own.
            if keys
                .get(i + len)
                .is_some_and(|k| ends_in_plural_denominator(k))
            {
                out.extend(words[i..=i + len].iter().map(|w| w.to_string()));
                i += len + 1;
                continue;
            }
            // A unit behind settles the ambiguity even for one word.
            if let Some((unit_len, written)) = match_unit(&keys, i + len, &value.to_string()) {
                out.push(written);
                i += len + unit_len;
                continue;
            }
            // Otherwise only a multi-word number is written as digits.
            if len > 1 {
                out.push(value.to_string());
                i += len;
                continue;
            }
        }
        out.push(words[i].to_string());
        i += 1;
    }

    out.join(" ")
}

/// The written suffix for an ordinal: 1st, 2nd, 3rd, 4th — and 11th, 12th,
/// 13th, which are the exceptions every naive version gets wrong.
fn ordinal_suffix(value: u64) -> &'static str {
    match (value % 100, value % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

/// The value of an ordinal word below twenty, if it is one.
fn ordinal_value(word: &str) -> Option<u64> {
    UNIT_ORDINALS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, v)| *v)
}

/// The value of a cardinal tens word, which is the only thing a unit ordinal
/// may be joined to. "twentieth-fifth" is not English, so an ordinal tens is
/// deliberately not accepted here.
fn tens_value(word: &str) -> Option<u64> {
    TENS.iter().find(|(w, _)| *w == word).map(|(_, v)| *v)
}

/// Match a compound ordinal at `i` — "twenty fifth", "twenty-fifth" — and
/// return how many words it spans and the written form.
///
/// Only compounds. A lone "first", "second" or "fifth" is far more often prose
/// than a number ("first of all", "a fifth of the budget"), which is the same
/// reason a lone cardinal is left alone. "second" would be the worst of them:
/// it is also a unit of time and a verb.
///
/// The tens-only ordinals ("thirtieth") are single words too, so they are left
/// alone for the same reason.
fn match_ordinal(keys: &[String], i: usize) -> Option<(usize, String)> {
    let here = keys.get(i)?.as_str();

    // "twenty-fifth" arrives as one token: `key` strips punctuation only from
    // the ends, so the joining hyphen survives.
    if let Some((tens, unit)) = here.split_once('-') {
        let value = tens_value(tens)? + unit_below_ten(unit)?;
        return Some((1, format!("{value}{}", ordinal_suffix(value))));
    }

    // "twenty fifth" as two words.
    let value = tens_value(here)? + unit_below_ten(keys.get(i + 1)?)?;
    Some((2, format!("{value}{}", ordinal_suffix(value))))
}

/// A unit ordinal that can follow a tens. "twenty tenth" is not a number, so
/// anything from ten up stops the match rather than being added on.
fn unit_below_ten(word: &str) -> Option<u64> {
    ordinal_value(word).filter(|v| *v < 10)
}

/// The value of the number word at `i`, if it is one.
fn word_value(keys: &[String], i: usize) -> Option<u64> {
    let k = keys.get(i)?.as_str();
    UNITS
        .iter()
        .chain(TENS.iter())
        .find(|(w, _)| *w == k)
        .map(|(_, v)| *v)
}

/// Match a spelled-out cardinal starting at `i`, returning how many words it
/// spans and its value.
///
/// Handles the shapes people actually speak: "twenty five", "three hundred",
/// "two thousand four hundred", "a hundred and one". Anything more elaborate
/// stops early and the remaining words are left as they were.
fn match_cardinal(keys: &[String], i: usize) -> Option<(usize, u64)> {
    let mut total = 0u64;
    // The part being built up before a scale word multiplies it.
    let mut current = 0u64;
    let mut len = 0usize;
    let mut saw_number = false;

    while i + len < keys.len() {
        let k = keys[i + len].as_str();

        // "and" only continues a number if one is already in progress and a
        // number follows it ("a hundred and one", never "one and done").
        if k == "and" {
            if !saw_number || word_value(keys, i + len + 1).is_none() {
                break;
            }
            len += 1;
            continue;
        }

        if let Some(value) = word_value(keys, i + len) {
            // Two units in a row are separate numbers, not one ("one two three"
            // is a sequence someone is reading out).
            if !current.is_multiple_of(10) && value < 10 {
                break;
            }
            current += value;
            saw_number = true;
            len += 1;
            continue;
        }

        // "a hundred" — the article is spoken where the digit 1 would be
        // written, but only directly before a scale word.
        if (k == "a" || k == "an")
            && !saw_number
            && keys
                .get(i + len + 1)
                .is_some_and(|next| SCALES.iter().any(|(w, _)| w == next))
        {
            current = 1;
            saw_number = true;
            len += 1;
            continue;
        }

        if let Some((_, scale)) = SCALES.iter().find(|(w, _)| *w == k) {
            if !saw_number {
                break;
            }
            if *scale >= 1_000 {
                total += current.max(1) * scale;
                current = 0;
            } else {
                current = current.max(1) * scale;
            }
            len += 1;
            continue;
        }
        break;
    }

    // An "and" only belongs to the number if something was added after it.
    // "one and two" breaks on the second unit, and must give the "and" back or
    // it would be swallowed into "1" — and "five and two thirds" would lose the
    // "and" that makes it a mixed number.
    if len > 0 && keys[i + len - 1] == "and" {
        len -= 1;
    }

    let value = total + current;
    (saw_number && len > 0).then_some((len, value))
}

/// The value of a plural denominator ("thirds" → 3), if the word is one.
fn plural_denominator(word: &str) -> Option<u64> {
    DENOMINATORS
        .iter()
        .find(|(_, plural, _)| *plural == word)
        .map(|(_, _, d)| *d)
}

/// Whether a word is a plural denominator, alone or as the end of a hyphenated
/// "two-thirds".
fn ends_in_plural_denominator(word: &str) -> bool {
    word.rsplit('-')
        .next()
        .is_some_and(|last| plural_denominator(last).is_some())
}

/// Match a proper plural fraction at `i` — "two thirds", "two-thirds" — and
/// return how many words it spans, the numerator and the denominator.
///
/// Proper and in lowest terms only; see the module doc for why "two halves"
/// and "two quarters" are counting, not arithmetic. With a denominator of ten
/// at most, the numerator is always one word, so only the units are looked up.
fn match_proper(keys: &[String], i: usize) -> Option<(usize, u64, u64)> {
    let here = keys.get(i)?.as_str();
    // "two-thirds" arrives as one token, like "twenty-fifth" does.
    let (span, numerator, denominator) = match here.split_once('-') {
        Some((n, d)) => (1, n, d),
        None => (2, here, keys.get(i + 1)?.as_str()),
    };
    let n = UNITS
        .iter()
        .find(|(w, _)| *w == numerator)
        .map(|(_, v)| *v)?;
    let d = plural_denominator(denominator)?;
    let lowest = !(2..=n).any(|f| n.is_multiple_of(f) && d.is_multiple_of(f));
    (n >= 2 && n < d && lowest).then_some((span, n, d))
}

/// Match a fraction or mixed number at `i`, returning how many words it spans
/// and the written form: "two thirds" → "2/3", "two and a half" → "2 1/2",
/// "two and a half percent" → "2.5%".
fn match_fraction(keys: &[String], i: usize) -> Option<(usize, String)> {
    if let Some((span, n, d)) = match_proper(keys, i) {
        // "quarters" is also coins and a financial calendar; "of" is what says
        // it is a share of something.
        let quarters = keys[i + span - 1].ends_with("quarters");
        if quarters && keys.get(i + span).is_none_or(|k| k != "of") {
            return None;
        }
        return Some((span, format!("{n}/{d}")));
    }

    let (len, whole) = match_cardinal(keys, i)?;
    if keys.get(i + len)? != "and" {
        return None;
    }
    let tail = i + len + 1;
    let (tail_len, n, d) = match (
        keys.get(tail).map(String::as_str),
        keys.get(tail + 1).map(String::as_str),
    ) {
        // "a third" is left out: it is an ordinal adjective as often as a
        // fraction ("step two and a third person").
        (Some("a"), Some("half")) => (2, 1, 2),
        (Some("a"), Some("quarter")) => (2, 1, 4),
        (Some("one"), Some(word)) => {
            let d = DENOMINATORS
                .iter()
                .find(|(singular, _, _)| *singular == word)
                .map(|(_, _, d)| *d)?;
            (2, 1, d)
        }
        _ => match_proper(keys, tail)?,
    };
    let span = len + 1 + tail_len;

    // A unit writes the fraction as a decimal, when there is an exact one.
    // Money takes exactly two places: "$2.50", and never "$2.125".
    let is_currency = keys
        .get(i + span)
        .is_some_and(|k| CURRENCIES.iter().any(|(w, _)| w == k));
    let places = if is_currency { 2..=2 } else { 1..=3 };
    if let Some((unit_len, written)) =
        decimal(whole, n, d, places).and_then(|dec| match_unit(keys, i + span, &dec))
    {
        return Some((span + unit_len, written));
    }
    Some((span, format!("{whole} {n}/{d}")))
}

/// `whole + n/d` as a decimal, at the first number of `places` that is exact.
/// Thirds never are, which is the point of asking.
fn decimal(whole: u64, n: u64, d: u64, places: RangeInclusive<u32>) -> Option<String> {
    places.into_iter().find_map(|p| {
        let scaled = n * 10u64.pow(p);
        scaled
            .is_multiple_of(d)
            .then(|| format!("{whole}.{:0width$}", scaled / d, width = p as usize))
    })
}

/// A clock time: "three thirty", "three thirty pm", "nine oh five".
fn match_time(keys: &[String], i: usize) -> Option<(usize, String)> {
    let hour = word_value(keys, i)?;
    if !(1..=12).contains(&hour) {
        return None;
    }

    // "oh five" is the spoken form of :05.
    let (minute, mut len) = match keys.get(i + 1).map(String::as_str) {
        Some("oh") | Some("o") => {
            let m = word_value(keys, i + 2).filter(|m| *m < 10)?;
            (m, 3)
        }
        _ => {
            let (span, m) = match_cardinal(keys, i + 1)?;
            // Only a two-word minute is unambiguous enough to treat as a time.
            // "three five" is far more likely a sequence than 3:05.
            if span != 1 || !(10..60).contains(&m) {
                return None;
            }
            (m, 2)
        }
    };

    let meridiem = match keys.get(i + len).map(String::as_str) {
        Some("am") | Some("a.m") => {
            len += 1;
            " am"
        }
        Some("pm") | Some("p.m") => {
            len += 1;
            " pm"
        }
        _ => "",
    };

    // Without am/pm this is only a time if the speaker said one — "twenty
    // thirty" is a year-shaped number, not half past eight.
    if meridiem.is_empty() && !(1..=12).contains(&hour) {
        return None;
    }
    Some((len, format!("{hour}:{minute:02}{meridiem}")))
}

/// A spoken year: "twenty twenty six", "nineteen ninety nine".
///
/// Only the two-part form, because that is the one Whisper writes as separate
/// numbers. "Two thousand and six" already falls out of [`match_cardinal`].
fn match_year(keys: &[String], i: usize) -> Option<(usize, String)> {
    let century = word_value(keys, i).filter(|c| (10..=20).contains(c))?;
    let (span, rest) = match_cardinal(keys, i + 1)?;
    if rest >= 100 {
        return None;
    }
    // A single-word remainder under ten ("twenty five") is a plain number, not
    // a year; a year needs the full two-digit remainder.
    if rest < 10 && span == 1 {
        return None;
    }
    Some((1 + span, format!("{}{:02}", century, rest)))
}

/// A unit word following a number at `i`, returning how many words it spans and
/// the written form of number-plus-unit.
///
/// `value` arrives already written, so a whole number and a decimal share it.
fn match_unit(keys: &[String], i: usize, value: &str) -> Option<(usize, String)> {
    let k = keys.get(i)?.as_str();

    if let Some((_, symbol)) = CURRENCIES.iter().find(|(w, _)| *w == k) {
        return Some((1, format!("{symbol}{value}")));
    }
    let (_, symbol, spaced) = UNIT_WORDS.iter().find(|(w, _, _)| *w == k)?;
    Some((
        1,
        if *spaced {
            format!("{value} {symbol}")
        } else {
            format!("{value}{symbol}")
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_word_numbers_become_digits() {
        assert_eq!(apply("we need twenty five of them"), "we need 25 of them");
        assert_eq!(apply("three hundred people"), "300 people");
        assert_eq!(apply("two thousand four hundred"), "2400");
        // The article is part of the number, so it goes with it.
        assert_eq!(apply("a hundred and one uses"), "101 uses");
    }

    /// The rule the whole module hangs on. A lone small number stays a word,
    /// because "one of the best" must survive.
    #[test]
    fn compound_ordinals_become_digits() {
        // The case this exists for: dates.
        assert_eq!(apply("the twenty fifth of June"), "the 25th of June");
        assert_eq!(apply("the twenty-fifth of June"), "the 25th of June");
        // The suffix comes off the value, not off the last word.
        assert_eq!(apply("twenty first"), "21st");
        assert_eq!(apply("thirty second"), "32nd");
        assert_eq!(apply("forty third"), "43rd");
        assert_eq!(apply("ninety ninth"), "99th");
    }

    #[test]
    fn a_lone_ordinal_stays_a_word() {
        // Same rule as the cardinals: prose far more often than a number.
        assert_eq!(apply("first of all"), "first of all");
        assert_eq!(apply("a fifth of the budget"), "a fifth of the budget");
        assert_eq!(apply("wait a second"), "wait a second");
        // Tens on their own are one word too, so they are left alone.
        assert_eq!(apply("the thirtieth"), "the thirtieth");
    }

    #[test]
    fn shapes_that_are_not_ordinals_are_left_alone() {
        // "twentieth-fifth" is not English, and must not be salvaged into one.
        assert_eq!(apply("twentieth-fifth"), "twentieth-fifth");
        // A tens followed by a cardinal is a cardinal, not an ordinal.
        assert_eq!(apply("twenty five"), "25");
        // An ordinal after a non-tens is left as spoken.
        assert_eq!(apply("hundred fifth"), "hundred fifth");
    }

    #[test]
    fn a_lone_small_number_stays_a_word() {
        assert_eq!(apply("one of the best"), "one of the best");
        assert_eq!(apply("give me two"), "give me two");
        assert_eq!(apply("nine lives"), "nine lives");
    }

    /// A unit behind is enough evidence on its own, even for one word.
    #[test]
    fn a_unit_settles_a_lone_number() {
        assert_eq!(apply("up five percent"), "up 5%");
        assert_eq!(apply("it costs twenty dollars"), "it costs $20");
        assert_eq!(apply("about ten kilometres"), "about 10 km");
        assert_eq!(apply("thirty degrees today"), "30\u{b0} today");
    }

    #[test]
    fn clock_times_use_a_colon() {
        assert_eq!(apply("meet at three thirty pm"), "meet at 3:30 pm");
        assert_eq!(apply("the nine fifteen train"), "the 9:15 train");
        assert_eq!(apply("at nine oh five am"), "at 9:05 am");
    }

    #[test]
    fn spoken_years_are_not_added_up() {
        assert_eq!(apply("in twenty twenty six"), "in 2026");
        assert_eq!(apply("since nineteen ninety nine"), "since 1999");
    }

    /// The ambiguity that makes years hard: "twenty five" is a quantity, and
    /// only the two-digit remainder makes it a year.
    #[test]
    fn a_short_remainder_is_a_quantity_not_a_year() {
        assert_eq!(apply("twenty five"), "25");
        assert_eq!(apply("twenty twenty"), "2020");
    }

    #[test]
    fn proper_fractions_become_digits() {
        assert_eq!(apply("two thirds of voters"), "2/3 of voters");
        assert_eq!(apply("about two-thirds of it"), "about 2/3 of it");
        assert_eq!(apply("three eighths"), "3/8");
        assert_eq!(apply("nine tenths"), "9/10");
        assert_eq!(apply("three fourths"), "3/4");
        // "of" is what makes quarters a share rather than coins or a calendar.
        assert_eq!(apply("three quarters of a mile"), "3/4 of a mile");
    }

    /// A fraction is only a number when nothing else reads it better.
    #[test]
    fn fraction_words_in_prose_stay_words() {
        for said in [
            "a third of the budget",
            "a half",
            "half the time",
            "only one third agreed",
            "the second half",
            "a quarter past",
            // Counting pieces, not arithmetic: improper, or not lowest terms.
            "cut it into two halves",
            "all four quarters",
            "five thirds",
            "two fourths",
            // Coins and a financial calendar.
            "three quarters for the meter",
            "the last three quarters",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    /// "twenty fifths" must not be read as an ordinal, nor half converted.
    #[test]
    fn a_number_running_into_a_denominator_stays_words() {
        assert_eq!(apply("twenty fifths"), "twenty fifths");
        assert_eq!(apply("twenty two thirds"), "twenty two thirds");
        assert_eq!(apply("twenty two-thirds"), "twenty two-thirds");
        assert_eq!(apply("twenty-fifths"), "twenty-fifths");
        // The plural never reaches the ordinal path; the singular still does.
        assert_eq!(apply("the twenty fifth"), "the 25th");
    }

    /// "seconds" is a unit of time, never a denominator.
    #[test]
    fn seconds_are_time_not_a_fraction() {
        assert_eq!(apply("three seconds"), "3 s");
        assert_eq!(apply("wait a second"), "wait a second");
        assert_eq!(apply("two and a second"), "two and a second");
        assert_eq!(apply("two and a half seconds"), "2.5 s");
    }

    #[test]
    fn mixed_numbers_are_written_with_a_space() {
        assert_eq!(apply("two and a half"), "2 1/2");
        assert_eq!(apply("one and a quarter cups"), "1 1/4 cups");
        assert_eq!(apply("five and two thirds"), "5 2/3");
        assert_eq!(apply("three and one third"), "3 1/3");
        assert_eq!(apply("twenty five and a half"), "25 1/2");
        // "a third" is an ordinal adjective as often as a fraction.
        assert_eq!(
            apply("step two and a third person"),
            "step two and a third person"
        );
    }

    /// A unit wants a decimal, and gets one when the fraction has an exact one.
    #[test]
    fn a_unit_turns_a_mixed_number_into_a_decimal() {
        assert_eq!(apply("up two and a half percent"), "up 2.5%");
        assert_eq!(apply("three and a quarter kilometres"), "3.25 km");
        assert_eq!(apply("two and a half dollars"), "$2.50");
        // No exact decimal, so the unit stays a word rather than "2.333 h".
        assert_eq!(apply("two and one third hours"), "2 1/3 hours");
    }

    /// The "and" in "a hundred and one" must not swallow a separate number.
    #[test]
    fn a_trailing_and_is_not_part_of_the_number() {
        assert_eq!(apply("one and two"), "one and two");
        assert_eq!(apply("twenty and done"), "twenty and done");
        assert_eq!(apply("a hundred and one uses"), "101 uses");
    }

    #[test]
    fn fractions_leave_times_years_and_ordinals_alone() {
        assert_eq!(apply("meet at three thirty pm"), "meet at 3:30 pm");
        assert_eq!(apply("in twenty twenty six"), "in 2026");
        assert_eq!(apply("the twenty first of June"), "the 21st of June");
    }

    /// Words that merely sound like numbers must never be touched.
    #[test]
    fn ordinary_words_are_left_alone() {
        for said in ["I have won the race", "no one knows", "for once"] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn text_with_no_numbers_is_untouched() {
        let said = "the quick brown fox";
        assert_eq!(apply(said), said);
    }
}
