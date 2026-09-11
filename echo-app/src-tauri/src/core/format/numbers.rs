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
//! ponytail: English only, and fractions ("two thirds") are still left alone —
//! they collide with the ordinals above, and "two thirds" is as often prose as
//! arithmetic. Other languages are a parser each, not another table. A number
//! converted wrongly is worse than one left as words, because the reader cannot
//! tell it was Echo that changed it.

use super::{key, words};

/// Number words below twenty, where each is its own value.
const UNITS: &[(&str, u64)] = &[
    ("zero", 0), ("one", 1), ("two", 2), ("three", 3), ("four", 4),
    ("five", 5), ("six", 6), ("seven", 7), ("eight", 8), ("nine", 9),
    ("ten", 10), ("eleven", 11), ("twelve", 12), ("thirteen", 13),
    ("fourteen", 14), ("fifteen", 15), ("sixteen", 16), ("seventeen", 17),
    ("eighteen", 18), ("nineteen", 19),
];

/// The tens, which combine with a unit ("twenty five").
const TENS: &[(&str, u64)] = &[
    ("twenty", 20), ("thirty", 30), ("forty", 40), ("fifty", 50),
    ("sixty", 60), ("seventy", 70), ("eighty", 80), ("ninety", 90),
];

/// Multipliers that scale whatever came before them.
const SCALES: &[(&str, u64)] = &[
    ("hundred", 100), ("thousand", 1_000), ("million", 1_000_000),
    ("billion", 1_000_000_000),
];

/// Ordinal words below twenty. Paired with their cardinal value, because what
/// gets written is the digits plus a suffix worked out from that value.
const UNIT_ORDINALS: &[(&str, u64)] = &[
    ("first", 1), ("second", 2), ("third", 3), ("fourth", 4), ("fifth", 5),
    ("sixth", 6), ("seventh", 7), ("eighth", 8), ("ninth", 9), ("tenth", 10),
    ("eleventh", 11), ("twelfth", 12), ("thirteenth", 13), ("fourteenth", 14),
    ("fifteenth", 15), ("sixteenth", 16), ("seventeenth", 17),
    ("eighteenth", 18), ("nineteenth", 19),
];

/// Units that follow a number and are conventionally written as a symbol or
/// abbreviation. `space` says whether the written form takes one.
const UNIT_WORDS: &[(&str, &str, bool)] = &[
    ("percent", "%", false),
    ("degrees", "\u{b0}", false),
    ("dollars", "$", false),   // handled as a prefix below
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

/// Whether number conversion exists for `language`.
///
/// English only. Number words are grammar rather than a lookup — "quatre-vingt
/// dix-sept", "einundzwanzig" — so another language is a parser of its own, not
/// another table. Reported honestly so the settings screen can say which
/// languages this stage applies to.
pub fn covers(language: Option<&str>) -> bool {
    let raw = language.unwrap_or("en").to_lowercase();
    raw.split(['-', '_']).next() == Some("en")
}

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
        if let Some((len, value)) = match_cardinal(&keys, i) {
            // A unit behind settles the ambiguity even for one word.
            if let Some((unit_len, written)) = match_unit(&keys, i + len, value) {
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
    UNIT_ORDINALS.iter().find(|(w, _)| *w == word).map(|(_, v)| *v)
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

    let value = total + current;
    (saw_number && len > 0).then_some((len, value))
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
fn match_unit(keys: &[String], i: usize, value: u64) -> Option<(usize, String)> {
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

    /// Words that merely sound like numbers must never be touched.
    #[test]
    fn ordinary_words_are_left_alone() {
        for said in ["I have won the race", "no one knows", "for once"] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn only_english_is_claimed() {
        assert!(covers(None));
        assert!(covers(Some("en")));
        assert!(covers(Some("en-GB")));
        for other in ["fr", "de", "es", "ja"] {
            assert!(!covers(Some(other)), "{other} has no number rules");
        }
    }

    #[test]
    fn text_with_no_numbers_is_untouched() {
        let said = "the quick brown fox";
        assert_eq!(apply(said), said);
    }
}
