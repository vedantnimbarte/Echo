/*!
 * SOURCE OF TRUTH KEYWORDS: Chunker, push, close_tail, ChunkDecision,
 *   MIN_CHUNK_MS, MAX_CHUNK_MS, BOUNDARY_SILENCE_MS, OVERLAP_MS
 * WHAT:  Accumulates captured audio and closes it into chunks at natural
 *        silence boundaries, with a deliberate overlap between them.
 * WHY:   This is the file that decides whether finishing a five-minute
 *        monologue costs the same as finishing a five-second one, and the
 *        numbers in it are counter-intuitive enough to be worth stating.
 *
 *        WHISPER'S ENCODER ALWAYS PROCESSES A 30-SECOND WINDOW. Shorter audio
 *        is padded up to it. So a 1-second chunk costs almost as much to encode
 *        as a 25-second one, and the instinct — "chunk small for low latency" —
 *        makes the app dramatically SLOWER: chunking at 1s would be roughly ten
 *        times the total compute of chunking at 10s.
 *
 *        So chunks are LONG (8-15s) and closed at silence. They decode in the
 *        background while the user keeps talking, so their individual latency
 *        is invisible. Only the trailing fragment is on the critical path when
 *        the user stops.
 *
 *        This replaces nothing, and that is worth being clear about: Echo's
 *        existing partial path re-decodes the whole utterance every time it wants a
 *        preview, which is quadratic and capped for exactly that reason. That
 *        path produces PREVIEW text; this one produces the FINAL text in pieces.
 *
 *        Chunks overlap by 200ms so a word spoken across a boundary is not
 *        lost. The duplicate that overlap creates is removed downstream by the
 *        assembler, on TEXT — the transcripts carry no reliable sub-chunk
 *        timing to align on.
 * WHERE: Fed by the capture stream; its chunks go to the decoder and its
 *        transcripts to core/asr/assembler.rs.
 */

/// Echo captures at 16 kHz mono, which is also what whisper wants.
const SAMPLE_RATE: usize = 16_000;

/// Below this a chunk is not worth its own encoder pass — see the module WHY.
pub const MIN_CHUNK_MS: u64 = 8_000;
/// Above this we close regardless of silence, so a continuous talker still gets
/// background decoding rather than one enormous chunk at the end.
pub const MAX_CHUNK_MS: u64 = 15_000;
/// Silence long enough to be a natural break rather than a breath.
pub const BOUNDARY_SILENCE_MS: u64 = 350;
/// Carried into the next chunk so a word across the seam survives.
pub const OVERLAP_MS: u64 = 200;

fn ms_to_samples(ms: u64) -> usize {
    (ms as usize * SAMPLE_RATE) / 1000
}

fn samples_to_ms(samples: usize) -> u64 {
    (samples as u64 * 1000) / SAMPLE_RATE as u64
}

/// A closed chunk, ready to decode.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// Where this chunk starts in the utterance, so the assembler can order
    /// transcripts that come back out of order.
    pub start_ms: u64,
    pub samples: Vec<f32>,
    /// True for the fragment closed when the user stopped talking. It is the
    /// only chunk on the critical path.
    pub is_tail: bool,
}

/**
 * SOURCE OF TRUTH KEYWORDS: Chunker
 * WHAT:  Holds the open chunk and decides when to close it.
 * WHERE: One per dictation.
 */
#[derive(Debug)]
pub struct Chunker {
    open: Vec<f32>,
    /// Where the open chunk began, in the utterance.
    start_ms: u64,
    /// Samples consumed so far, including everything already closed.
    consumed: usize,
    /// How long the run of quiet at the end of `open` is.
    trailing_quiet_ms: u64,
}

impl Default for Chunker {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunker {
    pub fn new() -> Self {
        Self {
            open: Vec::new(),
            start_ms: 0,
            consumed: 0,
            trailing_quiet_ms: 0,
        }
    }

    /// How much audio the open chunk holds.
    pub fn open_ms(&self) -> u64 {
        samples_to_ms(self.open.len())
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: push
     * WHAT:  Adds captured audio, returning a chunk if this closed one.
     * WHY:   `is_quiet` is passed in rather than measured here, because the
     *        capture path has already run a VAD over this very audio and
     *        measuring it twice would be two answers that can disagree — and
     *        the one this file would produce is the worse of the two.
     * WHERE: Called for every buffer the capture stream delivers.
     */
    pub fn push(&mut self, samples: &[f32], is_quiet: bool) -> Option<Chunk> {
        if samples.is_empty() {
            return None;
        }

        self.open.extend_from_slice(samples);
        self.consumed += samples.len();

        let chunk_ms = samples_to_ms(self.open.len());
        self.trailing_quiet_ms = if is_quiet {
            self.trailing_quiet_ms + samples_to_ms(samples.len())
        } else {
            0
        };

        // Long enough to be worth an encoder pass, AND at a natural break.
        let at_boundary = chunk_ms >= MIN_CHUNK_MS && self.trailing_quiet_ms >= BOUNDARY_SILENCE_MS;
        // Or simply too long: a continuous talker must not accumulate one
        // enormous chunk that all lands at the end.
        let too_long = chunk_ms >= MAX_CHUNK_MS;

        if at_boundary || too_long {
            Some(self.close(false))
        } else {
            None
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: close_tail
     * WHAT:  Closes whatever is open, because the user stopped talking.
     * WHY:   Returns None for an empty tail rather than an empty chunk: a
     *        decode of nothing costs a full encoder window and returns
     *        boilerplate, which is the exact thing the hallucination guard then
     *        has to clean up.
     * WHERE: Called when capture ends.
     */
    pub fn close_tail(&mut self) -> Option<Chunk> {
        if self.open.is_empty() {
            return None;
        }
        Some(self.close(true))
    }

    fn close(&mut self, is_tail: bool) -> Chunk {
        let overlap = ms_to_samples(OVERLAP_MS).min(self.open.len());
        let samples = std::mem::take(&mut self.open);
        let start_ms = self.start_ms;

        if is_tail {
            self.start_ms = samples_to_ms(self.consumed);
        } else {
            // The next chunk begins inside this one, by the overlap, so a word
            // spoken across the seam appears whole in at least one of them.
            let carried = samples[samples.len() - overlap..].to_vec();
            self.start_ms = start_ms + samples_to_ms(samples.len() - overlap);
            self.open = carried;
        }
        self.trailing_quiet_ms = 0;

        Chunk {
            start_ms,
            samples,
            is_tail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One second of audio, as the capture path would deliver it.
    fn second() -> Vec<f32> {
        vec![0.05; SAMPLE_RATE]
    }

    fn ms(n: usize) -> Vec<f32> {
        vec![0.05; ms_to_samples(n as u64)]
    }

    #[test]
    fn a_short_utterance_produces_one_tail_chunk_and_nothing_else() {
        let mut chunker = Chunker::new();
        for _ in 0..3 {
            assert!(chunker.push(&second(), false).is_none());
        }
        let tail = chunker.close_tail().expect("a tail");
        assert!(tail.is_tail);
        assert_eq!(tail.start_ms, 0);
    }

    /// The core claim: silence does not close a chunk that is too short to be
    /// worth an encoder pass. See the module WHY.
    #[test]
    fn silence_before_the_minimum_does_not_close_a_chunk() {
        let mut chunker = Chunker::new();
        chunker.push(&second(), false);
        // A full second of quiet, far past the boundary threshold.
        assert!(
            chunker.push(&second(), true).is_none(),
            "closed a 2s chunk, which costs nearly as much to encode as a 15s one"
        );
    }

    #[test]
    fn silence_after_the_minimum_closes_at_the_boundary() {
        let mut chunker = Chunker::new();
        for _ in 0..9 {
            assert!(chunker.push(&second(), false).is_none());
        }
        let chunk = chunker
            .push(&ms(BOUNDARY_SILENCE_MS as usize), true)
            .expect("closed at the pause");
        assert!(!chunk.is_tail);
        assert!(samples_to_ms(chunk.samples.len()) >= MIN_CHUNK_MS);
    }

    /// Someone who does not pause must still get background decoding.
    #[test]
    fn a_continuous_talker_is_cut_at_the_maximum() {
        let mut chunker = Chunker::new();
        let mut closed = None;
        for _ in 0..20 {
            if let Some(chunk) = chunker.push(&second(), false) {
                closed = Some(chunk);
                break;
            }
        }
        let chunk = closed.expect("never closed a chunk for a continuous talker");
        assert!(samples_to_ms(chunk.samples.len()) >= MAX_CHUNK_MS);
    }

    /// The overlap is what stops a word on the seam being lost.
    #[test]
    fn the_next_chunk_carries_the_overlap() {
        let mut chunker = Chunker::new();
        for _ in 0..9 {
            chunker.push(&second(), false);
        }
        let first = chunker
            .push(&ms(BOUNDARY_SILENCE_MS as usize), true)
            .expect("closed");

        // The open chunk starts already holding the overlap.
        assert_eq!(chunker.open_ms(), OVERLAP_MS);
        // And the next chunk's start is set back by exactly that much, so the
        // assembler can order them truthfully.
        let expected = first.start_ms + samples_to_ms(first.samples.len()) - OVERLAP_MS;
        assert_eq!(chunker.start_ms, expected);
    }

    /// An empty tail must not be sent: a decode of nothing costs a full encoder
    /// window and comes back with boilerplate.
    #[test]
    fn an_empty_tail_is_not_a_chunk() {
        let mut chunker = Chunker::new();
        assert!(chunker.close_tail().is_none());

        // And after a chunk closes exactly on a boundary, the tail is only the
        // overlap — still real audio, so still a chunk.
        for _ in 0..9 {
            chunker.push(&second(), false);
        }
        chunker.push(&ms(BOUNDARY_SILENCE_MS as usize), true);
        assert!(chunker.close_tail().is_some());
    }

    /// Start times must be monotonic, or the assembler orders the transcript
    /// wrongly under load — which is when it is hardest to notice.
    #[test]
    fn chunk_starts_never_go_backwards() {
        let mut chunker = Chunker::new();
        let mut last = 0;
        for i in 0..60 {
            if let Some(chunk) = chunker.push(&second(), i % 10 == 9) {
                assert!(chunk.start_ms >= last, "{} < {}", chunk.start_ms, last);
                last = chunk.start_ms;
            }
        }
        if let Some(tail) = chunker.close_tail() {
            assert!(tail.start_ms >= last);
        }
    }
}
