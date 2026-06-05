/// Simple energy-based Voice Activity Detector.
/// Phase 1 uses this lightweight fallback; Silero VAD (ONNX) replaces it in Phase 2.
pub struct EnergyVad {
    threshold: f32,
    /// Window of recent energy values used for dynamic thresholding.
    history: Vec<f32>,
    history_max: usize,
    /// Number of consecutive silent frames before declaring silence.
    silence_frames: usize,
    silent_count: usize,
}

impl EnergyVad {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            history: Vec::new(),
            history_max: 50,
            silence_frames: 15,
            silent_count: 0,
        }
    }

    pub fn is_speech(&mut self, samples: &[f32]) -> bool {
        let rms = rms(samples);
        self.history.push(rms);
        if self.history.len() > self.history_max {
            self.history.remove(0);
        }

        let active = rms > self.threshold;
        if active {
            self.silent_count = 0;
        } else {
            self.silent_count += 1;
        }

        active || self.silent_count < self.silence_frames
    }

    /// Reset internal state between recordings.
    pub fn reset(&mut self) {
        self.history.clear();
        self.silent_count = 0;
    }
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sq_sum: f32 = samples.iter().map(|s| s * s).sum();
    (sq_sum / samples.len() as f32).sqrt()
}
