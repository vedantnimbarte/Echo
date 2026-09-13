//! Spanish spelled-out numbers and the units that settle them.
//!
//! The English module's rule holds here unchanged: **never convert a lone
//! small word.** "Uno de los mejores" must survive, and so must "mil gracias"
//! and "a las quinientas", which are idioms and not quantities. A number is
//! written as digits when the speaker gave more than one word of it ("treinta y
//! cinco", "dos mil veintiséis"), or when a unit follows and settles the matter
//! ("cinco por ciento").
//!
//! **Where one word is enough.** Spanish fuses 21–29 into a single word:
//! "veinticinco" is what English spells "twenty five", and English converts
//! that. So the *veinti-* compounds convert on their own, and nothing else
//! does. The line is drawn exactly where English draws it, so the same spoken
//! quantity comes out the same in both languages:
//!
//! - "dieciséis" is a compound by etymology, but it is the word for "sixteen",
//!   and a lone "sixteen" stays a word. So does "dieciséis".
//! - "veinte", "cien", "doscientos", "mil" are one round word each. English
//!   leaves "twenty" and "thousand" alone, and Spanish style guides write round
//!   numbers in letters too — "doscientos años", not "200 años".
//!
//! **Articles are the trap.** "un" and "una" are "a/an" far more often than
//! they are 1, so neither ever starts a number, with one exception: "un millón"
//! followed by more of the number ("un millón quinientos mil"). A bare "un
//! millón" stays as words — "un millón de gracias" is a figure of speech, and
//! the digits 1000000 in its place would be absurd. After "y" the article
//! forms are unambiguous ("treinta y un días") and count as 1.
//!
//! **"y" joins only a tens and a unit.** "treinta y cinco" is 35; "cinco y
//! veinte" is a clock time, "entre veinte y treinta" is two numbers, and "mil y
//! una noches" is a title. Modern Spanish never puts "y" anywhere else inside a
//! number, so anywhere else it is the ordinary word "and".
//!
//! **Clock times need the words that make them clock times.** "tres y media"
//! is as often three and a half cups as 3:30, and "cinco y veinte" is as often
//! a sum as 5:20. What settles it is the article a time takes: "a las tres y
//! media", "son las cinco y veinte", "la una menos cuarto". After those, and
//! only those, an hour from one to twelve followed by "y" or "menos" and its
//! minutes is written "3:30", "7:45", "5:20". The article stays as spoken — "a
//! las 3:30" is how the RAE writes it. "menos" counts back from the hour, so
//! "las ocho menos cuarto" is 7:45 and "la una menos diez" 12:50.
//!
//! **A run of number words converts whole or not at all.** "a las ocho treinta
//! y cinco" is a clock time without its "y", which this module does not read. Converting the part it
//! can parse would give "a las ocho 35" — a figure half-rewritten, which is
//! worse than leaving the words. So when the grammar stops while number words
//! carry on, the whole run is left as spoken.
//!
//! ponytail: clock times without their article are skipped — "desde las tres y
//! media", "de tres y media a cinco", "las tres y media" said bare — because
//! "de las tres" is as often "of the three" and a bare "tres y media" is a
//! quantity. So are hours past twelve ("las catorce y treinta") and a time
//! with no "y" ("a las ocho treinta"), which is the same shape as two numbers
//! read out. Decimals are skipped because "coma" is already a spoken comma. Ordinals
//! ("vigésimo quinto") are skipped because dates use cardinals in Spanish.
//! "billón" is skipped because it is 10¹² in Spanish and 10⁹ to anyone
//! calquing English, and "mil millones" is left as words rather than parsed.
//! A number converted wrongly is worse than one left as words, because the
//! reader cannot tell it was Echo that changed it.

use crate::core::format::words;

/// What a number word can combine with, which is the whole grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// "un", "una": 1 only after "y" or before "millón". Otherwise "a/an".
    Article,
    /// "uno" to "nueve". Takes nothing after it.
    Unit,
    /// A word below a hundred that nothing joins: "diez" to "diecinueve", and
    /// "veinte", since modern Spanish fuses 21–29 rather than saying "veinte y".
    Whole,
    /// "veintiuno" to "veintinueve": the only single words that convert alone.
    Fused,
    /// "treinta" to "noventa", which take "y" and a unit.
    Tens,
    /// "doscientos" to "novecientos", in either gender, which take anything
    /// below a hundred directly after them: "doscientos cincuenta".
    Hundreds,
    /// "cien" is complete: "cien mil" is a scale, and "cien veinte" is not
    /// Spanish.
    Cien,
    /// "ciento" never stands alone — it needs what comes after ("ciento
    /// veinte"). On its own it is almost always "por ciento".
    Ciento,
    /// "mil", "millón", "millones", which multiply what came before them.
    Scale,
}

/// Every number word, unaccented. Whisper drops accents often enough that
/// "veintiseis" and "veintiséis" must both match, so keys are folded before
/// lookup (see [`fold`]) and the table only needs one spelling of each.
const NUMBER_WORDS: &[(&str, u64, Kind)] = &[
    ("un", 1, Kind::Article),
    ("una", 1, Kind::Article),
    ("uno", 1, Kind::Unit),
    ("dos", 2, Kind::Unit),
    ("tres", 3, Kind::Unit),
    ("cuatro", 4, Kind::Unit),
    ("cinco", 5, Kind::Unit),
    ("seis", 6, Kind::Unit),
    ("siete", 7, Kind::Unit),
    ("ocho", 8, Kind::Unit),
    ("nueve", 9, Kind::Unit),
    ("diez", 10, Kind::Whole),
    ("once", 11, Kind::Whole),
    ("doce", 12, Kind::Whole),
    ("trece", 13, Kind::Whole),
    ("catorce", 14, Kind::Whole),
    ("quince", 15, Kind::Whole),
    ("dieciseis", 16, Kind::Whole),
    ("diecisiete", 17, Kind::Whole),
    ("dieciocho", 18, Kind::Whole),
    ("diecinueve", 19, Kind::Whole),
    ("veinte", 20, Kind::Whole),
    // "veintiún" before a masculine noun or "mil", "veintiuna" before a
    // feminine one, "veintiuno" on its own. All three are 21.
    ("veintiun", 21, Kind::Fused),
    ("veintiuna", 21, Kind::Fused),
    ("veintiuno", 21, Kind::Fused),
    ("veintidos", 22, Kind::Fused),
    ("veintitres", 23, Kind::Fused),
    ("veinticuatro", 24, Kind::Fused),
    ("veinticinco", 25, Kind::Fused),
    ("veintiseis", 26, Kind::Fused),
    ("veintisiete", 27, Kind::Fused),
    ("veintiocho", 28, Kind::Fused),
    ("veintinueve", 29, Kind::Fused),
    ("treinta", 30, Kind::Tens),
    ("cuarenta", 40, Kind::Tens),
    ("cincuenta", 50, Kind::Tens),
    ("sesenta", 60, Kind::Tens),
    ("setenta", 70, Kind::Tens),
    ("ochenta", 80, Kind::Tens),
    ("noventa", 90, Kind::Tens),
    ("cien", 100, Kind::Cien),
    ("ciento", 100, Kind::Ciento),
    // The hundreds agree in gender with the noun: "doscientas personas".
    ("doscientos", 200, Kind::Hundreds),
    ("doscientas", 200, Kind::Hundreds),
    ("trescientos", 300, Kind::Hundreds),
    ("trescientas", 300, Kind::Hundreds),
    ("cuatrocientos", 400, Kind::Hundreds),
    ("cuatrocientas", 400, Kind::Hundreds),
    ("quinientos", 500, Kind::Hundreds),
    ("quinientas", 500, Kind::Hundreds),
    ("seiscientos", 600, Kind::Hundreds),
    ("seiscientas", 600, Kind::Hundreds),
    ("setecientos", 700, Kind::Hundreds),
    ("setecientas", 700, Kind::Hundreds),
    ("ochocientos", 800, Kind::Hundreds),
    ("ochocientas", 800, Kind::Hundreds),
    ("novecientos", 900, Kind::Hundreds),
    ("novecientas", 900, Kind::Hundreds),
    ("mil", 1_000, Kind::Scale),
    ("millon", 1_000_000, Kind::Scale),
    ("millones", 1_000_000, Kind::Scale),
];

/// How a unit is written once the number in front of it is digits.
enum Written {
    /// Appended to the digits. The spaced ones use a no-break space: Spanish
    /// (RAE) sets "25 %" and "25 €" apart, a line break between figure and
    /// symbol is a typography error, and the tidy stage strips an ordinary
    /// space before "%" — it only ever collapses U+0020.
    After(&'static str),
    /// The number converts and the unit word stays as spoken. For currencies
    /// whose symbol depends on the country: "$" is pesos in Mexico and dollars
    /// in Miami, so "veinte dólares" becomes "20 dólares", which is right
    /// everywhere.
    KeepWord,
}

/// Units that follow a number, as the keys they are spoken as.
///
/// `article` marks the one unit an article may take. "un kilo" and "un euro"
/// are "a kilo" and "a euro" and stay words; "un por ciento" has no reading
/// but 1 %.
const UNIT_WORDS: &[(&[&str], Written, bool)] = &[
    (&["por", "ciento"], Written::After("\u{a0}%"), true),
    // Colloquial in Spain ("el diez por cien"), and just as unambiguous.
    (&["por", "cien"], Written::After("\u{a0}%"), true),
    (&["euros"], Written::After("\u{a0}\u{20ac}"), false),
    (&["dolares"], Written::KeepWord, false),
    (&["pesos"], Written::KeepWord, false),
    // Attached, as English does: "30°" is right for an angle and is how a
    // temperature is written when the scale is not said.
    (&["grados"], Written::After("\u{b0}"), false),
    (&["kilometros"], Written::After("\u{a0}km"), false),
    (&["kilos"], Written::After("\u{a0}kg"), false),
    (&["kilogramos"], Written::After("\u{a0}kg"), false),
];

/// Convert spelled-out Spanish numbers and their units to written forms.
pub fn apply(text: &str) -> String {
    let words = words(text);
    let keys: Vec<String> = words.iter().map(|w| key(w)).collect();

    // A number never runs across punctuation: "veinte, cinco" is a list, not
    // 25. So each position can see only as far as the next mark, and the
    // parser is handed that stretch as its whole world. Worked out backwards
    // in one pass, because each word's reach is its neighbour's plus one.
    let mut reach = vec![words.len(); words.len()];
    for p in (0..words.len().saturating_sub(1)).rev() {
        reach[p] = if glued(words[p], words[p + 1]) {
            reach[p + 1]
        } else {
            p + 1
        };
    }

    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        // The clock looks back at the article before the hour, so it is handed
        // everything up to the end of the clause rather than the clause alone.
        if let Some((len, written)) = clock(&keys[..reach[i]], i) {
            out.push(rewrap(words[i], words[i + len - 1], &written));
            i += len;
            continue;
        }
        let clause = &keys[i..reach[i]];
        if let Some((len, written)) = convert(clause) {
            out.push(rewrap(words[i], words[i + len - 1], &written));
            i += len;
            continue;
        }
        // Not converted: step over the whole run of number words, so the
        // parser does not re-enter halfway through it and convert the tail.
        let len = left_as_spoken(clause);
        out.extend(words[i..i + len].iter().map(|w| w.to_string()));
        i += len;
    }

    out.join(" ")
}

/// The written form of the number starting a clause, and how many words it
/// replaces, if it should be converted at all.
fn convert(clause: &[String]) -> Option<(usize, String)> {
    let (_, kind) = number(clause, 0)?;

    let Some((len, value)) = cardinal(clause) else {
        // An article that is not the start of a number can still be 1 before
        // the one unit it has no other reading with.
        if kind == Kind::Article {
            return unit(clause, 1, 1)
                .filter(|(_, _, article)| *article)
                .map(|(unit_len, written, _)| (1 + unit_len, written));
        }
        return None;
    };

    // The grammar stopped but the number words did not: leave the lot.
    if len != run_end(clause) {
        return None;
    }
    if let Some((unit_len, written, _)) = unit(clause, len, value) {
        return Some((len + unit_len, written));
    }
    (len > 1 || kind == Kind::Fused).then(|| (len, value.to_string()))
}

/// A clock time whose hour is at `i`, returning how many words it spans and
/// how it is written. See the module docs for the shapes and why each needs its
/// article.
fn clock(keys: &[String], i: usize) -> Option<(usize, String)> {
    let before = |n: usize| i.checked_sub(n).map(|p| keys[p].as_str());
    let hour = match number(keys, i)? {
        // "la una" is the only singular hour, and "una" alone is "a/an".
        (1, Kind::Article) if keys[i] == "una" && before(1) == Some("la") => 1,
        (hour @ 2..=12, _)
            if before(1) == Some("las") && matches!(before(2), Some("a" | "son")) =>
        {
            hour
        }
        _ => return None,
    };
    let (len, minutes) = match (
        keys.get(i + 1).map(String::as_str),
        keys.get(i + 2).map(String::as_str),
    ) {
        (Some("y"), Some("media")) => (3, 30),
        (Some("y" | "menos"), Some("cuarto")) => (3, 15),
        (Some("y" | "menos"), _) => {
            let (len, minutes) = below_hundred(keys, i + 2)?;
            (2 + len, minutes)
        }
        _ => return None,
    };
    // "y cinco seis" is not a time the grammar reads: leave the run whole, as
    // everywhere else in this module.
    if number(keys, i + len).is_some() {
        return None;
    }
    let (hour, minutes) = match keys[i + 1].as_str() {
        "y" if (1..60).contains(&minutes) => (hour, minutes),
        // Nobody says "menos cuarenta"; past half the hour is said forwards.
        "menos" if (1..=30).contains(&minutes) => {
            (if hour == 1 { 12 } else { hour - 1 }, 60 - minutes)
        }
        _ => return None,
    };
    Some((len, format!("{hour}:{minutes:02}")))
}

/// How many words to pass over unchanged at the start of a clause: the whole
/// run if it begins with a number word, otherwise just the one word.
fn left_as_spoken(clause: &[String]) -> usize {
    match number(clause, 0) {
        Some((_, kind)) if kind != Kind::Article => run_end(clause),
        _ => 1,
    }
}

/// One past the last word of the run of number words starting the clause.
///
/// A run is number words back to back, plus "y" wherever a number word
/// follows it. Articles do not extend a run on their own — "cuesta veinticinco
/// un kilo" is 25 and "a kilo" — but after "y" they do, because "treinta y un"
/// is one number.
fn run_end(clause: &[String]) -> usize {
    let mut p = 1;
    while p < clause.len() {
        if number(clause, p).is_some_and(|(_, kind)| kind != Kind::Article) {
            p += 1;
        } else if clause[p] == "y" && number(clause, p + 1).is_some() {
            p += 2;
        } else {
            break;
        }
    }
    p
}

/// The value and kind of the word at `p`, if it is a number word.
fn number(clause: &[String], p: usize) -> Option<(u64, Kind)> {
    let k = clause.get(p)?;
    NUMBER_WORDS
        .iter()
        .find(|(w, _, _)| w == k)
        .map(|(_, v, kind)| (*v, *kind))
}

/// The value of a scale word at `p`, if that is what it is.
fn scale(clause: &[String], p: usize) -> Option<u64> {
    number(clause, p)
        .filter(|(_, kind)| *kind == Kind::Scale)
        .map(|(v, _)| v)
}

/// A number below a hundred at `p`: "cinco", "quince", "veintiséis", "treinta
/// y un". Never an article on its own: "doscientos un" is as likely "two
/// hundred, a…" as 201, and "ciento uno" is how 101 is said anyway.
fn below_hundred(clause: &[String], p: usize) -> Option<(usize, u64)> {
    let (value, kind) = number(clause, p)?;
    match kind {
        Kind::Unit | Kind::Whole | Kind::Fused => Some((1, value)),
        // "y" joins only when a unit really follows it — the article forms
        // included, since "treinta y un" is unambiguous. Otherwise it is the
        // word "and" and the tens stands alone.
        Kind::Tens => match (clause.get(p + 1).map(String::as_str), number(clause, p + 2)) {
            (Some("y"), Some((unit, Kind::Unit | Kind::Article))) => Some((3, value + unit)),
            _ => Some((1, value)),
        },
        _ => None,
    }
}

/// A number below a thousand at `p`: "cien", "ciento veinte", "trescientos
/// cuarenta y dos", or anything [`below_hundred`] accepts. An article never
/// starts one.
fn group(clause: &[String], p: usize) -> Option<(usize, u64)> {
    let (value, kind) = number(clause, p)?;
    match kind {
        Kind::Hundreds => Some(match below_hundred(clause, p + 1) {
            Some((len, rest)) => (1 + len, value + rest),
            None => (1, value),
        }),
        Kind::Ciento => below_hundred(clause, p + 1).map(|(len, rest)| (1 + len, value + rest)),
        Kind::Cien => Some((1, value)),
        _ => below_hundred(clause, p),
    }
}

/// Match a whole cardinal at the start of the clause, returning how many words
/// it spans and its value.
///
/// A cardinal is groups below a thousand separated by scale words, each scale
/// smaller than the last: "dos millones trescientos mil cuarenta". Spanish
/// puts no conjunction after a scale, so neither does this.
fn cardinal(clause: &[String]) -> Option<(usize, u64)> {
    let mut p = 0;
    let mut total = 0u64;
    let mut last_scale = u64::MAX;

    // "un millón" is the one place an article starts a number, and only if
    // more of the number follows — see the module docs.
    let article_million = number(clause, 0).is_some_and(|(_, kind)| kind == Kind::Article)
        && scale(clause, 1) == Some(1_000_000);

    loop {
        let found = if p == 0 && article_million {
            Some((1, 1))
        } else {
            group(clause, p)
        };
        let group_len = found.map_or(0, |(len, _)| len);

        match scale(clause, p + group_len).filter(|s| *s < last_scale) {
            Some(scale) => {
                let multiplier = match found {
                    // "uno mil" is not Spanish; the multiplier 1 is left
                    // unsaid ("mil") or apocopated ("un millón").
                    Some((1, 1)) if !(p == 0 && article_million) => break,
                    Some((_, value)) => value,
                    // "mil" on its own means a thousand. "millón" does not.
                    None if scale == 1_000 => 1,
                    None => break,
                };
                total += multiplier * scale;
                last_scale = scale;
                p += group_len + 1;
            }
            None => {
                if let Some((len, value)) = found {
                    total += value;
                    p += len;
                }
                break;
            }
        }
    }

    // A bare "un millón" ends right after the scale word.
    if article_million && p == 2 {
        return None;
    }
    (p > 0).then_some((p, total))
}

/// A unit at `p` following a number, returning how many words it spans, the
/// written number-plus-unit, and whether an article may take it.
fn unit(clause: &[String], p: usize, value: u64) -> Option<(usize, String, bool)> {
    UNIT_WORDS.iter().find_map(|(phrase, written, article)| {
        let spoken = clause.get(p..p + phrase.len())?;
        if spoken.iter().zip(phrase.iter()).any(|(s, w)| s != w) {
            return None;
        }
        Some(match written {
            Written::After(symbol) => (phrase.len(), format!("{value}{symbol}"), *article),
            Written::KeepWord => (0, value.to_string(), *article),
        })
    })
}

/// Whether two neighbouring words may belong to one number: nothing but a
/// space between them. "veinte, cinco" and "veinte (cinco" are two numbers.
fn glued(left: &str, right: &str) -> bool {
    left.chars().last().is_some_and(char::is_alphanumeric)
        && right.chars().next().is_some_and(char::is_alphanumeric)
}

/// Put the punctuation that surrounded the spoken words back around what
/// replaced them, so "¿veinticinco?" becomes "¿25?" rather than losing its
/// marks with the words.
fn rewrap(first: &str, last: &str, written: &str) -> String {
    let lead = first.len()
        - first
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .len();
    let trail = last.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
    format!("{}{written}{}", &first[..lead], &last[trail..])
}

/// The comparison form of a word: lowercased, accents folded, and every
/// surrounding mark removed.
///
/// Not the shared [`crate::core::format::key`], which trims ASCII punctuation
/// only: Spanish opens questions and exclamations with "¿" and "¡", and a
/// number at the start of one would never match.
fn key(word: &str) -> String {
    fold(
        &word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase(),
    )
}

/// Strip the accents Spanish number and unit words carry, so a transcript that
/// lost them ("veintiseis", "kilometros") matches the same entry.
///
/// Folding can only merge words that differ by an accent, and no Spanish word
/// differing from a number word only by an accent is itself common: "dé"/"de"
/// and "sé"/"se" fold together, but neither is in the tables.
fn fold(word: &str) -> String {
    word.chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_word_numbers_become_digits() {
        assert_eq!(apply("treinta y cinco personas"), "35 personas");
        assert_eq!(apply("en dos mil veintiséis"), "en 2026");
        assert_eq!(apply("ciento veinte páginas"), "120 páginas");
        assert_eq!(apply("trescientos cuarenta y dos"), "342");
        assert_eq!(apply("mil novecientos noventa y nueve"), "1999");
        assert_eq!(apply("cien mil habitantes"), "100000 habitantes");
        assert_eq!(apply("dos millones trescientos mil"), "2300000");
    }

    /// The rule the whole module hangs on.
    #[test]
    fn a_lone_simple_number_stays_a_word() {
        assert_eq!(apply("uno de los mejores"), "uno de los mejores");
        assert_eq!(apply("dame dos"), "dame dos");
        assert_eq!(apply("cinco"), "cinco");
        assert_eq!(apply("tiene quince años"), "tiene quince años");
        // Round single words, and the idioms built on them.
        assert_eq!(apply("mil gracias"), "mil gracias");
        assert_eq!(apply("llegó a las quinientas"), "llegó a las quinientas");
        assert_eq!(apply("doscientos años"), "doscientos años");
    }

    /// "veinticinco" is "twenty five" in one word, and converts as English
    /// does. "dieciséis" is "sixteen", and stays a word as English does.
    #[test]
    fn a_fused_twenty_converts_alone_but_a_teen_does_not() {
        assert_eq!(apply("tengo veinticinco años"), "tengo 25 años");
        assert_eq!(apply("veintiún días"), "21 días");
        assert_eq!(apply("veintiuna personas"), "21 personas");
        assert_eq!(apply("dieciséis"), "dieciséis");
        // Inside a longer number a teen is part of it like anything else.
        assert_eq!(apply("mil dieciséis"), "1016");
    }

    /// Whisper drops accents; the number must not depend on whether it did.
    #[test]
    fn accents_are_optional() {
        assert_eq!(apply("veintiséis"), "26");
        assert_eq!(apply("veintiseis"), "26");
        assert_eq!(apply("VEINTIDÓS"), "22");
        assert_eq!(apply("mil dieciseis"), "1016");
    }

    #[test]
    fn gender_forms_count_the_same() {
        assert_eq!(apply("doscientas cincuenta"), "250");
        assert_eq!(apply("treinta y una"), "31");
    }

    /// "un" and "una" are "a/an". They are 1 only after "y", or starting a
    /// million that carries on.
    #[test]
    fn articles_are_not_ones() {
        assert_eq!(apply("un perro"), "un perro");
        assert_eq!(apply("una vez"), "una vez");
        assert_eq!(apply("un kilo de tomates"), "un kilo de tomates");
        assert_eq!(apply("un euro"), "un euro");
        assert_eq!(apply("un mil"), "un mil");
        assert_eq!(apply("treinta y un días"), "31 días");
    }

    #[test]
    fn un_millon_converts_only_when_more_of_the_number_follows() {
        assert_eq!(apply("un millón de gracias"), "un millón de gracias");
        assert_eq!(apply("un millón"), "un millón");
        assert_eq!(apply("un millón quinientos mil"), "1500000");
    }

    /// "y" is "and" everywhere except between a tens and a unit.
    #[test]
    fn y_between_other_numbers_is_the_word_and() {
        assert_eq!(apply("entre veinte y treinta"), "entre veinte y treinta");
        assert_eq!(apply("las mil y una noches"), "las mil y una noches");
        assert_eq!(apply("cinco y seis"), "cinco y seis");
        assert_eq!(apply("treinta y tantos"), "treinta y tantos");
    }

    /// A clock time is a run the grammar cannot read. Converting the part it
    /// can would leave a figure half-rewritten.
    #[test]
    fn a_run_the_grammar_cannot_read_is_left_whole() {
        assert_eq!(
            apply("a las ocho treinta y cinco"),
            "a las ocho treinta y cinco"
        );
        assert_eq!(apply("las tres y media"), "las tres y media");
        assert_eq!(apply("dos mil millones"), "dos mil millones");
        assert_eq!(apply("cien veinte"), "cien veinte");
    }

    #[test]
    fn clock_times_after_their_article_use_a_colon() {
        assert_eq!(apply("a las tres y media"), "a las 3:30");
        assert_eq!(apply("a las ocho menos cuarto"), "a las 7:45");
        assert_eq!(apply("a las cinco y veinte"), "a las 5:20");
        assert_eq!(apply("son las doce y cuarto"), "son las 12:15");
        assert_eq!(apply("a las ocho y treinta y cinco"), "a las 8:35");
        assert_eq!(apply("a las nueve menos diez"), "a las 8:50");
        assert_eq!(apply("es la una y media."), "es la 1:30.");
        assert_eq!(apply("a la una menos cuarto"), "a la 12:45");
        assert_eq!(
            apply("quedamos a las siete y media de la tarde"),
            "quedamos a las 7:30 de la tarde"
        );
        assert_eq!(apply("A LAS TRES Y MEDIA"), "A LAS 3:30");
    }

    /// Without its article a time is a quantity or a sum, and stays words.
    #[test]
    fn a_clock_shape_without_its_article_stays_words() {
        for said in [
            "tres y media tazas",
            "echa tres y media tazas de harina",
            "cinco y veinte son muchos",
            "de las tres y media",
            "la una y la otra",
            "una y media",
            "las tres y media",
            "a las tres",
            "a las trece y media",
            "a las tres y cinco seis",
            "a las ocho menos cuarenta",
            "a las tres, y media",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn punctuation_separates_numbers_and_survives_conversion() {
        assert_eq!(apply("veinte, cinco"), "veinte, cinco");
        assert_eq!(apply("¿veinticinco?"), "¿25?");
        assert_eq!(apply("son treinta y cinco."), "son 35.");
    }

    /// A unit behind is enough evidence on its own, even for one word.
    #[test]
    fn a_unit_settles_a_lone_number() {
        assert_eq!(apply("subió cinco por ciento"), "subió 5\u{a0}%");
        assert_eq!(apply("el diez por cien"), "el 10\u{a0}%");
        assert_eq!(apply("cuesta veinte euros"), "cuesta 20\u{a0}\u{20ac}");
        assert_eq!(apply("unos diez kilómetros"), "unos 10\u{a0}km");
        assert_eq!(apply("tres kilos"), "3\u{a0}kg");
        assert_eq!(apply("hace treinta grados"), "hace 30\u{b0}");
        // "un por ciento" has no reading but 1 %.
        assert_eq!(apply("un por ciento"), "1\u{a0}%");
        // The "ciento" of "por ciento" is the unit, not a number of its own.
        assert_eq!(apply("doscientos por ciento"), "200\u{a0}%");
        assert_eq!(apply("por ciento"), "por ciento");
    }

    /// "$" is pesos in one country and dollars in the next, so the number
    /// converts and the currency stays a word.
    #[test]
    fn currencies_with_no_single_symbol_keep_their_word() {
        assert_eq!(apply("cinco dólares"), "5 dólares");
        assert_eq!(apply("cien pesos"), "100 pesos");
    }

    /// Through the whole pipeline, because tidy strips a space before "%" —
    /// the no-break space is what keeps the RAE spacing intact.
    #[test]
    fn units_survive_the_tidy_stage() {
        use crate::core::format::{apply as format, FormatOptions};
        let opts = FormatOptions {
            cleanup: true,
            spoken_punctuation: true,
            numbers: true,
            tidy: true,
        };
        assert_eq!(
            format("subió un cinco por ciento", opts, Some("es")),
            "Subió un 5\u{a0}%"
        );
        assert_eq!(
            format("son veinticinco euros coma gracias", opts, Some("es-MX")),
            "Son 25\u{a0}\u{20ac}, gracias"
        );
    }

    #[test]
    fn ordinary_sentences_are_left_alone() {
        for said in [
            "no sé qué decir",
            "uno nunca sabe",
            "me dio una mano",
            "el once de septiembre",
        ] {
            assert_eq!(apply(said), said);
        }
    }
}
