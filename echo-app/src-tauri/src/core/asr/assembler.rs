/*!
 * SOURCE OF TRUTH KEYWORDS: Assembler, push_chunk, finish, join_overlapping,
 *   MAX_SEAM_WORDS, normalise_word
 * WHAT:  Joins the transcripts decoded from successive chunks into one
 *        transcript, removing the duplication the deliberate chunk overlap
 *        creates.
 * WHY:   THE JOIN IS DONE ON TEXT, not on timestamps, and that is forced rather
 *        than chosen: Echo's decoders return a chunk's transcript as one
 *        string, with no reliable sub-chunk timing to align on. Anything trying
 *        to de-duplicate by comparing times would find the pieces merely
 *        adjacent and silently do nothing, which looks exactly like working.
 *
 *        The overlap is ~200ms, which is at most a word or two. THE SEARCH IS
 *        BOUNDED ACCORDINGLY. A longer window would start "finding" overlaps in
 *        ordinary repeated speech — "that that", "had had", a stutter — and
 *        delete words the user actually said, which is far worse than leaving a
 *        duplicate in. When in doubt this keeps the duplicate.
 *
 *        Pieces are inserted BY START TIME rather than appended, because chunks
 *        decode concurrently and may complete out of order. A transcript
 *        assembled in completion order would scramble under load, which is
 *        precisely when nobody is watching for it.
 * WHERE: Fed by the decoder as chunks complete; its output goes to the
 *        formatting stages.
 */

/// Upper bound on the seam search. Comfortably covers a 200ms overlap while
/// staying too short to match a genuine repeated phrase.
const MAX_SEAM_WORDS: usize = 6;

/**
 * SOURCE OF TRUTH KEYWORDS: Assembler
 * WHAT:  Accumulates decoded text in chunk order.
 * WHERE: One per dictation.
 */
#[derive(Debug, Default)]
pub struct Assembler {
    /// (start_ms, text), kept sorted by start_ms.
    parts: Vec<(u64, String)>,
}

impl Assembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one chunk's transcript, in its true position.
    pub fn push_chunk(&mut self, start_ms: u64, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let at = self
            .parts
            .iter()
            .position(|(existing, _)| *existing > start_ms)
            .unwrap_or(self.parts.len());
        self.parts.insert(at, (start_ms, text.to_string()));
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The finished transcript, seams removed.
    pub fn finish(&self) -> String {
        let mut out = String::new();
        for (_, text) in &self.parts {
            if out.is_empty() {
                out.push_str(text);
            } else {
                out = join_overlapping(&out, text);
            }
        }
        out
    }
}

/// Words compare ignoring case and trailing punctuation, because the decoder
/// punctuates each chunk independently: the same word can arrive as "there"
/// at the end of one and "There," at the start of the next.
fn normalise_word(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/**
 * SOURCE OF TRUTH KEYWORDS: join_overlapping
 * WHAT:  Concatenates two transcripts, dropping a repeated run of words at the
 *        seam if there is one.
 * WHY:   Longest match first, so "the cat sat" + "cat sat on" drops both
 *        repeated words rather than only the first. Bounded by MAX_SEAM_WORDS —
 *        see the module WHY for why a longer search is actively harmful.
 * WHERE: Used by Assembler::finish; tested below.
 */
pub fn join_overlapping(left: &str, right: &str) -> String {
    let left_words: Vec<&str> = left.split_whitespace().collect();
    let right_words: Vec<&str> = right.split_whitespace().collect();

    if left_words.is_empty() {
        return right.to_string();
    }
    if right_words.is_empty() {
        return left.to_string();
    }

    let max = MAX_SEAM_WORDS.min(left_words.len()).min(right_words.len());
    for len in (1..=max).rev() {
        let tail = &left_words[left_words.len() - len..];
        let head = &right_words[..len];
        if tail
            .iter()
            .zip(head.iter())
            .all(|(a, b)| normalise_word(a) == normalise_word(b) && !normalise_word(a).is_empty())
        {
            let kept = right_words[len..].join(" ");
            if kept.is_empty() {
                return left.to_string();
            }
            return format!("{left} {kept}");
        }
    }

    format!("{left} {right}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_word_at_the_seam_is_dropped_once() {
        assert_eq!(
            join_overlapping("the quick brown fox", "fox jumps over"),
            "the quick brown fox jumps over"
        );
    }

    /// Longest match first, or the second repeated word survives.
    #[test]
    fn a_repeated_run_is_dropped_whole() {
        assert_eq!(
            join_overlapping("the cat sat", "cat sat on the mat"),
            "the cat sat on the mat"
        );
    }

    /// The decoder punctuates and capitalises each chunk on its own, so the
    /// same word arrives differently dressed on each side of the seam.
    #[test]
    fn matching_ignores_case_and_punctuation() {
        assert_eq!(
            join_overlapping("we should go there", "There, and then home"),
            "we should go there and then home"
        );
    }

    /// THE FAILURE THAT MATTERS. A long search would call this an overlap and
    /// delete a word the user said.
    #[test]
    fn genuinely_repeated_speech_is_not_treated_as_a_seam() {
        // Far enough apart that no bounded seam search should connect them.
        let joined = join_overlapping(
            "I said that we would go to the shop and then",
            "the shop was closed so we came back",
        );
        assert_eq!(
            joined,
            "I said that we would go to the shop and then the shop was closed so we came back"
        );
    }

    #[test]
    fn no_overlap_is_a_plain_join() {
        assert_eq!(
            join_overlapping("first part", "second part entirely"),
            "first part second part entirely"
        );
    }

    #[test]
    fn a_chunk_wholly_contained_in_the_previous_one_adds_nothing() {
        assert_eq!(
            join_overlapping("hello there", "hello there"),
            "hello there"
        );
    }

    /// Chunks decode concurrently, so they arrive out of order. The transcript
    /// must not.
    #[test]
    fn pieces_are_ordered_by_start_time_not_arrival() {
        let mut assembler = Assembler::new();
        assembler.push_chunk(8_000, "and then we left");
        assembler.push_chunk(0, "we arrived at noon");
        assembler.push_chunk(16_000, "before it got dark");

        assert_eq!(
            assembler.finish(),
            "we arrived at noon and then we left before it got dark"
        );
    }

    #[test]
    fn empty_chunks_are_ignored() {
        let mut assembler = Assembler::new();
        assembler.push_chunk(0, "something");
        assembler.push_chunk(8_000, "   ");
        assembler.push_chunk(16_000, "");
        assert_eq!(assembler.finish(), "something");
    }

    #[test]
    fn nothing_decoded_is_an_empty_transcript_not_a_panic() {
        assert!(Assembler::new().finish().is_empty());
        assert!(Assembler::new().is_empty());
    }

    /// The realistic end-to-end shape: three chunks, each overlapping the last
    /// by a word, arriving out of order.
    #[test]
    fn a_chunked_utterance_reassembles_into_the_original_sentence() {
        let mut assembler = Assembler::new();
        assembler.push_chunk(14_800, "meeting to Thursday if that works");
        assembler.push_chunk(0, "Let's move the");
        assembler.push_chunk(7_800, "move the meeting to Thursday");

        assert_eq!(
            assembler.finish(),
            "Let's move the meeting to Thursday if that works"
        );
    }
}
