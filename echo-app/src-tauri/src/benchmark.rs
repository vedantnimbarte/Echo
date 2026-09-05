//! `echo --benchmark` — put numbers on the dictation path.
//!
//! The resident whisper server and the GPU packs were argued for on
//! architecture and shipped without a measurement, which meant "Echo is faster
//! now" was a claim rather than a fact. This measures the thing that was
//! actually changed.
//!
//! What it is honest about: whisper.cpp pads any clip shorter than 30 seconds
//! out to a full 30-second window, so the encoder — the expensive part, and the
//! part a GPU accelerates — costs the same whether you dictate for one second
//! or twenty. Only the decoder loop scales with how much was actually said, and
//! a synthetic tone produces almost no tokens. So these numbers represent
//! **engine overhead**: process spawn, model load, IPC and the encoder pass.
//! That is exactly what the resident server removed, so it is the right
//! measurement for that claim, and a fair GPU-versus-CPU comparison. It will
//! understate the decode time of a long, dense sentence.
//!
//! Deliberately not run for timing in CI: shared runners are too noisy for the
//! numbers to mean anything. CI runs it only to prove the harness still works.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use tauri::Manager;

use crate::core::asr::decode_opts::DecodeConfig;
use crate::core::asr::whisper_cli;
use crate::core::asr::whisper_server::{DecodeConfigKey, Signature};
use crate::core::vad::{gate::speech_gate, EnergyVad, Vad};
use crate::state::AppState;

/// Seconds of synthetic audio per transcription. Any value under 30 costs the
/// encoder the same, so this is kept short to keep the run bearable.
const CLIP_SECONDS: usize = 3;
const SAMPLE_RATE: usize = 16_000;

/// Transcriptions per engine. Each CLI run reloads the model, so this is a
/// balance between a stable median and a benchmark nobody waits for.
const ASR_RUNS: usize = 5;

/// The in-process stages are microseconds each; more runs cost nothing.
const MICRO_RUNS: usize = 50;

pub fn requested() -> bool {
    std::env::args().any(|arg| arg == "--benchmark")
}

/// One measured stage.
struct Timing {
    label: String,
    samples: Vec<Duration>,
}

impl Timing {
    fn median(&self) -> Duration {
        let mut sorted = self.samples.clone();
        sorted.sort();
        sorted[sorted.len() / 2]
    }

    fn min(&self) -> Duration {
        *self.samples.iter().min().unwrap()
    }

    fn max(&self) -> Duration {
        *self.samples.iter().max().unwrap()
    }
}

/// Measure `f` `runs` times.
fn time<F: FnMut()>(label: impl Into<String>, runs: usize, mut f: F) -> Timing {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        f();
        samples.push(start.elapsed());
    }
    Timing {
        label: label.into(),
        samples,
    }
}

fn ms(d: Duration) -> String {
    format!("{:.1}ms", d.as_secs_f64() * 1000.0)
}

/// Run every measurement and exit. Never returns, for the same reason
/// `--selftest` does not: a benchmark that left a window open would hang a
/// scripted run.
pub fn run(app: &tauri::AppHandle) -> ! {
    let report = tauri::async_runtime::block_on(measure(app));

    println!("{report}");
    tracing::info!("\n{report}");

    // `process::exit` skips destructors, so the resident model would be left
    // behind holding a few hundred megabytes.
    if let Some(state) = app.try_state::<AppState>() {
        tauri::async_runtime::block_on(state.whisper_server.shutdown());
    }
    std::process::exit(0)
}

async fn measure(app: &tauri::AppHandle) -> String {
    let state = app.state::<AppState>();
    let mut out = String::from("echo --benchmark\n\n");

    let audio = tone(0.3, CLIP_SECONDS * SAMPLE_RATE);

    // ── Context ──────────────────────────────────────────────────────────────
    let model = {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::get_setting(&conn, "whisper_model")
            .unwrap_or(None)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| crate::core::asr::model_manager::DEFAULT_MODEL.to_string())
    };
    let dictionary_terms = state
        .dictionary
        .read()
        .await
        .prompt_terms(None)
        .map(|p| p.split(", ").count())
        .unwrap_or(0);

    let _ = writeln!(out, "  model      {model}");
    let _ = writeln!(out, "  compute    {}", state.binaries.gpu().label());
    let _ = writeln!(
        out,
        "  accel      {}",
        match state.binaries.active_gpu_pack() {
            Some(p) => p.label(),
            None => "not in use",
        }
    );
    let _ = writeln!(out, "  clip       {CLIP_SECONDS}s synthetic");
    let _ = writeln!(
        out,
        "\n  Engine overhead, not decode throughput: whisper pads anything under\n  \
         30s to a full window, so the encoder costs the same regardless of clip\n  \
         length, and a tone produces almost no decoder work.\n"
    );

    let mut timings: Vec<Timing> = Vec::new();

    // ── In-process stages ────────────────────────────────────────────────────
    timings.push(time("vad (energy)", MICRO_RUNS, || {
        let mut vad = EnergyVad::new(0.01);
        for chunk in audio.chunks(1_600) {
            std::hint::black_box(vad.is_speech(chunk));
        }
    }));

    timings.push(time("speech gate", MICRO_RUNS, || {
        std::hint::black_box(speech_gate(&audio));
    }));

    {
        let dictionary = state.dictionary.read().await;
        let sample = "deploy the cooper netties cluster to staging this afternoon";
        timings.push(time(
            format!("dictionary ({dictionary_terms} terms)"),
            MICRO_RUNS,
            || {
                std::hint::black_box(dictionary.process_for(sample, None));
            },
        ));
    }

    // ── Transcription engines ────────────────────────────────────────────────
    let asr_note = measure_asr(&state, &model, &audio, &mut timings).await;

    // ── Table ────────────────────────────────────────────────────────────────
    let width = timings.iter().map(|t| t.label.len()).max().unwrap_or(10).max(24);
    let _ = writeln!(
        out,
        "  {:<width$}  {:>9}  {:>9}  {:>9}  {:>5}",
        "stage", "median", "min", "max", "runs"
    );
    for t in &timings {
        let _ = writeln!(
            out,
            "  {:<width$}  {:>9}  {:>9}  {:>9}  {:>5}",
            t.label,
            ms(t.median()),
            ms(t.min()),
            ms(t.max()),
            t.samples.len()
        );
    }

    if let Some(note) = asr_note {
        let _ = write!(out, "\n{note}");
    }
    out
}

/// Time the CLI and the resident server against each other.
///
/// Returns the headline comparison, or an explanation of why there is none.
async fn measure_asr(
    state: &AppState,
    model: &str,
    audio: &[f32],
    timings: &mut Vec<Timing>,
) -> Option<String> {
    if !state.models.is_downloaded(model) {
        return Some(format!("  (no transcription timings: '{model}' is not downloaded)\n"));
    }
    let Some(cli_binary) = state.binaries.resolve() else {
        return Some("  (no transcription timings: whisper-cli is not installed)\n".into());
    };

    let model_path = state.models.model_path(model);
    let decode = DecodeConfig {
        threads: crate::core::asr::decode_opts::auto_threads(),
        use_gpu: state
            .binaries
            .active_dir()
            .map(|(_, accel)| accel)
            .unwrap_or(false),
    };
    let language = whisper_cli::resolve_language(model, Some("en"));
    let wav = match crate::core::asr::wav::pcm_f32_to_wav(audio, 16_000) {
        Ok(w) => w,
        Err(e) => return Some(format!("  (no transcription timings: {e})\n")),
    };

    // One unmeasured run first. The very first transcription after a build or
    // a cold boot competes with whatever is still finishing and reads the model
    // off disk rather than out of the page cache. Including it produced a
    // median almost twice every later run, and a headline figure that was
    // simply wrong.
    let _ = whisper_cli::run_cli(&cli_binary, &model_path, &wav, language, decode, None).await;

    // ── One-shot CLI: reloads the model every call, which is the cost the
    //    resident server exists to remove.
    let mut cli_samples = Vec::new();
    for _ in 0..ASR_RUNS {
        let start = Instant::now();
        let result =
            whisper_cli::run_cli(&cli_binary, &model_path, &wav, language, decode, None).await;
        if let Err(e) = result {
            return Some(format!("  (whisper-cli failed: {e})\n"));
        }
        cli_samples.push(start.elapsed());
    }
    let cli = Timing {
        label: "whisper-cli (cold each)".into(),
        samples: cli_samples,
    };
    let cli_median = cli.median();
    let cli_max = cli.max();
    timings.push(cli);

    // ── Resident server: first call pays the model load, the rest do not.
    let Some(server_binary) = state.binaries.resolve_server() else {
        timings.push(Timing {
            label: "whisper-server".into(),
            samples: vec![Duration::ZERO],
        });
        return Some("  (no server comparison: whisper-server is not installed)\n".into());
    };

    let sig = Signature {
        binary: server_binary,
        model: model_path.clone(),
        decode: DecodeConfigKey::from(decode),
    };

    // Force a genuine cold start so the first number means what it says.
    state.whisper_server.shutdown().await;

    let start = Instant::now();
    let cold_result = state
        .whisper_server
        .transcribe(&sig, wav.clone(), CLIP_SECONDS as u32, language, None)
        .await;
    let cold = start.elapsed();
    if let Err(e) = cold_result {
        return Some(format!("  (whisper-server failed: {e})\n"));
    }
    timings.push(Timing {
        label: "whisper-server (cold start)".into(),
        samples: vec![cold],
    });

    let mut warm_samples = Vec::new();
    for _ in 0..ASR_RUNS {
        let start = Instant::now();
        let result = state
            .whisper_server
            .transcribe(&sig, wav.clone(), CLIP_SECONDS as u32, language, None)
            .await;
        if let Err(e) = result {
            return Some(format!("  (whisper-server failed while warm: {e})\n"));
        }
        warm_samples.push(start.elapsed());
    }
    let warm = Timing {
        label: "whisper-server (warm)".into(),
        samples: warm_samples,
    };
    let warm_median = warm.median();
    let warm_max = warm.max();
    timings.push(warm);

    // A sample this scattered is measuring the machine's background load rather
    // than Echo. Say so instead of quoting a headline drawn from it — the first
    // run of this benchmark did exactly that and reported a 1.6x speed-up that
    // did not survive a second run.
    if warm_max > warm_median * 2 || cli_max > cli_median * 2 {
        return Some(
            "  These timings are too scattered to conclude anything from — something
               else on this machine was competing for the CPU. Re-run when it is idle.
"
                .into(),
        );
    }

    // The headline: what the resident model is worth per utterance.
    Some(if warm_median < cli_median {
        let saved = cli_median - warm_median;
        format!(
            "  Keeping the model resident saves {} per utterance after the first\n  \
             ({} -> {}, a {:.1}x speed-up).\n",
            ms(saved),
            ms(cli_median),
            ms(warm_median),
            cli_median.as_secs_f64() / warm_median.as_secs_f64().max(f64::EPSILON),
        )
    } else {
        // Worth saying plainly rather than hiding: on some machines the spawn
        // is cheap and the server buys nothing.
        format!(
            "  The resident server is NOT faster here ({} warm vs {} cold).\n  \
             Worth investigating before relying on it.\n",
            ms(warm_median),
            ms(cli_median),
        )
    })
}

/// A sine at `amp`, `samples` long at 16 kHz.
fn tone(amp: f32, samples: usize) -> Vec<f32> {
    (0..samples).map(|i| (i as f32 * 0.05).sin() * amp).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(ms_values: &[u64]) -> Timing {
        Timing {
            label: "t".into(),
            samples: ms_values.iter().map(|m| Duration::from_millis(*m)).collect(),
        }
    }

    #[test]
    fn median_ignores_a_single_outlier() {
        // The reason the report leads with a median: one scheduler hiccup must
        // not become the headline number.
        let t = timing(&[100, 102, 101, 103, 9_000]);
        assert_eq!(t.median(), Duration::from_millis(102));
        assert_eq!(t.min(), Duration::from_millis(100));
        assert_eq!(t.max(), Duration::from_millis(9_000));
    }

    #[test]
    fn a_single_sample_is_its_own_median() {
        let t = timing(&[250]);
        assert_eq!(t.median(), Duration::from_millis(250));
    }

    #[test]
    fn durations_render_at_one_decimal() {
        assert_eq!(ms(Duration::from_micros(1_234)), "1.2ms");
        assert_eq!(ms(Duration::from_millis(1_500)), "1500.0ms");
    }

    #[test]
    fn timing_records_one_sample_per_run() {
        let mut calls = 0;
        let t = time("x", 4, || calls += 1);
        assert_eq!(calls, 4);
        assert_eq!(t.samples.len(), 4);
    }
}
