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
//! ponytail: clock times are left as words. The idiomatic ones are relative —
//! "half drie" is 2:30, not 3:30, and "kwart over drie" says nothing about
//! morning or afternoon — and "drie uur" is as often a duration as a time. Also
//! left: years said in two parts ("negentien negenennegentig", "twintig
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
        match match_ordinal(&w, i).or_else(|| match_number(&w, i)) {
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

    #[test]
    fn clock_times_are_left_alone() {
        for said in ["om half drie", "kwart over drie", "om drie uur"] {
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
