//! Spelled-out numbers in French.
//!
//! French builds its numbers out of words that are also ordinary words. "un"
//! and "une" are the indefinite article, "neuf" is "new", "sept", "cent" and
//! "mille" turn up in prose, and "et" is "and". So the English rule applies
//! with more force: **a lone number word stays a word**, and a number is only
//! written as digits when the speaker gave more than one piece of it
//! ("vingt-cinq", "trois cents") or a unit settles it ("cinq pour cent").
//!
//! **Hyphens do not matter.** "quatre-vingt-dix-sept", "quatre vingt dix sept"
//! and the 1990 "vingt-et-un" are all the same number, and Whisper writes
//! them every way. A hyphenated word is split into its pieces and parsed
//! exactly like the spaced form. It still counts as one word, though: a number
//! may only end where a word ends, so "vingt-quatre" is never half-converted.
//!
//! **The grammar is checked, not summed.** An English-style accumulator would
//! happily read "sept neuf" as 16 or "mille et une" as 1001. Instead the pieces
//! must form a number French actually says: tens take "et" only before "un" and
//! (for soixante) "onze", quatre-vingt never takes it, only soixante and
//! quatre-vingt count on past nine. Anything else is not a number, and the
//! longest prefix that *is* one wins, which is how "vingt-deux trois" becomes
//! "22 trois" rather than 25.
//!
//! **"un" and "une" are the article until proven otherwise.** As separate
//! words they only count after "et" ("vingt et un", "soixante et une"). "cent
//! un" is 101 on paper, but "il en reste cent un peu partout" is far more
//! likely, and the reader cannot tell it was Echo that wrote "101 peu". Inside
//! a hyphenated word ("quatre-vingt-un") there is no article to confuse it
//! with. The belgian and swiss "septante", "huitante"/"octante" and "nonante"
//! follow the same rules as trente.
//!
//! **Typography is French.** Thousands are grouped from five digits with a
//! narrow no-break space (U+202F, what CLDR and the Imprimerie nationale use),
//! so "25 000" but "2026". "%" takes the same narrow space; "€", "km", "kg" and
//! the "h" of a time take a regular no-break space (U+00A0), again per CLDR.
//! Neither is a plain space on purpose: the tidy stage removes a plain space in
//! front of "%", and a line break between a number and its unit is the one
//! thing every style guide forbids.
//!
//! **Ordinals convert only as compounds**, as in English: "vingt-cinquième" is
//! "25e", "trente et unième" "31e", "deux cent vingt-cinquième" "225e" — the
//! short form the Imprimerie nationale gives, and never "25ème". A lone
//! "premier", "deuxième", "second" or "vingtième" stays a word: "le premier
//! venu" and "en second lieu" are prose, the same reason a lone cardinal stays.
//! Dates need nothing more, because French dates use cardinals ("le vingt-cinq
//! juin" → "le 25 juin") except the first, and "le premier juin" is written in
//! letters as often as "1er". The ordinal's last piece must be below a hundred:
//! "deux centièmes" (2/100) sounds exactly like "deux centième", so hundredths
//! and thousandths stay words. Plurals ("trois vingt-cinquièmes") are
//! fractions, and so is a singular after "un"/"une" ("un vingt-cinquième de la
//! population"), so both stay too.
//!
//! ponytail: left as words, each for a reason that costs little: "million"
//! and "milliard" (French writes "2 millions", which is what leaving them
//! gives once the multiplier is converted), lone ordinals and the fractions
//! above, "1er" for "premier", decimals said with "virgule" (the punctuation stage owns that
//! word), negatives ("moins cinq degrés"), and clock times other than "N
//! heure(s) M", "et demie" and "et quart" ("midi", "moins le quart").

use crate::core::format::{key, words};

/// Units that settle a lone number, as spoken → what follows the digits. The
/// separator is part of the written form; see the module docs for why it is
/// never a plain space. Singulars are listed where "zéro" can precede them,
/// since "un" and "une" never reach this table.
const UNIT_WORDS: &[(&str, &str)] = &[
    ("pour cent", "\u{202f}%"),
    ("euros", "\u{a0}\u{20ac}"),
    ("euro", "\u{a0}\u{20ac}"),
    ("degrés", "\u{b0}"),
    ("degres", "\u{b0}"),
    ("degré", "\u{b0}"),
    ("degre", "\u{b0}"),
    ("kilomètres", "\u{a0}km"),
    ("kilometres", "\u{a0}km"),
    ("kilogrammes", "\u{a0}kg"),
    ("kilos", "\u{a0}kg"),
];

/// Convert spelled-out numbers, times and units to their written forms.
pub fn apply(text: &str) -> String {
    // Line by line: a number never spans a line break, and splitting the whole
    // text on whitespace would flatten the "nouvelle ligne" the punctuation
    // stage has just inserted.
    text.split('\n')
        .map(apply_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_line(line: &str) -> String {
    let words = words(line);
    let keys: Vec<String> = words.iter().map(|w| key(w)).collect();

    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        // A time first: "trois heures vingt" is 3 h 20, not 3 and a stray 20.
        // An ordinal before a cardinal, which would otherwise convert the
        // "deux cent" of "deux cent vingt-cinquième" and strand the rest.
        let found = match_time(&words, &keys, i)
            .or_else(|| match_ordinal(&words, &keys, i))
            .or_else(|| {
                let (len, value, pieces) = cardinal_at(&words, &keys, i)?;
                // A unit behind settles the ambiguity even for one word.
                match_unit(&words, &keys, i, len, value)
                    // Otherwise only a number spoken in several pieces is digits.
                    .or_else(|| (pieces > 1).then(|| (len, grouped(value))))
            });
        match found {
            Some((len, written)) => {
                // The decoder's own punctuation around the span survives it.
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

/// The value of a number word that stands on its own, if it is one. Tens and
/// units only: "cent" and "mille" multiply, so they are handled by the grammar.
fn value(word: &str) -> Option<u64> {
    Some(match word {
        "un" | "une" => 1,
        "deux" => 2,
        "trois" => 3,
        "quatre" => 4,
        "cinq" => 5,
        "six" => 6,
        "sept" => 7,
        "huit" => 8,
        "neuf" => 9,
        "dix" => 10,
        "onze" => 11,
        "douze" => 12,
        "treize" => 13,
        "quatorze" => 14,
        "quinze" => 15,
        "seize" => 16,
        // The plural only changes the spelling ("quatre-vingts"), so both count.
        "vingt" | "vingts" => 20,
        "trente" => 30,
        "quarante" => 40,
        "cinquante" => 50,
        "soixante" => 60,
        "septante" => 70,
        "huitante" | "octante" => 80,
        "nonante" => 90,
        _ => return None,
    })
}

fn is_tens(word: &str) -> bool {
    value(word).is_some_and(|v| v >= 20)
}

/// Whether `piece` can be part of a spoken number at all. Deciding whether the
/// pieces make a number is [`parse`]'s job.
fn is_piece(piece: &str) -> bool {
    value(piece).is_some() || matches!(piece, "et" | "cent" | "cents" | "mille" | "zéro" | "zero")
}

/// The longest spoken number starting at word `i`, as (words spanned, value,
/// pieces). Pieces are what tells "vingt-cinq" (two) from "vingt" (one), so the
/// lone-word rule can be applied to what was said rather than to whitespace.
fn cardinal_at(words: &[&str], keys: &[String], i: usize) -> Option<(usize, u64, usize)> {
    let mut pieces: Vec<&str> = Vec::new();
    // How many pieces there are after each whole word, because a number may
    // only end where a word does.
    let mut ends: Vec<usize> = Vec::new();

    for n in i..keys.len() {
        // Punctuation between two words means two things were said.
        if n > i && !lead(words[n]).is_empty() {
            break;
        }
        // A free-standing "un"/"une" is the article unless "et" came first.
        if matches!(keys[n].as_str(), "un" | "une") && pieces.last() != Some(&"et") {
            break;
        }
        let parts: Vec<&str> = keys[n].split('-').collect();
        if !parts.iter().all(|p| is_piece(p)) {
            break;
        }
        pieces.extend(parts);
        ends.push(pieces.len());
        if !trail(words[n]).is_empty() {
            break;
        }
    }

    ends.iter()
        .enumerate()
        .rev()
        .find_map(|(n, &end)| parse(&pieces[..end]).map(|v| (n + 1, v, end)))
}

/// The value of `pieces` if, all together, they are one number French says.
fn parse(pieces: &[&str]) -> Option<u64> {
    // Zero only ever stands alone; "vingt zéro" is not a number.
    if let ["zéro" | "zero"] = pieces {
        return Some(0);
    }
    match number(pieces)? {
        ([], value) => Some(value),
        _ => None,
    }
}

// The parsers below each take the longest number they can from the front and
// return what is left, so [`parse`] can insist nothing is.

type Parsed<'a> = Option<(&'a [&'a str], u64)>;

/// Up to 999 999: "mille", "deux mille", "deux mille vingt-six".
fn number<'a>(a: &'a [&'a str]) -> Parsed<'a> {
    let (rest, thousands) = match below_2000(a) {
        Some((["mille", rest @ ..], n)) if (2..=999).contains(&n) => (rest, n * 1000),
        Some(done) => return Some(done),
        None => match a {
            ["mille", rest @ ..] => (rest, 1000),
            _ => return None,
        },
    };
    Some(match below_2000(rest) {
        Some((r, n)) => (r, thousands + n),
        None => (rest, thousands),
    })
}

/// Hundreds: "cent", "trois cents", "cent vingt". The multiplier goes up to
/// nineteen for the years said as hundreds ("dix-neuf cent quatre-vingt-quatre")
/// — left out, "dix-neuf" would convert and strand "cent" beside a digit.
fn below_2000<'a>(a: &'a [&'a str]) -> Parsed<'a> {
    let (rest, hundreds) = match a {
        ["cent" | "cents", rest @ ..] => (rest, 100),
        _ => match below_20(a) {
            // "un cent" and "dix cents" are not French.
            Some((["cent" | "cents", rest @ ..], m)) if m >= 2 && m != 10 => (rest, m * 100),
            _ => return below_100(a),
        },
    };
    Some(match below_100(rest) {
        Some((r, n)) => (r, hundreds + n),
        None => (rest, hundreds),
    })
}

/// 1 to 99, where all of French's irregularity lives.
fn below_100<'a>(a: &'a [&'a str]) -> Parsed<'a> {
    match a {
        // 80 takes no "et" and counts on to 99: quatre-vingt-un, -onze, -dix-neuf.
        ["quatre", "vingt" | "vingts", rest @ ..] => Some(match below_20(rest) {
            Some((r, n)) => (r, 80 + n),
            None => (rest, 80),
        }),
        ["soixante", "et", "onze", rest @ ..] => Some((rest, 71)),
        // "et" joins only here. "vingt et deux" is "twenty and two".
        [t, "et", "un" | "une", rest @ ..] if is_tens(t) => Some((rest, value(t)? + 1)),
        [t, rest @ ..] if is_tens(t) => {
            let tens = value(t)?;
            // Only soixante counts on past nine (soixante-douze); a one or an
            // eleven needs the "et" handled above.
            let top = if tens == 60 { 19 } else { 9 };
            Some(match below_20(rest) {
                Some((r, n)) if (2..=top).contains(&n) && n != 11 => (r, tens + n),
                _ => (rest, tens),
            })
        }
        _ => below_20(a),
    }
}

/// 1 to 19: a single word, or "dix" before sept, huit or neuf.
fn below_20<'a>(a: &'a [&'a str]) -> Parsed<'a> {
    match a {
        ["dix", u @ ("sept" | "huit" | "neuf"), rest @ ..] => Some((rest, 10 + value(u)?)),
        [w, rest @ ..] => value(w).filter(|v| *v <= 16).map(|v| (rest, v)),
        [] => None,
    }
}

/// A clock time: "trois heures vingt", "quinze heures trente", "une heure et
/// demie". Written the French way, "15 h 30".
///
/// "N heures" alone is left alone, because "il a dormi trois heures" is a
/// duration. With minutes it is a time, and even when it is a duration
/// ("trois heures vingt de route") "3 h 20" is how French writes that too.
fn match_time(words: &[&str], keys: &[String], i: usize) -> Option<(usize, String)> {
    // "une heure" is the one place the article is plainly the number.
    let (hour_len, hour) = if keys[i] == "une" {
        (1, 1)
    } else {
        let (len, hour, _) = cardinal_at(words, keys, i)?;
        (len, hour)
    };
    let h = i + hour_len;
    if hour > 24 || !matches!(keys.get(h)?.as_str(), "heure" | "heures") {
        return None;
    }

    let word = |n: usize| keys.get(n).map(String::as_str);
    let (minute_len, minute) = match (word(h + 1), word(h + 2)) {
        (Some("et"), Some("demie")) => (2, 30),
        (Some("et"), Some("quart")) => (2, 15),
        _ => {
            let (len, minute, _) = cardinal_at(words, keys, h + 1)?;
            // "deux heures vingt minutes" spelled the duration out; leave it.
            if !(1..=59).contains(&minute)
                || matches!(word(h + 1 + len), Some("minute" | "minutes"))
            {
                return None;
            }
            (len, minute)
        }
    };

    let len = hour_len + 1 + minute_len;
    joined(&words[i..i + len]).then(|| (len, format!("{hour}\u{a0}h\u{a0}{minute:02}")))
}

/// A compound ordinal starting at word `i`: "vingt-cinquième" → "25e",
/// "trente et unième" → "31e". See the module docs for what stays a word.
fn match_ordinal(words: &[&str], keys: &[String], i: usize) -> Option<(usize, String)> {
    // "un vingt-cinquième" is a fraction as often as "a 25th".
    if i > 0 && matches!(keys[i - 1].as_str(), "un" | "une") {
        return None;
    }
    let mut pieces: Vec<&str> = Vec::new();
    for n in i..keys.len() {
        if n > i && !lead(words[n]).is_empty() {
            return None;
        }
        let parts: Vec<&str> = keys[n].split('-').collect();
        let (last, init) = parts.split_last()?;
        if init.iter().all(|p| is_piece(p)) {
            if let Some(cardinal) = ordinal_piece(last) {
                pieces.extend(init);
                pieces.push(cardinal);
                // More than one piece, or it is the lone ordinal that stays.
                return (pieces.len() > 1)
                    .then(|| parse(&pieces))
                    .flatten()
                    .map(|value| (n + 1 - i, format!("{value}e")));
            }
        }
        if !parts.iter().all(|p| is_piece(p)) || !trail(words[n]).is_empty() {
            return None;
        }
        pieces.extend(parts);
    }
    None
}

/// The cardinal piece an ordinal piece is built on — "cinquième" → "cinq",
/// "trentième" → "trente" — for anything below a hundred. Singular only: the
/// plural is a fraction. Whisper drops the accent often enough to accept
/// "ieme".
fn ordinal_piece(piece: &str) -> Option<&str> {
    const DROPS_E: &[&str] = &[
        "quatre",
        "onze",
        "douze",
        "treize",
        "quatorze",
        "quinze",
        "seize",
        "trente",
        "quarante",
        "cinquante",
        "soixante",
        "septante",
        "huitante",
        "octante",
        "nonante",
    ];
    let stem = piece
        .strip_suffix("ième")
        .or_else(|| piece.strip_suffix("ieme"))?;
    match stem {
        "cinqu" => Some("cinq"),
        "neuv" => Some("neuf"),
        _ if value(stem).is_some() => Some(stem),
        _ => DROPS_E
            .iter()
            .find(|c| c.strip_suffix('e') == Some(stem))
            .copied(),
    }
}

/// A unit following the `len`-word number at `i`, returning the words spanned
/// by both and the written form.
fn match_unit(
    words: &[&str],
    keys: &[String],
    i: usize,
    len: usize,
    value: u64,
) -> Option<(usize, String)> {
    let at = i + len;
    UNIT_WORDS.iter().find_map(|(phrase, suffix)| {
        let end = at + phrase.split(' ').count();
        let spoken = keys.get(at..end)?.join(" ");
        // A number after the unit means a larger amount was spoken: "dix
        // euros cinquante" is 10,50 €, and "10 € cinquante" would be wrong.
        (spoken == *phrase && joined(&words[i..end]) && cardinal_at(words, keys, end).is_none())
            .then(|| (end - i, format!("{}{suffix}", grouped(value))))
    })
}

/// Digits grouped the French way: a narrow no-break space every three, from
/// five digits up. Four digits stay solid, as French typography allows and as
/// every year is written.
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    if digits.len() < 5 {
        return digits;
    }
    let mut out = String::new();
    for (n, c) in digits.chars().enumerate() {
        if n > 0 && (digits.len() - n).is_multiple_of(3) {
            out.push('\u{202f}');
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

/// Punctuation the decoder put after a word: "vingt-cinq." must stay "25.".
fn trail(word: &str) -> &str {
    &word[word
        .trim_end_matches(|c: char| c.is_ascii_punctuation())
        .len()..]
}

/// Whether `span` reads as one unbroken run: punctuation may sit before its
/// first word and after its last, never in between. "vingt, cinq" is two
/// numbers someone listed, not 25.
fn joined(span: &[&str]) -> bool {
    span.iter().enumerate().all(|(n, w)| {
        (n == 0 || lead(w).is_empty()) && (n + 1 == span.len() || trail(w).is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_word_numbers_become_digits() {
        assert_eq!(apply("nous en avons vingt cinq"), "nous en avons 25");
        assert_eq!(apply("trois cents personnes"), "300 personnes");
        assert_eq!(apply("trois cent vingt"), "320");
        assert_eq!(apply("en deux mille vingt-six"), "en 2026");
        assert_eq!(apply("mille neuf cent quatre-vingt-dix-neuf"), "1999");
        // The years said as hundreds.
        assert_eq!(apply("en dix-neuf cent quatre-vingt-quatre"), "en 1984");
    }

    /// Whisper hyphenates as it pleases, so both spellings are one number.
    #[test]
    fn hyphens_are_optional() {
        assert_eq!(apply("vingt-cinq"), "25");
        assert_eq!(apply("quatre-vingt-dix-sept"), "97");
        assert_eq!(apply("quatre vingt dix sept"), "97");
        assert_eq!(apply("quatre-vingt dix-sept"), "97");
        // The plural "s" is spelling, not value.
        assert_eq!(apply("quatre-vingts"), "80");
        assert_eq!(apply("deux cent"), "200");
    }

    #[test]
    fn et_joins_only_where_french_puts_it() {
        assert_eq!(apply("soixante et onze"), "71");
        assert_eq!(apply("soixante-et-onze"), "71");
        assert_eq!(apply("vingt et une pages"), "21 pages");
        assert_eq!(apply("trente-et-un"), "31");
        // Everywhere else it is "and".
        assert_eq!(
            apply("deux et deux font quatre"),
            "deux et deux font quatre"
        );
        assert_eq!(apply("les mille et une nuits"), "les mille et une nuits");
    }

    #[test]
    fn belgian_and_swiss_tens() {
        assert_eq!(apply("septante-cinq"), "75");
        assert_eq!(apply("nonante et un"), "91");
        assert_eq!(apply("huitante-deux"), "82");
        assert_eq!(apply("octante huit"), "88");
    }

    #[test]
    fn large_numbers_are_grouped_the_french_way() {
        assert_eq!(apply("vingt-cinq mille"), "25\u{202f}000");
        assert_eq!(apply("deux cent mille"), "200\u{202f}000");
        // Millions stay a word; the multiplier converts only if it would anyway.
        assert_eq!(apply("deux millions"), "deux millions");
        assert_eq!(apply("vingt-cinq millions"), "25 millions");
    }

    /// The rule the whole module hangs on.
    #[test]
    fn a_lone_number_word_stays_a_word() {
        for said in [
            "un des meilleurs",
            "deux",
            "j'en veux trois",
            "mille mercis",
            "cent fois",
            "sept jours sur sept",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    /// "un" and "une" are the article far more often than they are 1.
    #[test]
    fn articles_are_not_one() {
        for said in [
            "un livre",
            "une fois",
            "un euro",
            "une heure",
            // "cent un" is 101 on paper and "a hundred, a bit" in speech.
            "il en reste cent un peu partout",
        ] {
            assert_eq!(apply(said), said);
        }
        // Inside a hyphenated number there is nothing to confuse it with.
        assert_eq!(apply("quatre-vingt-un"), "81");
    }

    /// "neuf" is also "new", and must not be swallowed after a number.
    #[test]
    fn neuf_the_adjective_survives() {
        assert_eq!(apply("un livre neuf"), "un livre neuf");
        assert_eq!(apply("deux livres neufs"), "deux livres neufs");
        // Two units in a row are a sequence, not a sum.
        assert_eq!(apply("sept neuf"), "sept neuf");
        assert_eq!(apply("vingt-neuf"), "29");
    }

    #[test]
    fn shapes_french_does_not_say_are_left_alone() {
        // No "et" in 81, and none missing from 71.
        assert_eq!(apply("soixante-onze"), "soixante-onze");
        assert_eq!(apply("vingt-un"), "vingt-un");
        // Only the valid prefix converts, never a sum of the rest.
        assert_eq!(apply("vingt-deux trois"), "22 trois");
    }

    /// A unit behind is enough evidence on its own, even for one word.
    #[test]
    fn a_unit_settles_a_lone_number() {
        assert_eq!(
            apply("en hausse de cinq pour cent"),
            "en hausse de 5\u{202f}%"
        );
        assert_eq!(apply("vingt euros"), "20\u{a0}\u{20ac}");
        assert_eq!(apply("trente degrés"), "30\u{b0}");
        assert_eq!(apply("zéro degré"), "0\u{b0}");
        assert_eq!(apply("dix kilomètres"), "10\u{a0}km");
        assert_eq!(apply("deux kilos"), "2\u{a0}kg");
        assert_eq!(apply("deux cents euros"), "200\u{a0}\u{20ac}");
        // Cents after the unit: the amount is left whole rather than split.
        assert_eq!(apply("dix euros cinquante"), "dix euros cinquante");
    }

    #[test]
    fn clock_times_are_written_with_h() {
        assert_eq!(apply("trois heures vingt"), "3\u{a0}h\u{a0}20");
        assert_eq!(apply("à quinze heures trente"), "à 15\u{a0}h\u{a0}30");
        assert_eq!(apply("une heure et demie"), "1\u{a0}h\u{a0}30");
        assert_eq!(apply("neuf heures cinq"), "9\u{a0}h\u{a0}05");
        // A bare "N heures" is a duration as often as a time.
        assert_eq!(apply("trois heures"), "trois heures");
        assert_eq!(
            apply("deux heures vingt minutes"),
            "deux heures vingt minutes"
        );
    }

    #[test]
    fn compound_ordinals_become_digits() {
        assert_eq!(apply("vingt-cinquième"), "25e");
        assert_eq!(apply("trente et unième"), "31e");
        assert_eq!(apply("trente-et-unième"), "31e");
        assert_eq!(apply("soixante-dix-septième"), "77e");
        assert_eq!(apply("quatre-vingt-dixième"), "90e");
        assert_eq!(apply("quatre-vingt-neuvième"), "89e");
        assert_eq!(apply("cent unième"), "101e");
        assert_eq!(apply("deux cent vingt-cinquième"), "225e");
        assert_eq!(apply("vingt-quatrième"), "24e");
        assert_eq!(apply("vingt cinquieme"), "25e");
        assert_eq!(apply("la vingt-cinquième fois."), "la 25e fois.");
    }

    #[test]
    fn lone_ordinals_and_fractions_stay_words() {
        for said in [
            "le premier venu",
            "en second lieu",
            "le deuxième jour",
            "le vingtième siècle",
            "le centième",
            "unième",
            "deux centièmes de seconde",
            "deux centième",
            "trois vingt-cinquièmes",
            "un vingt-cinquième de la population",
            "vingt, cinquième",
            "vingt et deuxième",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn punctuation_around_a_number_is_kept_and_splits_it() {
        assert_eq!(apply("j'en ai vingt-cinq."), "j'en ai 25.");
        assert_eq!(apply("(vingt cinq)"), "(25)");
        assert_eq!(apply("vingt, cinq"), "vingt, cinq");
    }

    #[test]
    fn line_breaks_survive() {
        assert_eq!(apply("vingt-cinq\n\ntrente et un"), "25\n\n31");
    }

    #[test]
    fn ordinary_sentences_are_left_alone() {
        for said in [
            "le point est clair",
            "et alors",
            "il est neuf heures",
            "nous étions cent",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    /// Through the whole pipeline: the no-break spaces are what stop tidy
    /// taking the space away from "%", and a spoken mark lands after the unit.
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
                "la hausse est de vingt-cinq pour cent virgule soit dix euros",
                all,
                Some("fr")
            ),
            "La hausse est de 25\u{202f}%, soit 10\u{a0}\u{20ac}"
        );
        assert_eq!(
            crate::core::format::apply("rendez-vous à quinze heures trente point", all, Some("fr")),
            "Rendez-vous à 15\u{a0}h\u{a0}30."
        );
        let fr = |said| crate::core::format::apply(said, all, Some("fr"));
        assert_eq!(
            fr("euh c'est le vingt-cinquième anniversaire point"),
            "C'est le 25e anniversaire."
        );
        assert_eq!(
            fr("au vingt et unième siècle virgule le vingt-cinq juin"),
            "Au 21e siècle, le 25 juin"
        );
        // Dates are cardinals, and the first of the month stays a word.
        assert_eq!(fr("le premier juin"), "Le premier juin");
        assert_eq!(fr("le deuxième jour"), "Le deuxième jour");
    }
}
