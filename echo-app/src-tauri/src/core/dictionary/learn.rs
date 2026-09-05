//! Learning dictionary entries from the corrections a user makes by hand.
//!
//! When someone fixes "cooper netties" to "Kubernetes", they have just told us
//! two things for free: what the decoder gets wrong, and what the right answer
//! is. That is exactly the pair [`super::DictionaryEngine`] needs — both to
//! repair future transcripts and, more valuably, to hint the decoder so the
//! mistake stops happening at all.
//!
//! The hard part is not finding the difference, it is refusing to learn
//! nonsense. Rewording a sentence must not quietly fill the dictionary with
//! rules that then corrupt every later transcript, so almost everything here is
//! about rejection. Two ideas do the work:
//!
//! 1. **A correction is a small span inside otherwise untouched text.** Common
//!    prefix and suffix are stripped, and only what is left is a candidate. An
//!    edit with nothing in common at either end is a rewrite, not a fix.
//! 2. **A mishearing resembles its target when *spoken*, not when spelled.**
//!    "cooper netties" and "Kubernetes" share almost no letters, so a plain
//!    character diff rejects the very case this exists for. Comparing consonant
//!    skeletons catches it, while still rejecting "cat" → "elephant".

/// A learned replacement: `from` is what Echo produced, `to` is what the user
/// changed it to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub from: String,
    pub to: String,
}

/// Words shorter than this are skipped.
///
/// Short tokens are where false positives live: "in" → "on" is ordinary
/// rewording, not a mishearing, and as a dictionary rule it would corrupt every
/// later transcript containing that word.
const MIN_WORD_LEN: usize = 3;

/// Longest changed span, in words, still treated as a correction. Beyond this
/// the user restructured the sentence.
const MAX_SPAN_WORDS: usize = 4;

/// The changed span may not exceed this share of the whole text, so that a
/// short transcript cannot be "corrected" into something unrelated.
const MAX_CHANGED_RATIO: f32 = 0.5;

/// Permitted difference between two strings, as a fraction of the longer one.
const SIMILARITY_TOLERANCE: f32 = 0.5;

/// Extract a high-confidence correction from a hand-edited transcript.
///
/// Returns an empty vec whenever the edit does not look like a correction —
/// which is the common case, and the right answer for it.
pub fn extract_corrections(original: &str, edited: &str) -> Vec<Correction> {
    match correction_from(original, edited) {
        Some(c) => vec![c],
        None => Vec::new(),
    }
}

fn correction_from(original: &str, edited: &str) -> Option<Correction> {
    let from_words = tokenize(original);
    let to_words = tokenize(edited);

    if from_words.is_empty() || to_words.is_empty() || from_words == to_words {
        return None;
    }

    let (from_span, to_span) = changed_span(&from_words, &to_words)?;

    // A pure insertion or deletion has no pair to learn: added words replace
    // nothing, and removed words have no replacement.
    if from_span.is_empty() || to_span.is_empty() {
        return None;
    }
    if from_span.len() > MAX_SPAN_WORDS || to_span.len() > MAX_SPAN_WORDS {
        return None;
    }

    // How much of the original the edit touched.
    let longest = from_words.len().max(to_words.len()) as f32;
    if from_span.len().max(to_span.len()) as f32 / longest > MAX_CHANGED_RATIO {
        return None;
    }

    let from = from_span.join(" ");
    let to = to_span.join(" ");
    is_plausible_correction(&from, &to).then_some(Correction { from, to })
}

/// Strip the common prefix and suffix, returning the differing middle of each.
fn changed_span<'a>(
    from: &'a [String],
    to: &'a [String],
) -> Option<(&'a [String], &'a [String])> {
    let prefix = from
        .iter()
        .zip(to.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // Suffix must not reach back past the prefix in either sequence.
    let max_suffix = (from.len().min(to.len())) - prefix;
    let suffix = from
        .iter()
        .rev()
        .zip(to.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(max_suffix);

    Some((
        &from[prefix..from.len() - suffix],
        &to[prefix..to.len() - suffix],
    ))
}

/// Whether `from` → `to` looks like a fixed mishearing rather than a reworded
/// phrase.
fn is_plausible_correction(from: &str, to: &str) -> bool {
    if from.chars().count() < MIN_WORD_LEN || to.chars().count() < MIN_WORD_LEN {
        return false;
    }

    if from.eq_ignore_ascii_case(to) {
        // A capitalisation fix worth learning changes more than the first
        // letter ("github" → "GitHub"). A change to the first letter alone is
        // someone capitalising the start of a sentence, which as a dictionary
        // rule would capitalise that word everywhere.
        return from != to && !differs_only_in_first_char(from, to);
    }

    // Spelled alike (a typo fixed), or sounding alike (a mishearing fixed).
    similar(&normalise(from), &normalise(to))
        || similar(&consonant_skeleton(from), &consonant_skeleton(to))
}

fn differs_only_in_first_char(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    a.len() == b.len() && a.iter().zip(&b).skip(1).all(|(x, y)| x == y)
}

/// Whether two keys are close enough to be the same thing, mistyped or misheard.
fn similar(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let longest = a.len().max(b.len());
    if longest == 0 {
        return false;
    }
    let allowed = (longest as f32 * SIMILARITY_TOLERANCE).ceil() as usize;
    edit_distance(&a, &b) <= allowed
}

fn normalise(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// A crude phonetic key: the consonants that carry the sound of a word.
///
/// Vowels are dropped because they are what mishearings scramble most, letters
/// that sound alike are folded together (c/k/q, s/z, f/ph), and runs are
/// collapsed. It is not a real phonetic algorithm — it does not need to be. It
/// only has to bring "coopernetties" and "kubernetes" close together while
/// keeping "cat" and "elephant" apart.
fn consonant_skeleton(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        let mapped = match c {
            'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'h' | 'w' => continue,
            'c' | 'k' | 'q' | 'g' | 'x' => 'k',
            's' | 'z' => 's',
            'f' | 'v' | 'p' | 'b' => 'f',
            'd' | 't' => 't',
            'm' | 'n' => 'n',
            'l' | 'r' => 'r',
            c if c.is_alphanumeric() => c,
            _ => continue,
        };
        // Collapse doubled sounds ("tt", and the boundary in "cooper netties").
        if !out.ends_with(mapped) {
            out.push(mapped);
        }
    }
    out
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let m = b.len();
    // Two rows are enough — we never need to reconstruct the path.
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];

    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            curr[j + 1] = if ca == cb {
                prev[j]
            } else {
                1 + prev[j].min(prev[j + 1]).min(curr[j])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// Split into words, dropping punctuation at the edges so "Kubernetes." and
/// "Kubernetes" are the same token.
fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                .to_string()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corrections(a: &str, b: &str) -> Vec<(String, String)> {
        extract_corrections(a, b)
            .into_iter()
            .map(|c| (c.from, c.to))
            .collect()
    }

    /// The case this whole module exists for: two words misheard as one, which
    /// sound alike but share almost no letters.
    #[test]
    fn learns_a_phonetic_mishearing_spanning_two_words() {
        assert_eq!(
            corrections("deploy to cooper netties today", "deploy to Kubernetes today"),
            vec![("cooper netties".into(), "Kubernetes".into())]
        );
    }

    #[test]
    fn learns_a_single_word_typo_fix() {
        assert_eq!(
            corrections("please recieve the file", "please receive the file"),
            vec![("recieve".into(), "receive".into())]
        );
    }

    #[test]
    fn learns_an_internal_capitalisation_fix() {
        assert_eq!(
            corrections("push it to github now", "push it to GitHub now"),
            vec![("github".into(), "GitHub".into())]
        );
    }

    #[test]
    fn sentence_capitalisation_is_not_a_dictionary_rule() {
        // Learning this would capitalise "ship" everywhere it ever appears.
        assert!(corrections("ship it now", "Ship it now").is_empty());
    }

    #[test]
    fn an_unchanged_transcript_teaches_nothing() {
        assert!(corrections("all good here", "all good here").is_empty());
        assert!(corrections("", "").is_empty());
        assert!(corrections("something", "").is_empty());
    }

    #[test]
    fn punctuation_only_edits_are_not_corrections() {
        assert!(corrections("ship it", "ship it.").is_empty());
    }

    /// The important negative: rewriting must not poison the dictionary.
    #[test]
    fn a_rewrite_is_rejected_rather_than_learned() {
        assert!(corrections(
            "the meeting is at three o'clock tomorrow afternoon",
            "let us postpone until next week entirely instead"
        )
        .is_empty());
    }

    #[test]
    fn swapping_in_a_different_word_is_not_a_correction() {
        // Neither spelled nor sounding alike — a reword, not a fix.
        assert!(corrections("the cat is here", "the elephant is here").is_empty());
    }

    #[test]
    fn short_words_are_never_learned() {
        assert!(corrections("put it in the box", "put it on the box").is_empty());
    }

    #[test]
    fn pure_insertions_and_deletions_teach_nothing() {
        assert!(corrections("the build passed", "the build passed and tests are green").is_empty());
        assert!(corrections("the build passed cleanly today", "the build passed today").is_empty());
    }

    #[test]
    fn consonant_skeleton_brings_homophones_together() {
        assert_eq!(consonant_skeleton("cooper netties"), consonant_skeleton("kubernetes"));
        assert_ne!(consonant_skeleton("cat"), consonant_skeleton("elephant"));
    }

    #[test]
    fn edit_distance_matches_known_values() {
        let d = |a: &str, b: &str| {
            edit_distance(
                &a.chars().collect::<Vec<_>>(),
                &b.chars().collect::<Vec<_>>(),
            )
        };
        assert_eq!(d("kitten", "sitting"), 3);
        assert_eq!(d("", "abc"), 3);
        assert_eq!(d("same", "same"), 0);
    }
}
