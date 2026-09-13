//! Removing what you said but did not mean to write.
//!
//! Speech has hesitations in it. "Um", a word said twice while the sentence
//! catches up, a false start abandoned mid-phrase — none of that is meant for
//! the page, and a faithful transcript full of it reads as though the writer
//! were unwell.
//!
//! **This stage is in tension with the rest of Echo, and the tension is
//! deliberate.** Everything else here works to reproduce what was said. This
//! throws some of it away. So the rules are narrow, and each one only fires
//! where the alternative reading is not language anybody writes:
//!
//! - **Fillers** are removed only from a fixed list of non-words. "Um" is never
//!   a word someone meant. "Like" and "actually" are, so they are not on it,
//!   however often they are used as filler.
//! - **Doubled words** collapse only for function words, and only for the ones
//!   that cannot legitimately double. "The the" is a stutter; "that that" is
//!   grammar, so it is not on the list.
//!
//! **What rules cannot do is self-correction.** "Send it Tuesday, no, Wednesday"
//! needs to become "Send it Wednesday", and deciding how far back to delete is a
//! judgement about meaning, not a pattern. A rule aggressive enough to catch it
//! would eat clauses people meant to keep. That half is left to the optional
//! local-LLM pass in [`crate::core::command`], which is off by default because
//! it rewrites your words.
//!
//! **Both lists are facts about one language**, so each language has its own
//! pair and a language without one is left alone ([`covers`]). The lists are
//! not translations of the English ones. "Um" is Portuguese for "one", "em" is
//! Portuguese for "in", "eh" is German for "anyway" — a shared filler list would
//! delete real words in exactly the languages it claims to help. Every word
//! that was considered and left off is named next to its list, with the reason,
//! because the next person to add "bah" to French should see why it isn't
//! there.
//!
//! Two rules hold in every language, because they are about speech rather than
//! grammar:
//!
//! - A filler written with `?` or `!` is kept. Nobody hesitates with a question
//!   mark: "eh?" is a tag question ("cold, eh?", "no lo hagas, eh?"), and "hm!"
//!   is a reaction. Dropping it would also drop the mark that ends the sentence.
//! - A repeat across punctuation is not a stutter. "I did it, it works" is two
//!   clauses that happen to meet at the same word; the comma is the decoder
//!   saying so.

use super::{key, words};

/// One language's cleanup rules.
struct Rules {
    /// Sounds that are never words in this language. Removing one cannot
    /// destroy meaning, which is the entire criterion for being on the list.
    fillers: &'static [&'static str],
    /// Function words whose immediate repeat is always a stutter. Repeating a
    /// *content* word is often deliberate ("very very good", "no no no"), and
    /// collapsing that changes emphasis the speaker chose, so only grammar words
    /// are candidates — and of those, only the ones that cannot double in real
    /// sentences.
    stutters: &'static [&'static str],
}

/// English.
///
/// Fillers leave out "like", "so", "right", "well", "actually", "basically" and
/// "literally": used as filler constantly, and also ordinary words; no rule can
/// tell which without understanding the sentence.
///
/// Stutters leave out "that" ("the rule that that covers"), "had" ("had had
/// enough"), "is" ("what it is is complicated"), "was" and "will", which all
/// double in grammatical English. Also "in", "on" and "by", which end phrasal
/// verbs and then start the next phrase ("log in in the corner", "carry on on
/// Monday", "stop by by five"); "so" ("so-so"); "there" ("there, there"); and
/// "you" ("I love you you know", when the decoder leaves out the comma).
const EN: Rules = Rules {
    fillers: &["um", "uh", "erm", "uhh", "umm", "hmm", "mmm", "mm", "eh"],
    stutters: &[
        "the", "a", "an", "and", "or", "but", "to", "of", "at", "for", "with", "from", "as", "if",
        "it", "i", "we", "he", "she", "they", "this", "then",
    ],
};

/// Spanish.
///
/// Fillers leave out "este", "pues", "bueno", "o sea", "vale", "digamos" and
/// "¿no?" (all real words), "ah", "oh" and "uh" (interjections of surprise and
/// disappointment the RAE lists, and people write them). "eh" is on the list
/// although the RAE lists it too: as a call or a warning it is written "¡eh!"
/// or "¿eh?", and the question/exclamation rule keeps those — bare, it is the
/// hesitation sound Whisper writes for Spanish.
///
/// Stutters leave out "que" ("lo que qué", once Whisper drops the accent), "es"
/// ("lo que es es"), "si" ("sí, sí" without its accent), "a" ("voy a A
/// Coruña"), "de" ("el combate de De la Hoya") and "para", which is also "stops"
/// ("el autobús para para recoger").
const ES: Rules = Rules {
    fillers: &["eh", "ehh", "ehm", "em", "mm", "mmm", "hmm"],
    stutters: &[
        "el", "la", "los", "las", "un", "una", "y", "o", "en", "con", "por", "yo", "lo",
    ],
};

/// French.
///
/// Fillers leave out "bah" and "ben" ("bah, tant pis", "ben oui" — dismissal
/// and agreement, both meant), "hein" (a tag question), "eh" ("eh bien" is
/// "well"), "ah", "oh", and the filler words "genre", "quoi", "voilà", "enfin",
/// "du coup" and "en fait", which are also just words.
///
/// Stutters leave out "nous" and "vous" ("nous nous sommes vus" is the
/// ordinary reflexive), "elle" ("elle, elle travaille" is emphasis, even when
/// the comma is missing), "si" ("si, si" is an emphatic yes), "de" ("le
/// discours de De Gaulle") and "un", which is also the digit one ("zéro six un
/// un").
const FR: Rules = Rules {
    fillers: &["euh", "heu", "hum", "hmm", "mmh", "mm", "mmm"],
    stutters: &[
        "le", "la", "les", "une", "et", "ou", "à", "en", "dans", "pour", "avec", "sur", "je", "tu",
        "il", "on", "ils", "ce",
    ],
};

/// German.
///
/// Fillers leave out "eh" (colloquial "anyway": "das ist eh egal"), "mhm" (yes),
/// "na", "naja", "ne", "halt", "also", "quasi", "sozusagen" (words), and "ah"
/// and "oh".
///
/// **Stutters are almost empty, on purpose.** German doubles its most common
/// stutter words legitimately:
///
/// - Every article is also a relative pronoun: "die Frau, die die Zeitung
///   liest", "das Buch, das das Kind will". "Die die" is the stutter people
///   make most and it is grammar just as often.
/// - Pronouns double at the end of a verb-final clause: "wenn ich ich selbst
///   bin", "dass sie Sie sieht". The same goes for "einer", "einen", "einem" and
///   "eine" used as "one" ("das kann einen einen Tag kosten").
/// - Separable prefixes meet their own preposition: "kommst du mit mit dem
///   Auto?", "hör auf auf mich zu warten", and "an", "zu" likewise.
/// - "von von Weizsäcker", "aber aber!" ("now, now") and "und und und" ("and
///   so on") are idioms.
///
/// - "ein" is a separable prefix too, and spoken German moves phrases past the
///   verb freely: "er schläft ein ein bisschen später".
///
/// What is left cannot double: "in", "im" and "für" are neither pronouns nor
/// verb prefixes.
const DE: Rules = Rules {
    fillers: &["äh", "ähm", "ähh", "öh", "öhm", "ehm", "hm", "hmm", "mmm"],
    stutters: &["in", "im", "für"],
};

/// Italian.
///
/// Fillers leave out "eh" ("eh, lo so" and "bello, eh?" carry resignation and
/// agreement — it is Italian's "well"), "beh" (also "well"), "boh" ("no idea"),
/// "uh" (pain or surprise), "ah", "oh", and the filler words "cioè", "tipo",
/// "allora", "insomma", "diciamo", "praticamente" and "ecco".
///
/// Stutters leave out "che" ("che che cosa" is heard, and "che" is too many
/// things at once), "di" ("Di Maio"), "da" ("dà da mangiare" once the accent is
/// dropped), "e" ("è e non sarà", likewise), "a", "si" ("sì sì"), "su" ("su,
/// su!" is "come on") and "uno", which is the digit one ("uno uno due").
const IT: Rules = Rules {
    fillers: &["ehm", "uhm", "mh", "hmm", "mm", "mmm"],
    stutters: &[
        "il", "lo", "la", "i", "gli", "le", "un", "una", "in", "con", "per", "io",
    ],
};

/// Portuguese.
///
/// Fillers leave out "é" ("is"), "né" ("isn't it"), "tipo", "então", "assim",
/// "bom", "pois", "tá", "eh" ("eh pá" in Portugal, a greeting and a cheer),
/// "ah", "oh", and "hã" and "ahn", which are "huh?" and often stand as a
/// question without the mark. "um" and "em" are real words here, so the English
/// "um" and the Spanish "em" are exactly what this list must not contain.
///
/// Stutters leave out "que" ("o que que é isso?" is ordinary Brazilian speech),
/// "a" (article and preposition meet when a speaker skips the crase), "e" ("é e
/// não é", accent dropped), "para" ("o ônibus para para ali"), "de" (surnames,
/// as in Spanish) and "um", which is the digit one.
const PT: Rules = Rules {
    fillers: &["ãh", "ãhn", "ehm", "uhm", "hum", "hmm", "mm", "mmm"],
    stutters: &["o", "os", "as", "uma", "em", "no", "na", "com", "por", "eu"],
};

/// Dutch.
///
/// Fillers leave out "hè" (a tag question), "nou" ("well"), "tja" (resignation),
/// "och", "ah", "oh", and "dus", "eigenlijk", "gewoon" and "zeg maar", which are
/// words.
///
/// Stutters leave out a lot, because Dutch doubles like German:
///
/// - "dat dat" and "die die" are conjunction or relative pronoun meeting a
///   demonstrative: "ik weet dat dat waar is", "de man die die auto kocht".
/// - "het het" ("ik vind het het beste"), "je je" ("heb je je sleutels?") and
///   "ze ze" ("hebben ze ze gezien?") are a pronoun meeting an article or its
///   own possessive, and all three are everyday sentences.
/// - Separable prefixes meet their preposition: "stap je in in Utrecht?", and
///   "op op", "aan aan", "voor voor", "om om" likewise.
/// - "van Van Gogh", and "een", which is also the digit one.
const NL: Rules = Rules {
    fillers: &["eh", "uh", "uhm", "ehm", "euh", "hmm", "mm", "mmm"],
    stutters: &["de", "en", "met", "ik", "hij", "we", "te"],
};

/// Language code → its rules. Matched on the leading subtag, so "en-GB" and
/// "pt-BR" find theirs.
const RULES: &[(&str, Rules)] = &[
    ("en", EN),
    ("de", DE),
    ("es", ES),
    ("fr", FR),
    ("it", IT),
    ("nl", NL),
    ("pt", PT),
];

/// Language codes cleanup has rules for, so the settings screen can say which
/// languages this stage applies to.
pub fn supported_languages() -> Vec<&'static str> {
    RULES.iter().map(|(code, _)| *code).collect()
}

fn rules_for(language: Option<&str>) -> Option<&'static Rules> {
    let raw = language.unwrap_or("en").to_lowercase();
    let code = raw.split(['-', '_']).next().unwrap_or("en");
    RULES.iter().find(|(c, _)| *c == code).map(|(_, r)| r)
}

/// Whether the cleanup rules exist for `language`.
///
/// Guessing a filler list for a language without one is how a rule starts
/// deleting words that were meant.
pub fn covers(language: Option<&str>) -> bool {
    rules_for(language).is_some()
}

/// Strip hesitations and stutters from a transcript using `language`'s rules,
/// or return it untouched when there are none.
pub fn apply(text: &str, language: Option<&str>) -> String {
    let Some(rules) = rules_for(language) else {
        return text.to_string();
    };
    let words = words(text);
    let mut out: Vec<&str> = Vec::with_capacity(words.len());

    for word in words {
        let k = key(word);

        // A filler carries no meaning, so it goes — unless it was asked or
        // exclaimed, which a hesitation never is.
        if rules.fillers.contains(&k.as_str()) && !word.contains(['?', '!']) {
            continue;
        }

        // A word repeated immediately after itself, where the repeat cannot be
        // grammar. Compared on the key so "the The" still collapses, but not
        // across punctuation: "it, it" is two clauses, not a stutter.
        if out.last().is_some_and(|prev| {
            key(prev) == k
                && !prev.ends_with([',', '.', ';', ':', '?', '!'])
                && rules.stutters.contains(&k.as_str())
        }) {
            continue;
        }

        out.push(word);
    }

    if out.is_empty() {
        // Everything was filler. Better to hand back what was said than an
        // empty string the user cannot tell from a broken microphone.
        return text.trim().to_string();
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en(text: &str) -> String {
        apply(text, Some("en"))
    }

    #[test]
    fn hesitations_are_removed() {
        assert_eq!(en("um so I was uh thinking"), "so I was thinking");
        assert_eq!(en("Um, hello"), "hello");
        assert_eq!(en("well hmm maybe"), "well maybe");
    }

    /// The rule that keeps this honest. These are filler constantly, and they
    /// are also words — no rule can tell which without understanding the
    /// sentence, so they stay.
    #[test]
    fn words_that_are_only_sometimes_filler_are_kept() {
        for said in [
            "I like this actually",
            "so basically it works",
            "well that is literally right",
        ] {
            assert_eq!(en(said), said);
        }
    }

    #[test]
    fn stuttered_function_words_collapse() {
        assert_eq!(en("the the cat sat"), "the cat sat");
        assert_eq!(en("I I think so"), "I think so");
        assert_eq!(en("go to to the shop"), "go to the shop");
    }

    /// English doubles some words legitimately, and collapsing those is a
    /// grammar error the user then has to fix by hand.
    #[test]
    fn legitimate_doubles_survive() {
        for said in [
            "I had had enough",
            "the rule that that covers it",
            "what it is is complicated",
            "log in in the corner",
            "carry on on Monday",
            "it was so so",
            "there there",
        ] {
            assert_eq!(en(said), said);
        }
    }

    /// Repeating a content word is emphasis the speaker chose.
    #[test]
    fn repeated_content_words_are_emphasis_not_stutter() {
        assert_eq!(en("very very good"), "very very good");
        assert_eq!(en("no no no"), "no no no");
    }

    /// An utterance that was nothing but filler still happened. Returning an
    /// empty string would be indistinguishable from a failed recording.
    #[test]
    fn an_utterance_of_pure_filler_is_not_erased() {
        assert_eq!(en("um"), "um");
        assert_eq!(en("uh um uh"), "uh um uh");
        assert_eq!(apply("äh ähm", Some("de")), "äh ähm");
    }

    /// Only the immediate repeat collapses; the same word twice in a sentence
    /// is ordinary language.
    #[test]
    fn a_word_used_twice_in_a_sentence_is_left_alone() {
        assert_eq!(en("the cat sat on the mat"), "the cat sat on the mat");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        let said = "the quick brown fox jumped over the lazy dog";
        assert_eq!(en(said), said);
    }

    /// Two clauses can meet at the same word. The comma is the evidence.
    #[test]
    fn a_repeat_across_punctuation_is_two_clauses() {
        assert_eq!(en("I did it, it works"), "I did it, it works");
        assert_eq!(
            apply("Ich war in, in Berlin", Some("de")),
            "Ich war in, in Berlin"
        );
    }

    /// A filler asked or exclaimed is a tag or a reaction, and it carries the
    /// mark that ends the sentence.
    #[test]
    fn a_filler_with_a_question_or_exclamation_mark_is_kept() {
        assert_eq!(en("cold out, eh?"), "cold out, eh?");
        assert_eq!(apply("no lo hagas, eh?", Some("es")), "no lo hagas, eh?");
        assert_eq!(apply("Hm! Gut", Some("de")), "Hm! Gut");
    }

    #[test]
    fn spanish() {
        let es = |t| apply(t, Some("es-MX"));
        assert_eq!(es("eh creo que ehm sí"), "creo que sí");
        assert_eq!(es("la la casa y y el perro"), "la casa y el perro");
        for kept in [
            "este libro es bueno",
            "pues bueno o sea vale",
            "lo que es es complicado",
            "voy a A Coruña",
            "el combate de De la Hoya",
            "el autobús para para recoger",
            "sí si claro",
        ] {
            assert_eq!(es(kept), kept);
        }
    }

    #[test]
    fn french() {
        let fr = |t| apply(t, Some("fr"));
        assert_eq!(fr("euh je je pense que heu oui"), "je pense que oui");
        assert_eq!(fr("le le chat"), "le chat");
        for kept in [
            "nous nous sommes vus",
            "vous vous trompez",
            "elle elle travaille",
            "bah tant pis",
            "ben oui hein",
            "eh bien voilà",
            "si si je te jure",
            "le discours de De Gaulle",
            "zéro six un un",
        ] {
            assert_eq!(fr(kept), kept);
        }
    }

    #[test]
    fn german() {
        let de = |t| apply(t, Some("de"));
        assert_eq!(de("äh ich habe ähm öhm für für dich"), "ich habe für dich");
        assert_eq!(de("in in Berlin"), "in Berlin");
        for kept in [
            "die Frau die die Zeitung liest",
            "das Buch das das Kind will",
            "der Mann der der Frau hilft",
            "wenn ich ich selbst bin",
            "dass sie Sie sieht",
            "das kann einen einen Tag kosten",
            "kommst du mit mit dem Auto",
            "er schläft ein ein bisschen später",
            "die Rede von von Weizsäcker",
            "und und und",
            "aber aber",
            "das ist eh egal",
            "also halt naja",
        ] {
            assert_eq!(de(kept), kept);
        }
    }

    #[test]
    fn italian() {
        let it = |t| apply(t, Some("it"));
        assert_eq!(it("ehm io io penso uhm di sì"), "io penso di sì");
        assert_eq!(it("il il gatto"), "il gatto");
        for kept in [
            "eh lo so",
            "beh boh",
            "il partito di Di Maio",
            "da da mangiare",
            "su su andiamo",
            "uno uno due",
            "che che cosa",
        ] {
            assert_eq!(it(kept), kept);
        }
    }

    /// Portuguese is where a shared filler list would do the most damage: the
    /// English "um" is the Portuguese article.
    #[test]
    fn portuguese() {
        let pt = |t| apply(t, Some("pt-BR"));
        assert_eq!(pt("hum eu eu acho ãh que sim"), "eu acho que sim");
        assert_eq!(pt("o o carro"), "o carro");
        for kept in [
            "um livro em casa",
            "um um dois",
            "o que que é isso",
            "o ônibus para para ali",
            "é né tipo então",
            "hã",
        ] {
            assert_eq!(pt(kept), kept);
        }
    }

    #[test]
    fn dutch() {
        let nl = |t| apply(t, Some("nl"));
        assert_eq!(nl("eh ik ik denk uhm van wel"), "ik denk van wel");
        assert_eq!(nl("de de auto"), "de auto");
        for kept in [
            "ik weet dat dat waar is",
            "de man die die auto kocht",
            "ik vind het het beste",
            "heb je je sleutels",
            "hebben ze ze gezien",
            "een schilderij van Van Gogh",
            "nul zes een een",
            "stap je in in Utrecht",
            "nou tja hè",
        ] {
            assert_eq!(nl(kept), kept);
        }
    }

    #[test]
    fn only_languages_with_rules_are_claimed() {
        assert!(covers(None));
        assert!(covers(Some("en-GB")));
        for code in supported_languages() {
            assert!(covers(Some(code)));
        }
        for other in ["ja", "ru", "pl"] {
            assert!(!covers(Some(other)), "{other} has no cleanup rules");
            assert_eq!(apply("um the the", Some(other)), "um the the");
        }
    }
}
