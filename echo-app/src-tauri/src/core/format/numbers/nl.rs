//! Spelled-out numbers in Dutch.
//!
//! Dutch builds numbers the way German does — one word, units before tens,
//! joined by "en": "eenentwintig", "vijfendertig", "tweeduizendzesentwintig" —
//! so the grammar is German's, shared from [`super::de`], and this module
//! supplies the Dutch morphemes, its article and its written forms. The rules
//! are the same too: a compound of two or more number morphemes becomes digits,
//! a lone "drie", "tien" or "honderd" stays a word, and a unit behind settles a
//! lone number.
//!
//! **"een" is the article far more than the number.** Dutch spells the numeral
//! "één" precisely to tell them apart, and Whisper often does not bother. So a
//! standalone "een" is never 1, not even before a unit ("een procent" stays),
//! while an explicit "één" before a unit is ("één procent" → "1%"). Alone, "één"
//! stays a word like any other simple number. Fused, "een" is unambiguous:
//! "eenentwintig", "eenhonderd".
//!
//! **Spelling variants.** The diaeresis that splits vowels ("tweeëntwintig",
//! "drieëntwintig") is folded away before matching, so the forms Whisper writes
//! without it parse the same. Split compounds are rejoined across "honderd" or
//! "duizend" only ("twee honderd"), never across a standalone "en": "twee en
//! twintig" may be a list.
//!
//! **Years** are the plain value of the fused word:
//! "negentienhonderdnegenennegentig" is 1999, "tweeduizendzesentwintig" is 2026.
//!
//! **Ordinals** are written "25e", the short form the Taalunie gives. Only the
//! compound "-ste" ordinals convert ("vijfentwintigste", "eenentwintigste
//! eeuw"); "eerste", "tiende", "twintigste" stay words, as lone ordinals do in
//! English. The written form ends in a letter, so the tidy stage has nothing to
//! misread, unlike the German "25.".
//!
//! **Units follow Dutch convention**, which is not German's: the percent sign
//! attaches ("25%"), the euro sign goes in front with a space ("€ 25"), degrees
//! attach ("30°"), and measures take a space ("10 km").
//!
//! **Clock times** convert only after "om", which is what makes them times:
//! "om half drie", "om kwart over drie", "om tien voor vier", "om vijf over
//! half vier". Dutch counts relative to the hour, and "half" to the *next*
//! one — "half drie" is 2:30, not 3:30 — so "om half drie" is written "om 2:30",
//! "om tien voor vier" "om 3:50", "om vijf over half vier" "om 3:35". The hour
//! is one to twelve and so is the written form: "kwart over drie" says nothing
//! about morning or afternoon, and "15:15" would be a guess.
//!
//! ponytail: clock times without "om" stay words ("het is half drie", "tot
//! kwart over drie"): "half drie" alone is as readable as half of three, and
//! "tien voor vier" as ten for four. "drie uur" stays too, even after "om",
//! because it is as often a duration as a time. After "om" the known ceiling is
//! "om" meaning "in order to": "om tien voor vier mensen te koken" would read
//! as a time. A unit or another number after the hour blocks that ("om tien
//! voor vier euro te kopen"), and so does "een" — the article — anywhere but
//! the end of the phrase ("om vijf over een hek", "om een voor een"); a plain
//! noun does not, and telling it apart needs a dictionary of nouns.
//!
//! Also left: years said in two parts ("negentien negenennegentig", "twintig
//! zesentwintig"), which Dutch speakers do use; telling them from two numbers
//! read out needs the same evidence the English year rule guesses at, and a
//! wrong guess here is invisible. Millions ("drie miljoen") are left too.

use super::de::{number_at, pieces, split, value, Part, Words};

use Part::{And, Num};

/// Dutch number morphemes, with the diaeresis and accents folded away.
const MORPHEMES: &[(&str, Part)] = &[
    ("nul", Num(0)),
    ("een", Num(1)),
    ("twee", Num(2)),
    ("drie", Num(3)),
    ("vier", Num(4)),
    ("vijf", Num(5)),
    ("zes", Num(6)),
    ("zeven", Num(7)),
    ("acht", Num(8)),
    ("negen", Num(9)),
    ("tien", Num(10)),
    ("elf", Num(11)),
    ("twaalf", Num(12)),
    ("dertien", Num(13)),
    ("veertien", Num(14)),
    ("vijftien", Num(15)),
    ("zestien", Num(16)),
    ("zeventien", Num(17)),
    ("achttien", Num(18)),
    ("negentien", Num(19)),
    ("twintig", Num(20)),
    ("dertig", Num(30)),
    ("veertig", Num(40)),
    ("vijftig", Num(50)),
    ("zestig", Num(60)),
    ("zeventig", Num(70)),
    ("tachtig", Num(80)),
    ("negentig", Num(90)),
    ("honderd", Num(100)),
    ("duizend", Num(1_000)),
    ("en", And),
];

/// The article, compared before accents are folded, so "één" gets through.
const ARTICLES: &[&str] = &["een"];

/// Units written after the number: spoken form, symbol, and whether a space
/// separates them. The euro is handled apart because it goes in front.
const UNITS: &[(&str, &str, bool)] = &[
    ("procent", "%", false),
    ("graden", "\u{b0}", false),
    ("graad", "\u{b0}", false),
    ("kilometer", "km", true),
    ("kilogram", "kg", true),
    ("kilo", "kg", true),
];

/// Convert spelled-out Dutch numbers, ordinals and units to their written
/// forms.
pub fn apply(text: &str) -> String {
    let w = Words::new(text);
    let mut edits = Vec::new();
    let mut i = 0;

    while i < w.words.len() {
        match match_clock(&w, i)
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
        let unit = w.key(last + 1);
        if unit == Some("euro") {
            return Some((len + 1, format!("\u{20ac} {value}")));
        }
        if let Some((_, symbol, spaced)) = UNITS.iter().find(|(u, ..)| unit == Some(*u)) {
            let gap = if *spaced { " " } else { "" };
            return Some((len + 1, format!("{value}{gap}{symbol}")));
        }
    }
    // "negentien negenennegentig" is a year said in two parts. Converting only
    // the compound half would write "negentien 99", which is worse than either
    // reading, so a two-digit compound straight after a lone ten-to-twenty
    // stays a word along with it.
    let century = i
        .checked_sub(1)
        .filter(|prev| w.joined(*prev))
        .and_then(|prev| number_at(w, prev, MORPHEMES, ARTICLES))
        .is_some_and(|(_, v, p)| p == 1 && (10..=20).contains(&v));
    if century && value < 100 {
        return None;
    }
    (pieces > 1).then(|| (len, value.to_string()))
}

/// A clock time after "om", starting at `i`: "half drie", "kwart over drie",
/// "tien voor vier", "vijf over half vier". See the module docs for why each
/// needs its "om".
fn match_clock(w: &Words, i: usize) -> Option<(usize, String)> {
    let om = i.checked_sub(1)?;
    if w.key(om) != Some("om") {
        return None;
    }
    // The value of a number word at `p` — fused compounds included, so
    // "vijfentwintig over drie" reads — unless it is out of `range`.
    let at = |p: usize, range: std::ops::RangeInclusive<u64>| {
        w.key(p)
            .and_then(|k| value(&split(k, MORPHEMES)?))
            .filter(|v| range.contains(v))
    };

    // Minutes before or after the hour at the end, counted in the words
    // leading up to it: "kwart over" is +15, "tien voor" -10, "half" -30.
    let mut p = i;
    let mut offset: i64 = 0;
    let by = match w.key(p)? {
        "half" => None,
        "kwart" => Some(15),
        // Never one: "om een voor een te controleren" is "one by one".
        _ => Some(at(p, 2..=29)? as i64),
    };
    if let Some(by) = by {
        offset = match w.key(p + 1)? {
            "over" => by,
            "voor" => -by,
            _ => return None,
        };
        p += 2;
    }
    if w.key(p) == Some("half") {
        // "kwart over half" is not Dutch; only a count of minutes goes there.
        if w.key(i) == Some("kwart") {
            return None;
        }
        offset -= 30;
        p += 1;
    }
    let hour = at(p, 1..=12)?;
    let len = p + 1 - i;

    // One phrase from "om" to the hour: "om half, drie" is not a time.
    if !(om..p).all(|q| w.joined(q)) {
        return None;
    }
    // "om tien voor vier euro" is a price and "om half drie vijf" is not a
    // time the grammar reads. An unaccented "een" is the article unless the
    // phrase ends on it: "om vijf over een hek te klimmen".
    if w.joined(p) {
        let next = w.key(p + 1);
        if w.raw(p) == Some("een")
            || next == Some("euro")
            || UNITS.iter().any(|(u, ..)| next == Some(*u))
            || at(p + 1, 0..=u64::MAX).is_some()
        {
            return None;
        }
    }

    // Always at least one minute past midnight: the smallest offset is "29
    // voor half één", 0:01. Hour zero is written as twelve.
    let total = hour as i64 * 60 + offset;
    let (h, m) = (total / 60, total % 60);
    Some((len, format!("{}:{m:02}", if h == 0 { 12 } else { h })))
}

/// A compound "-ste" ordinal at `i`: "vijfentwintigste" → "25e".
///
/// "-ste" is also the superlative ("grootste", "meeste"), which is harmless:
/// the stem has to be a number end to end, and "groot" is not.
fn match_ordinal(w: &Words, i: usize) -> Option<(usize, String)> {
    let stem = w.key(i)?.strip_suffix("ste")?;
    let parts = split(stem, MORPHEMES)?;
    if pieces(&parts) < 2 {
        return None;
    }
    Some((1, format!("{}e", value(&parts)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::format::FormatOptions;

    #[test]
    fn fused_compounds_become_digits() {
        assert_eq!(apply("we hebben vijfendertig nodig"), "we hebben 35 nodig");
        assert_eq!(apply("eenentwintig"), "21");
        assert_eq!(apply("achtentachtig"), "88");
        assert_eq!(apply("driehonderdvijfenveertig"), "345");
        assert_eq!(apply("eenhonderd"), "100");
        assert_eq!(apply("tienduizend"), "10000");
    }

    #[test]
    fn the_diaeresis_is_optional() {
        assert_eq!(apply("tweeëntwintig"), "22");
        assert_eq!(apply("tweeentwintig"), "22");
        assert_eq!(apply("drieëndertig"), "33");
    }

    #[test]
    fn a_lone_simple_word_stays_a_word() {
        for said in [
            "drie dagen",
            "tien minuten",
            "honderd keer",
            "duizend dank",
            "twintig",
            "dertien",
            "één van de beste",
            "in acht nemen",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    /// "een" is the article; "één" is the number, and only a unit lets even
    /// that one convert.
    #[test]
    fn the_article_is_not_a_number() {
        for said in [
            "een hond en een kat",
            "een procent meer",
            "een honderd",
            "een duizend keer",
            "een euro",
        ] {
            assert_eq!(apply(said), said);
        }
        assert_eq!(apply("één procent meer"), "1% meer");
        assert_eq!(apply("één euro"), "\u{20ac} 1");
    }

    /// Whole tokens only: a word must be made of number morphemes end to end.
    #[test]
    fn words_containing_number_morphemes_are_left_alone() {
        for said in [
            "nog eens",
            "de tweede keer",
            "we gaan vieren",
            "achter het huis",
            "met z'n drieën",
            "honderden mensen",
            "duizenden jaren",
            "de eenheid",
            "het grootste deel",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn split_compounds_are_rejoined_across_a_scale_word() {
        assert_eq!(apply("twee honderd mensen"), "200 mensen");
        assert_eq!(apply("twee duizend zesentwintig"), "2026");
        // No scale word at the gap, or a standalone "en": left as spoken.
        assert_eq!(apply("vier tien"), "vier tien");
        assert_eq!(apply("twee en twintig"), "twee en twintig");
        assert_eq!(apply("drie, honderd"), "drie, honderd");
    }

    #[test]
    fn years_are_plain_values() {
        assert_eq!(apply("sinds negentienhonderdnegenennegentig"), "sinds 1999");
        assert_eq!(apply("in tweeduizendzesentwintig"), "in 2026");
    }

    /// Left whole rather than half converted into "negentien 99".
    #[test]
    fn two_part_spoken_years_are_left_alone() {
        for said in ["in negentien negenennegentig", "twintig zesentwintig"] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn compound_ordinals_take_an_e() {
        assert_eq!(apply("de vijfentwintigste juni"), "de 25e juni");
        assert_eq!(apply("de eenentwintigste eeuw"), "de 21e eeuw");
        // Lone ordinals are prose.
        for said in ["de eerste keer", "de twintigste", "de achtste", "de tiende"] {
            assert_eq!(apply(said), said);
        }
    }

    /// Taalunie: "25%", "€ 25", "30°", "10 km".
    #[test]
    fn a_unit_settles_a_lone_number() {
        assert_eq!(apply("vijf procent"), "5%");
        assert_eq!(apply("het kost twintig euro"), "het kost \u{20ac} 20");
        assert_eq!(apply("dertig graden vandaag"), "30\u{b0} vandaag");
        assert_eq!(apply("tien kilometer"), "10 km");
        assert_eq!(apply("vijf kilo"), "5 kg");
    }

    /// "half" counts to the next hour, not from the last one.
    #[test]
    fn clock_times_after_om_use_a_colon() {
        assert_eq!(apply("om half drie"), "om 2:30");
        assert_eq!(apply("om kwart over drie"), "om 3:15");
        assert_eq!(apply("om kwart voor drie"), "om 2:45");
        assert_eq!(apply("om tien voor vier"), "om 3:50");
        assert_eq!(apply("om vijf over half vier"), "om 3:35");
        assert_eq!(apply("om tien voor half vier"), "om 3:20");
        assert_eq!(apply("om half een"), "om 12:30");
        assert_eq!(apply("om vijf over twaalf"), "om 12:05");
        assert_eq!(
            apply("we spreken om half drie af."),
            "we spreken om 2:30 af."
        );
        assert_eq!(apply("Om Kwart Over Drie"), "Om 3:15");
        assert_eq!(apply("om kwart over één vandaag"), "om 1:15 vandaag");
        // A price, not a time. The number stage reads the price as it always did.
        assert_eq!(
            apply("om tien voor vier euro te kopen"),
            "om tien voor \u{20ac} 4 te kopen"
        );
    }

    #[test]
    fn a_clock_shape_without_om_stays_words() {
        for said in [
            "half drie",
            "het is half drie",
            "kwart over drie",
            "tot tien voor vier",
            "drie uur",
            "om drie uur",
            "om drie",
            "om half",
            "om een voor een te controleren",
            "om vijf over een hek te klimmen",
            "om half een brood",
            "om dertig over drie",
            "om half dertien",
            "om kwart over half drie",
            "om half, drie",
            "om half drie vijf",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn punctuation_and_line_breaks_survive() {
        assert_eq!(apply("het waren vijfendertig."), "het waren 35.");
        assert_eq!(apply("(vijf euro)"), "(\u{20ac} 5)");
        assert_eq!(apply("eenentwintig\ntweeëntwintig"), "21\n22");
    }

    #[test]
    fn ordinary_sentences_are_untouched() {
        for said in [
            "Dit is een heel gewone zin.",
            "Kun je me  even helpen?",
            "Ik heb er een paar gezien",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    /// The written forms have to survive every later stage: tidy strips spaces
    /// before "%" and after an opening mark, and capitalises after ".".
    #[test]
    fn output_survives_the_whole_pipeline() {
        let all = FormatOptions {
            cleanup: true,
            spoken_punctuation: true,
            numbers: true,
            tidy: true,
        };
        let run = |said| crate::core::format::apply(said, all, Some("nl"));
        assert_eq!(
            run("de prijzen stijgen vijfentwintig procent punt"),
            "De prijzen stijgen 25%."
        );
        assert_eq!(
            run("het kost twintig euro komma zegt hij"),
            "Het kost \u{20ac} 20, zegt hij"
        );
        assert_eq!(
            run("in de eenentwintigste eeuw punt sinds tweeduizendzesentwintig"),
            "In de 21e eeuw. Sinds 2026"
        );
    }
}
