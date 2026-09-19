/*!
 * SOURCE OF TRUTH KEYWORDS: LatencyRecorder, StageTimer, MetricSample, mark,
 *   take_samples, percentile, StageLatency
 * WHAT:  Collects per-stage timings for one dictation and hands them over as a
 *        batch when it ends.
 * WHY:   Echo could already count events; it could not say where the time went.
 *        "Stop talking to text on screen" is the number the product is actually
 *        judged on, and before this nothing measured it — the benchmark command
 *        timed decodes in isolation and said so in its own output.
 *
 *        TIMINGS ACCUMULATE IN MEMORY and are written once. A dictation emits
 *        several of these inside the finalize budget, and spending a SQLite
 *        round trip on each would make the act of measuring latency a
 *        measurable part of it.
 *
 *        Durations are measured with Instant, never by subtracting wall-clock
 *        timestamps, so a clock adjustment mid-dictation cannot produce a
 *        negative elapsed time — which SQLite would happily store and the
 *        percentile would happily report.
 *
 *        Recorded against the LatencyStage ENUM, never a free string, so the
 *        insights panel cannot end up querying a stage nothing writes. That is
 *        the same guarantee registry/reachability.rs checks from the other
 *        direction.
 * WHERE: Held by the recording session; drained into storage::repositories by
 *        commands/recording.rs when the dictation finishes.
 */

use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use specta::Type;

/**
 * SOURCE OF TRUTH KEYWORDS: LatencyStage
 * WHAT:  One measured leg of the path from keypress to text on screen.
 * WHY:   Recorded against this enum, never a free string, so the insights
 *        panel cannot end up querying a stage nothing writes — which is the
 *        failure the reachability test exists to catch, expressed as a type.
 *
 *        The stages are cut at the boundaries a user would recognise, not at
 *        the ones the code happens to have: everything before the first sample
 *        is Capture, everything the model owns is Decode, everything after the
 *        last token is Deliver. TotalFinalize is the one the product promises —
 *        stop talking to text arriving — and it is deliberately measured
 *        end-to-end rather than summed from the others, so a stage nobody
 *        thought to time cannot hide inside it.
 * WHERE: Written by the recorder below, declared by registry entries, read by
 *        the insights panel.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LatencyStage {
    /// Hotkey press to the first audio sample reaching the pipeline. On a cold
    /// microphone this is most of the answer to "why did it miss my first word".
    CaptureStart,
    /// Text handed to the OS, to it landing in the focused app.
    Inject,
    /// Stop to delivered, measured end to end. The number the product promises.
    ///
    /// Deliberately measured end to end rather than summed from the others, so
    /// a stage nobody thought to time cannot hide inside it.
    TotalFinalize,
}

impl LatencyStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            LatencyStage::CaptureStart => "capture_start",
            LatencyStage::Inject => "inject",
            LatencyStage::TotalFinalize => "total_finalize",
        }
    }
}

/// One measurement, ready to be stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricSample {
    pub stage: LatencyStage,
    pub ms: u64,
}

/**
 * SOURCE OF TRUTH KEYWORDS: LatencyRecorder
 * WHAT:  A thread-safe bag of stage timings for one dictation.
 * WHY:   Shared across the tokio side and the decoder task, so it is behind a
 *        lock — but one held only for a push, never across an await, which
 *        keeps it uncontended in practice.
 * WHERE: One per dictation.
 */
#[derive(Debug, Default)]
pub struct LatencyRecorder {
    samples: Mutex<Vec<MetricSample>>,
}

impl LatencyRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a finished measurement.
    pub fn record(&self, stage: LatencyStage, ms: u64) {
        if let Ok(mut samples) = self.samples.lock() {
            samples.push(MetricSample { stage, ms });
        }
    }

    /// Starts timing a stage. The timer records itself when told to stop.
    pub fn start(&self, stage: LatencyStage) -> StageTimer<'_> {
        StageTimer {
            recorder: self,
            stage,
            started: Instant::now(),
        }
    }

    /// Takes everything collected so far, leaving the recorder empty.
    pub fn take_samples(&self) -> Vec<MetricSample> {
        self.samples
            .lock()
            .map(|mut s| std::mem::take(&mut *s))
            .unwrap_or_default()
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: StageTimer
 * WHAT:  An open measurement of one stage.
 * WHY:   Explicitly stopped rather than recording on Drop. A Drop-based timer
 *        records on every path out of the scope, including the error paths —
 *        and a failed decode's duration is not a decode time. It would drag the
 *        percentile toward whatever the timeout happens to be.
 * WHERE: Returned by LatencyRecorder::start.
 */
pub struct StageTimer<'a> {
    recorder: &'a LatencyRecorder,
    stage: LatencyStage,
    started: Instant,
}

impl StageTimer<'_> {
    /// Records the elapsed time. Only call this when the stage SUCCEEDED.
    pub fn stop(self) {
        let ms = self.started.elapsed().as_millis() as u64;
        self.recorder.record(self.stage, ms);
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: StageLatency, percentile
 * WHAT:  What the insights panel shows for one stage.
 * WHY:   p50 AND p95, because they answer different questions and only the pair
 *        is honest. The median is what dictation usually feels like; the p95 is
 *        the one people actually remember, because a delay you notice once in
 *        twenty is a delay you have noticed. Reporting only the median is how
 *        an app measures well and feels slow.
 * WHERE: Produced by storage::repositories::latency_summary.
 */
#[derive(Debug, Clone, Serialize, Type)]
pub struct StageLatency {
    pub stage: String,
    pub label: String,
    pub p50_ms: u64,
    pub p95_ms: u64,
    /// How many measurements the pair is drawn from. Shown, because a p95 over
    /// four samples is not a p95 and the reader deserves to know.
    pub samples: u64,
}

/**
 * SOURCE OF TRUTH KEYWORDS: percentile
 * WHAT:  The nearest-rank percentile of an already-sorted slice.
 * WHY:   Nearest-rank rather than interpolating, because these are measured
 *        milliseconds: every value returned is a duration that genuinely
 *        happened, which is a claim an interpolated figure cannot make.
 * WHERE: Used by the latency queries and tested below.
 */
pub fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    // Nearest-rank: ceil(p * n), clamped into the slice.
    let rank = (p * sorted.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_values_that_actually_happened() {
        let sorted = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&sorted, 0.5), 50);
        assert_eq!(percentile(&sorted, 0.95), 100);
        // Every answer is one of the inputs — nothing is interpolated into
        // existence.
        for p in [0.0, 0.25, 0.5, 0.75, 0.95, 1.0] {
            assert!(sorted.contains(&percentile(&sorted, p)));
        }
    }

    #[test]
    fn a_single_sample_is_its_own_median_and_p95() {
        assert_eq!(percentile(&[42], 0.5), 42);
        assert_eq!(percentile(&[42], 0.95), 42);
    }

    #[test]
    fn no_samples_reports_zero_rather_than_panicking() {
        assert_eq!(percentile(&[], 0.5), 0);
    }

    #[test]
    fn the_recorder_hands_over_everything_and_empties() {
        let recorder = LatencyRecorder::new();
        recorder.record(LatencyStage::CaptureStart, 120);
        recorder.record(LatencyStage::Inject, 8);

        let samples = recorder.take_samples();
        assert_eq!(samples.len(), 2);
        assert!(
            recorder.take_samples().is_empty(),
            "samples were handed over twice"
        );
    }

    /// The reason the timer is explicit rather than Drop-based: a stage that
    /// failed must contribute nothing, or the percentile drifts toward the
    /// timeout.
    #[test]
    fn a_timer_that_is_never_stopped_records_nothing() {
        let recorder = LatencyRecorder::new();
        {
            let _timer = recorder.start(LatencyStage::Inject);
            // Dropped without stop() — the decode failed.
        }
        assert!(recorder.take_samples().is_empty());
    }

    #[test]
    fn a_stopped_timer_records_its_stage() {
        let recorder = LatencyRecorder::new();
        recorder.start(LatencyStage::CaptureStart).stop();

        let samples = recorder.take_samples();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].stage, LatencyStage::CaptureStart);
    }
}
