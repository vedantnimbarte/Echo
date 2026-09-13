//! Portuguese spelled-out numbers and the units that settle them, for Brazil
//! and Portugal alike.
//!
//! The English module's rule holds here unchanged: **never convert a lone
//! small word.** "Um dos melhores" must survive, and so must "os dois" and "mil
//! desculpas". A number is written as digits when the speaker gave more than
//! one word of it ("vinte e cinco", "dois mil e vinte e seis"), or when a unit
//! follows and settles the matter ("cinco por cento").
//!
//! **No single word converts on its own.** Portuguese has no fused 21–29 the
//! way Spanish does — "vinte e cinco" is three words — so the only one-word
//! numbers are teens and round words: "dezesseis" is "sixteen", "vinte" is
//! "twenty", "duzentos" is a round hundred. English leaves all of those alone,
//! and so does this.
//!
//! **"e" is the hard word.** It joins the parts of almost every Portuguese
//! number ("cento e vinte e cinco", "mil e quinhentos"), and it is also the
//! ordinary word "and". So it joins only where the grammar puts it: after a
//! tens before a unit, after a hundred before anything below a hundred, and
//! after "mil" or "milhão" before what follows. Everywhere else — "dois e
//! três", "entre vinte e trinta" — it is "and", and both numbers stay words.
//! "é" ("is") is a different word, and accent folding deliberately leaves it
//! alone so "trinta é pouco" never reads as "trinta e".
//!
//! **Articles are the trap.** "um" and "uma" are "a/an" far more often than 1,
//! so neither starts a number, with one exception: "um milhão" followed by more
//! of the number ("um milhão e duzentos mil"). A bare "um milhão" stays words.
//! After a tens or a hundred they are unambiguous ("vinte e um", "cento e
//! uma"), but not after "mil": "mil e uma noites" and "mil e um motivos" are a
//! title and an idiom for "countless", so "mil e um" stays as spoken.
//!
//! **Clock times need the preposition that makes them clock times.** "nove e
//! quinze" is as often two numbers as 9:15, and "três e meia" is three and a
//! half of something. "às" settles it — "às três e meia", "às nove e quinze" —
//! and so does "à uma", the singular. After those, an hour from one to twelve
//! followed by "e" and its minutes ("meia", "um quarto", or a number) is
//! written "às 3:30", "às 9:15". The preposition is checked with its accent:
//! "às" folds to "as", which is the article ("as três e meia xícaras"), so a
//! transcript that lost the accent keeps its words.
//!
//! **A run of number words converts whole or not at all.** "às oito trinta e
//! cinco" is a clock time without its "e", which this module does not read,
//! and converting the part it can parse would give "às oito 35". When the grammar stops while number
//! words carry on, the whole run is left as spoken.
//!
//! ponytail: clock times counted back to the hour ("cinco para as oito", "um
//! quarto para as nove", "às oito menos um quarto") are skipped: "para as oito"
//! puts the article, not the preposition, in front of the hour, and "dez para
//! as duas" reads just as well as "ten for the two". So are times without "às"
//! ("são três e meia", "das três e meia"), hours past twelve ("às catorze e
//! trinta"), and a time with no "e" ("às oito trinta"). Decimals are skipped because "vírgula" is already a spoken comma. Ordinals
//! ("vigésimo quinto") are skipped because dates use cardinals. "bilhão" and
//! "bilião" are skipped because Brazil means 10⁹ and Portugal 10¹², and "mil
//! milhões" is left as words rather than parsed. "meia" for six when reading
//! out digits is Brazilian phone-number speech, not a quantity, and is left
//! alone. A number converted wrongly is worse than one left as words, because
//! the reader cannot tell it was Echo that changed it.

use crate::core::format::words;

/// What a number word can combine with, which is the whole grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// "um", "uma": 1 only after "e" or before "milhão". Otherwise "a/an".
    Article,
    /// "dois" to "nove". Takes nothing after it.
    Unit,
    /// "dez" to "dezenove", which nothing joins.
    Whole,
    /// "vinte" to "noventa", which take "e" and a unit.
    Tens,
    /// "duzentos" to "novecentos", in either gender, which take "e" and
    /// anything below a hundred: "duzentos e cinquenta".
    Hundreds,
    /// "cem" is complete: "cem mil" is a scale, and "cem e vinte" is not
    /// Portuguese.
    Cem,
    /// "cento" never stands alone — it needs "e" and what comes after ("cento
    /// e vinte"). On its own it is almost always "por cento".
    Cento,
    /// "mil", "milhão", "milhões", which multiply what came before them.
    Scale,
}

/// Every number word, unaccented except for "é" (see [`fold`]). Whisper drops
/// accents often enough that "tres" and "três" must both match.
///
/// Both orthographies are here where they differ. Brazil writes "dezesseis",
/// "dezessete", "dezenove"; Portugal "dezasseis", "dezassete", "dezanove".
/// Brazil also accepts "quatorze" beside "catorze". None of the spellings
/// collides with anything, so there is no reason to make the region choose.
const NUMBER_WORDS: &[(&str, u64, Kind)] = &[
    ("um", 1, Kind::Article),
    ("uma", 1, Kind::Article),
    // The units that agree in gender: "dois carros", "duas casas".
    ("dois", 2, Kind::Unit),
    ("duas", 2, Kind::Unit),
    ("tres", 3, Kind::Unit),
    ("quatro", 4, Kind::Unit),
    ("cinco", 5, Kind::Unit),
    ("seis", 6, Kind::Unit),
    ("sete", 7, Kind::Unit),
    ("oito", 8, Kind::Unit),
    ("nove", 9, Kind::Unit),
    ("dez", 10, Kind::Whole),
    ("onze", 11, Kind::Whole),
    ("doze", 12, Kind::Whole),
    ("treze", 13, Kind::Whole),
    ("catorze", 14, Kind::Whole),
    ("quatorze", 14, Kind::Whole),
    ("quinze", 15, Kind::Whole),
    ("dezesseis", 16, Kind::Whole),
    ("dezasseis", 16, Kind::Whole),
    ("dezessete", 17, Kind::Whole),
    ("dezassete", 17, Kind::Whole),
    ("dezoito", 18, Kind::Whole),
    ("dezenove", 19, Kind::Whole),
    ("dezanove", 19, Kind::Whole),
    ("vinte", 20, Kind::Tens),
    ("trinta", 30, Kind::Tens),
    ("quarenta", 40, Kind::Tens),
    // "cinqüenta" before the 2009 spelling reform; the fold covers it.
    ("cinquenta", 50, Kind::Tens),
    ("sessenta", 60, Kind::Tens),
    ("setenta", 70, Kind::Tens),
    ("oitenta", 80, Kind::Tens),
    ("noventa", 90, Kind::Tens),
    ("cem", 100, Kind::Cem),
    ("cento", 100, Kind::Cento),
    ("duzentos", 200, Kind::Hundreds),
    ("duzentas", 200, Kind::Hundreds),
    ("trezentos", 300, Kind::Hundreds),
    ("trezentas", 300, Kind::Hundreds),
    ("quatrocentos", 400, Kind::Hundreds),
    ("quatrocentas", 400, Kind::Hundreds),
    ("quinhentos", 500, Kind::Hundreds),
    ("quinhentas", 500, Kind::Hundreds),
    ("seiscentos", 600, Kind::Hundreds),
    ("seiscentas", 600, Kind::Hundreds),
    ("setecentos", 700, Kind::Hundreds),
    ("setecentas", 700, Kind::Hundreds),
    ("oitocentos", 800, Kind::Hundreds),
    ("oitocentas", 800, Kind::Hundreds),
    ("novecentos", 900, Kind::Hundreds),
    ("novecentas", 900, Kind::Hundreds),
    ("mil", 1_000, Kind::Scale),
    ("milhao", 1_000_000, Kind::Scale),
    ("milhoes", 1_000_000, Kind::Scale),
];

/// How a unit is written once the number in front of it is digits.
enum Written {
    /// Appended to the digits. "%" is attached, as Brazilian and Portuguese
    /// writing both do in practice. The spaced ones use a no-break space, so a
    /// line never breaks between figure and symbol.
    After(&'static str),
    /// Written in front of the digits. "R$ 25" takes a space, and it has to be
    /// a no-break one: the tidy stage strips an ordinary space after "$".
    Before(&'static str),
    /// The number converts and the unit word stays as spoken. "$" alone means
    /// too many currencies to pick one for "dólares".
    KeepWord,
}

/// Units that follow a number, as the keys they are spoken as.
///
/// `article` marks the one unit an article may take. "um quilo" and "um real"
/// are "a kilo" and "a real" as often as they are 1; "um por cento" is only
/// ever 1%.
const UNIT_WORDS: &[(&[&str], Written, bool)] = &[
    (&["por", "cento"], Written::After("%"), true),
    // "reais" is also the plural of "real" as in "problemas reais", but only
    // directly after a number is it read as money.
    (&["reais"], Written::Before("R$\u{a0}"), false),
    // Portugal sets the euro after the figure with a space: "25 €".
    (&["euros"], Written::After("\u{a0}\u{20ac}"), false),
    (&["dolares"], Written::KeepWord, false),
    (&["graus"], Written::After("\u{b0}"), false),
    // "quilômetros" in Brazil, "quilómetros" in Portugal; both fold to this.
    (&["quilometros"], Written::After("\u{a0}km"), false),
    (&["quilos"], Written::After("\u{a0}kg"), false),
    (&["quilogramas"], Written::After("\u{a0}kg"), false),
];

/// Convert spelled-out Portuguese numbers and their units to written forms.
pub fn apply(text: &str) -> String {
    let words = words(text);
    let keys: Vec<String> = words.iter().map(|w| key(w)).collect();

    // A number never runs across punctuation: "vinte, cinco" is a list, not
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
        // The clock looks back at the preposition before the hour, so it is
        // handed everything up to the end of the clause rather than the clause
        // alone.
        if let Some((len, written)) = clock(&words[..reach[i]], &keys[..reach[i]], i) {
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
    (len > 1).then(|| (len, value.to_string()))
}

/// A clock time whose hour is at `i`, returning how many words it spans and
/// how it is written. See the module docs for the shapes and why each needs its
/// preposition.
fn clock(words: &[&str], keys: &[String], i: usize) -> Option<(usize, String)> {
    // Unfolded, because the accent is the whole difference between "às" and
    // the article "as".
    let before = i.checked_sub(1).map(|p| {
        words[p]
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase()
    });
    let hour = match (before.as_deref(), number(keys, i)?) {
        // "uma" alone is "a/an"; only "à uma" is one o'clock.
        (Some("à"), (1, Kind::Article)) if keys[i] == "uma" => 1,
        (Some("às"), (hour @ 2..=12, _)) => hour,
        _ => return None,
    };
    if keys.get(i + 1).map(String::as_str) != Some("e") {
        return None;
    }
    let (len, minutes) = match (
        keys.get(i + 2).map(String::as_str),
        keys.get(i + 3).map(String::as_str),
    ) {
        (Some("meia"), _) => (3, 30),
        (Some("um"), Some("quarto")) => (4, 15),
        _ => {
            let (len, minutes) = below_hundred(keys, i + 2)?;
            (2 + len, minutes)
        }
    };
    // "e quinze dois" is not a time the grammar reads: leave the run whole, as
    // everywhere else in this module.
    if number(keys, i + len).is_some() || !(1..60).contains(&minutes) {
        return None;
    }
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
/// A run is number words back to back, plus "e" wherever a number word
/// follows it. Articles do not extend a run on their own — "custa vinte e
/// cinco um quilo" is 25 and "a kilo" — but after "e" they do.
fn run_end(clause: &[String]) -> usize {
    let mut p = 1;
    while p < clause.len() {
        if number(clause, p).is_some_and(|(_, kind)| kind != Kind::Article) {
            p += 1;
        } else if clause[p] == "e" && number(clause, p + 1).is_some() {
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

/// A number below a hundred at `p`: "cinco", "quinze", "vinte e seis". Never
/// an article on its own; see [`and_below_hundred`] for where one may count.
fn below_hundred(clause: &[String], p: usize) -> Option<(usize, u64)> {
    let (value, kind) = number(clause, p)?;
    match kind {
        Kind::Unit | Kind::Whole => Some((1, value)),
        // "e" joins only when a unit really follows it — "vinte e um"
        // included. Otherwise it is the word "and" and the tens stands alone.
        Kind::Tens => match (clause.get(p + 1).map(String::as_str), number(clause, p + 2)) {
            (Some("e"), Some((unit, Kind::Unit | Kind::Article))) => Some((3, value + unit)),
            _ => Some((1, value)),
        },
        _ => None,
    }
}

/// "e" followed by a number below a hundred, as a hundred takes it: "e
/// quarenta", "e um". The article counts here because nothing but a number
/// comes between "cento e" and a noun.
fn and_below_hundred(clause: &[String], p: usize) -> Option<(usize, u64)> {
    if clause.get(p).map(String::as_str) != Some("e") {
        return None;
    }
    match number(clause, p + 1) {
        Some((value, Kind::Article)) => Some((2, value)),
        _ => below_hundred(clause, p + 1).map(|(len, value)| (1 + len, value)),
    }
}

/// A number below a thousand at `p`: "cem", "cento e vinte", "trezentos e
/// quarenta e dois", or anything [`below_hundred`] accepts. An article never
/// starts one.
fn group(clause: &[String], p: usize) -> Option<(usize, u64)> {
    let (value, kind) = number(clause, p)?;
    match kind {
        Kind::Hundreds => Some(match and_below_hundred(clause, p + 1) {
            Some((len, rest)) => (1 + len, value + rest),
            None => (1, value),
        }),
        Kind::Cento => and_below_hundred(clause, p + 1).map(|(len, rest)| (1 + len, value + rest)),
        Kind::Cem => Some((1, value)),
        _ => below_hundred(clause, p),
    }
}

/// Match a whole cardinal at the start of the clause, returning how many words
/// it spans and its value.
///
/// A cardinal is groups below a thousand separated by scale words, each scale
/// smaller than the last. After a scale, what follows may be joined with "e"
/// or not: "mil e quinhentos" and "mil duzentos e trinta" are both correct, so
/// both are accepted.
fn cardinal(clause: &[String]) -> Option<(usize, u64)> {
    let mut p = 0;
    let mut total = 0u64;
    let mut last_scale = u64::MAX;

    // "um milhão" is the one place an article starts a number, and only if
    // more of the number follows — see the module docs.
    let article_million = number(clause, 0).is_some_and(|(_, kind)| kind == Kind::Article)
        && scale(clause, 1) == Some(1_000_000);

    loop {
        // After a scale, an "e" belongs to the number only if a group follows
        // it. [`group`] never starts with an article, which is what keeps
        // "mil e uma noites" out.
        let at =
            if p > 0 && clause.get(p).is_some_and(|w| w == "e") && group(clause, p + 1).is_some() {
                p + 1
            } else {
                p
            };
        let found = if p == 0 && article_million {
            Some((1, 1))
        } else {
            group(clause, at)
        };
        let group_len = found.map_or(0, |(len, _)| len);

        match scale(clause, at + group_len).filter(|s| *s < last_scale) {
            Some(scale) => {
                let multiplier = match found {
                    Some((_, value)) => value,
                    // "mil" on its own means a thousand. "milhão" does not.
                    None if scale == 1_000 => 1,
                    None => break,
                };
                total += multiplier * scale;
                last_scale = scale;
                p = at + group_len + 1;
            }
            None => {
                if let Some((len, value)) = found {
                    total += value;
                    p = at + len;
                }
                break;
            }
        }
    }

    // A bare "um milhão" ends right after the scale word.
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
            Written::Before(symbol) => (phrase.len(), format!("{symbol}{value}"), *article),
            Written::KeepWord => (0, value.to_string(), *article),
        })
    })
}

/// Whether two neighbouring words may belong to one number: nothing but a
/// space between them. "vinte, cinco" and "vinte (cinco" are two numbers.
fn glued(left: &str, right: &str) -> bool {
    left.chars().last().is_some_and(char::is_alphanumeric)
        && right.chars().next().is_some_and(char::is_alphanumeric)
}

/// Put the punctuation that surrounded the spoken words back around what
/// replaced them, so "vinte e cinco." becomes "25." rather than losing its
/// full stop with the words.
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
/// only, so a number inside «guillemets» — which Portugal uses — would never
/// match.
fn key(word: &str) -> String {
    fold(
        &word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase(),
    )
}

/// Strip the accents Portuguese number and unit words carry, so a transcript
/// that lost them ("tres", "milhoes") matches the same entry.
///
/// **"é" is not folded.** It is the verb "is", and folding it to "e" would make
/// it the conjunction that joins numbers: "trinta é pouco" would be read as the
/// start of "trinta e…". No number or unit word here carries "é", so nothing is
/// lost by leaving it.
fn fold(word: &str) -> String {
    word.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
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
        assert_eq!(apply("vinte e cinco pessoas"), "25 pessoas");
        assert_eq!(apply("trezentos e quarenta"), "340");
        assert_eq!(apply("cento e vinte e cinco"), "125");
        assert_eq!(apply("mil e quinhentos"), "1500");
        assert_eq!(apply("em dois mil e vinte e seis"), "em 2026");
        assert_eq!(apply("mil novecentos e noventa e nove"), "1999");
        assert_eq!(apply("duzentos e cinquenta mil"), "250000");
        assert_eq!(apply("três milhões"), "3000000");
        assert_eq!(apply("cem mil"), "100000");
    }

    /// Portuguese accepts a scale with or without "e" before what follows it.
    #[test]
    fn e_after_a_scale_is_optional() {
        assert_eq!(apply("mil duzentos e trinta"), "1230");
        assert_eq!(apply("mil e duzentos"), "1200");
    }

    /// The rule the whole module hangs on.
    #[test]
    fn a_lone_number_word_stays_a_word() {
        assert_eq!(apply("um dos melhores"), "um dos melhores");
        assert_eq!(apply("os dois foram"), "os dois foram");
        assert_eq!(apply("cinco"), "cinco");
        assert_eq!(apply("dezesseis"), "dezesseis");
        assert_eq!(apply("vinte"), "vinte");
        assert_eq!(apply("mil desculpas"), "mil desculpas");
        assert_eq!(apply("duzentos anos"), "duzentos anos");
    }

    /// Brazil and Portugal spell the teens differently, and both are numbers.
    #[test]
    fn brazilian_and_european_spellings_both_convert() {
        assert_eq!(apply("mil e dezesseis"), "1016");
        assert_eq!(apply("mil e dezasseis"), "1016");
        assert_eq!(apply("dezenove mil"), "19000");
        assert_eq!(apply("dezanove mil"), "19000");
        assert_eq!(apply("quatorze mil"), "14000");
        assert_eq!(apply("catorze mil"), "14000");
    }

    /// Whisper drops accents; the number must not depend on whether it did.
    #[test]
    fn accents_are_optional() {
        assert_eq!(apply("três mil"), "3000");
        assert_eq!(apply("tres mil"), "3000");
        assert_eq!(apply("dois milhoes"), "2000000");
        assert_eq!(apply("cinqüenta e dois"), "52");
    }

    #[test]
    fn gender_forms_count_the_same() {
        assert_eq!(apply("duzentas e uma pessoas"), "201 pessoas");
        assert_eq!(apply("vinte e duas"), "22");
        assert_eq!(apply("duas mil casas"), "2000 casas");
    }

    /// "um" and "uma" are "a/an". They are 1 only after "e" inside a number,
    /// or starting a million that carries on.
    #[test]
    fn articles_are_not_ones() {
        assert_eq!(apply("um carro"), "um carro");
        assert_eq!(apply("uma vez"), "uma vez");
        assert_eq!(apply("um quilo de arroz"), "um quilo de arroz");
        assert_eq!(apply("um real"), "um real");
        assert_eq!(apply("um mil"), "um mil");
        assert_eq!(apply("vinte e um dias"), "21 dias");
        assert_eq!(apply("cento e um"), "101");
    }

    #[test]
    fn um_milhao_converts_only_when_more_of_the_number_follows() {
        assert_eq!(apply("um milhão de pessoas"), "um milhão de pessoas");
        assert_eq!(apply("um milhão"), "um milhão");
        assert_eq!(apply("um milhão e duzentos mil"), "1200000");
    }

    /// "e" is "and" everywhere the number grammar does not put it.
    #[test]
    fn e_between_other_numbers_is_the_word_and() {
        assert_eq!(apply("dois e três"), "dois e três");
        assert_eq!(apply("entre vinte e trinta"), "entre vinte e trinta");
        assert_eq!(apply("entre cem e duzentos"), "entre cem e duzentos");
        assert_eq!(apply("vinte e poucos anos"), "vinte e poucos anos");
        // A title and an idiom for "countless", not 1001.
        assert_eq!(apply("as mil e uma noites"), "as mil e uma noites");
        assert_eq!(apply("mil e um motivos"), "mil e um motivos");
    }

    /// "é" is "is", and must never be mistaken for the "e" that joins numbers.
    #[test]
    fn e_with_an_accent_is_the_verb() {
        assert_eq!(apply("trinta é pouco"), "trinta é pouco");
        assert_eq!(apply("vinte é cinco"), "vinte é cinco");
    }

    /// A clock time is a run the grammar cannot read. Converting the part it
    /// can would leave a figure half-rewritten.
    #[test]
    fn a_run_the_grammar_cannot_read_is_left_whole() {
        assert_eq!(apply("às oito trinta e cinco"), "às oito trinta e cinco");
        assert_eq!(apply("três e meia"), "três e meia");
        // "mil e dois" is 1002, but not when "mil" follows.
        assert_eq!(apply("entre mil e dois mil"), "entre mil e dois mil");
        assert_eq!(apply("dois mil milhões"), "dois mil milhões");
        // Without its "e" this is not Portuguese, so it is not guessed at.
        assert_eq!(apply("duzentos cinquenta"), "duzentos cinquenta");
    }

    #[test]
    fn clock_times_after_their_preposition_use_a_colon() {
        assert_eq!(apply("às três e meia"), "às 3:30");
        assert_eq!(apply("às nove e quinze"), "às 9:15");
        assert_eq!(apply("às duas e um quarto"), "às 2:15");
        assert_eq!(apply("às oito e trinta e cinco"), "às 8:35");
        assert_eq!(apply("à uma e meia."), "à 1:30.");
        assert_eq!(apply("Às doze e dez"), "Às 12:10");
        assert_eq!(
            apply("chego às sete e meia da noite"),
            "chego às 7:30 da noite"
        );
    }

    /// Without "às" a time is a quantity or two numbers, and stays words.
    #[test]
    fn a_clock_shape_without_its_preposition_stays_words() {
        for said in [
            "três e meia xícaras",
            "nove e quinze",
            "as três e meia xícaras",
            "as tres e meia",
            "são três e meia",
            "uma e meia",
            "às três",
            "às catorze e trinta",
            "às nove e quinze dois",
            "às três, e meia",
            "cinco para as oito",
        ] {
            assert_eq!(apply(said), said);
        }
    }

    #[test]
    fn punctuation_separates_numbers_and_survives_conversion() {
        assert_eq!(apply("vinte, cinco"), "vinte, cinco");
        assert_eq!(apply("são vinte e cinco."), "são 25.");
        assert_eq!(apply("«vinte e cinco»"), "«25»");
    }

    /// A unit behind is enough evidence on its own, even for one word.
    #[test]
    fn a_unit_settles_a_lone_number() {
        assert_eq!(apply("subiu cinco por cento"), "subiu 5%");
        assert_eq!(apply("custa vinte reais"), "custa R$\u{a0}20");
        assert_eq!(apply("dez euros"), "10\u{a0}\u{20ac}");
        assert_eq!(apply("trinta graus"), "30\u{b0}");
        assert_eq!(apply("dez quilômetros"), "10\u{a0}km");
        assert_eq!(apply("dez quilómetros"), "10\u{a0}km");
        assert_eq!(apply("três quilos"), "3\u{a0}kg");
        // "um por cento" has no reading but 1%.
        assert_eq!(apply("um por cento"), "1%");
        // The "cento" of "por cento" is the unit, not a number of its own.
        assert_eq!(apply("cem por cento"), "100%");
        assert_eq!(apply("por cento"), "por cento");
    }

    /// "$" alone is too many currencies, so the number converts and the word
    /// stays.
    #[test]
    fn dollars_keep_their_word() {
        assert_eq!(apply("cinco dólares"), "5 dólares");
    }

    /// Through the whole pipeline, because tidy strips a space after "$" — the
    /// no-break space is what keeps "R$ 25" intact.
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
            format("custa vinte e cinco reais", opts, Some("pt-BR")),
            "Custa R$\u{a0}25"
        );
        assert_eq!(
            format("subiu dez por cento virgula ontem", opts, Some("pt")),
            "Subiu 10%, ontem"
        );
        assert_eq!(
            format("são vinte euros ponto", opts, Some("pt-PT")),
            "São 20\u{a0}\u{20ac}."
        );
    }

    #[test]
    fn ordinary_sentences_are_left_alone() {
        for said in [
            "é um prazer",
            "cada um sabe de si",
            "e então fomos embora",
            "problemas reais",
        ] {
            assert_eq!(apply(said), said);
        }
    }
}
