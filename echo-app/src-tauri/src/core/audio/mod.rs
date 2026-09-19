//! Microphone capture, and the two things that make dictation feel instant:
//! keeping the device open between utterances, and keeping the half-second
//! before you pressed the key.
//!
//! Opening a capture device is slow — tens of milliseconds on a good day, and
//! seconds on a cold USB interface. Doing that *after* the hotkey means the
//! first syllable is already gone by the time the stream delivers its first
//! buffer, which is why dictation apps are so often accused of "eating the
//! first word". [`AudioService::warm`] opens the device ahead of time and keeps
//! a rolling [`PRE_ROLL`] of audio, so when recording actually starts the
//! utterance begins slightly *before* the key press rather than after it.
//!
//! The device callback never makes a policy decision. It pushes every buffer
//! into one channel, and a router task decides whether that audio is discarded,
//! kept as pre-roll, or forwarded to a live recording. That keeps the real-time
//! audio thread free of locks it could block on.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::error::{EchoError, Result};

/// How much audio to keep from before recording starts.
///
/// Long enough to recover a word begun a moment early, short enough that the
/// decoder is not handed a wall of silence to chew through on every utterance.
const PRE_ROLL: Duration = Duration::from_millis(500);
const PRE_ROLL_SAMPLES: usize = (16_000 * PRE_ROLL.as_millis() as usize) / 1000;

/// How long a warmed microphone stays open before it is released.
///
/// This is a privacy control as much as a resource one: an open capture device
/// lights the OS "microphone in use" indicator, and leaving that on
/// indefinitely because the user *might* dictate later would be indefensible.
const WARM_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct AudioDevice {
    pub name: String,
    pub is_default: bool,
}

/// What the router should do with incoming audio.
enum Mode {
    /// Discard. The device is open but nothing wants the audio yet.
    Idle,
    /// Keep only the most recent [`PRE_ROLL_SAMPLES`].
    Warm(VecDeque<f32>),
    /// Forward to a live recording.
    Active(mpsc::Sender<Vec<f32>>),
}

enum Cmd {
    /// A newly opened device's buffers arrive here.
    Attach(mpsc::Receiver<Vec<f32>>),
    /// Collect pre-roll, but never interrupt a recording in progress.
    Warm,
    /// End the recording and collect pre-roll. Unlike [`Cmd::Warm`] this
    /// *does* apply while active: it is how a finished dictation releases the
    /// channel the ASR stage is reading.
    EndCapture,
    Start(mpsc::Sender<Vec<f32>>),
    Stop,
}

struct OpenStream {
    stream: Stream,
    /// The device this stream was opened on, so a later request for the *same*
    /// device can reuse it and a request for a different one reopens.
    device_name: Option<String>,
    /// Cleared by the stream's error callback. A stream that has faulted still
    /// looks perfectly alive from the outside — it simply stops delivering
    /// buffers — so reuse must be gated on this rather than on its existence.
    healthy: Arc<AtomicBool>,
}

pub struct AudioService {
    host: cpal::Host,
    open: Mutex<Option<OpenStream>>,
    cmd_tx: mpsc::UnboundedSender<Cmd>,
    /// Bumped whenever the warm window is renewed or consumed, so a pending
    /// expiry timer from an earlier warm-up cannot close a newer stream.
    warm_generation: Arc<AtomicU64>,
}

// Stream is not Send but we manage it behind a Mutex and never share the reference.
unsafe impl Send for AudioService {}
unsafe impl Sync for AudioService {}

impl AudioService {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        // `tauri::async_runtime::spawn`, not `tokio::spawn`: this is built in
        // Tauri's `setup` hook, which is not inside a Tokio runtime context —
        // a bare `tokio::spawn` there panics with "no reactor running" before
        // the window ever appears.
        tauri::async_runtime::spawn(route(cmd_rx));
        Ok(Self {
            host,
            open: Mutex::new(None),
            cmd_tx,
            warm_generation: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn list_input_devices(&self) -> Result<Vec<AudioDevice>> {
        let default_name: Option<String> =
            self.host.default_input_device().and_then(|d| d.name().ok());

        let devices: Vec<AudioDevice> = self
            .host
            .input_devices()
            .map_err(|e| EchoError::AudioDevice(e.to_string()))?
            .filter_map(|d| d.name().ok())
            .map(|name| {
                let is_default = default_name.as_deref() == Some(name.as_str());
                AudioDevice { name, is_default }
            })
            .collect();

        Ok(devices)
    }

    /// Open the microphone ahead of a recording and start collecting pre-roll.
    ///
    /// Safe to call repeatedly — an already-open, healthy stream on the same
    /// device just has its warm window renewed. Failure is deliberately not an
    /// error the caller has to handle: warming is an optimisation, and a
    /// machine that cannot warm can still record.
    pub fn warm(self: &Arc<Self>, device_name: Option<&str>) {
        self.warm_with(device_name, Cmd::Warm)
    }

    fn warm_with(self: &Arc<Self>, device_name: Option<&str>, cmd: Cmd) {
        if let Err(e) = self.ensure_stream(device_name) {
            warn!("Could not warm the microphone: {e}");
            return;
        }
        let _ = self.cmd_tx.send(cmd);

        // Release the device if the recording we warmed for never arrives.
        let generation = self.warm_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let gen_handle = self.warm_generation.clone();
        // The service itself, because stopping the *router* is not releasing
        // the *device*: the capture stream has to be dropped, or the OS goes on
        // showing the microphone as in use for as long as Echo is open — while
        // this very task logs that it was released.
        let service = self.clone();
        // Same reasoning as in `new`: `warm` is reachable from a synchronous
        // Tauri command, which does not run inside a Tokio runtime either.
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(WARM_TIMEOUT).await;
            // A newer warm-up or a started recording has superseded this timer.
            if gen_handle.load(Ordering::SeqCst) != generation {
                return;
            }
            info!("Warm microphone expired; releasing the device");
            let _ = service.cmd_tx.send(Cmd::Stop);
            service.close_stream();
        });
    }

    /// Start capturing from the named device (or default if None).
    /// Returns a receiver of PCM chunks (f32, mono, 16 kHz), beginning with
    /// whatever pre-roll a prior [`Self::warm`] collected.
    pub fn start_capture(&self, device_name: Option<&str>) -> Result<mpsc::Receiver<Vec<f32>>> {
        // Claim the warm window so its expiry timer cannot close the device
        // out from under the recording we are about to start.
        self.warm_generation.fetch_add(1, Ordering::SeqCst);

        self.ensure_stream(device_name)?;

        let (tx, rx) = mpsc::channel::<Vec<f32>>(256);
        self.cmd_tx
            .send(Cmd::Start(tx))
            .map_err(|_| EchoError::AudioDevice("audio router stopped".into()))?;

        info!("Audio capture started");
        Ok(rx)
    }

    /// Stop delivering audio and release the device.
    pub fn stop_capture(&self) {
        self.warm_generation.fetch_add(1, Ordering::SeqCst);
        let _ = self.cmd_tx.send(Cmd::Stop);
        self.close_stream();
    }

    /// Stop the recording but keep the device open and collecting pre-roll, so
    /// a follow-up utterance starts instantly.
    ///
    /// This is the common case after a dictation: people dictate in bursts, and
    /// paying the device-open cost between every sentence is the difference
    /// between Echo feeling instant and feeling sluggish.
    pub fn stop_capture_warm(self: &Arc<Self>, device_name: Option<&str>) {
        self.warm_with(device_name, Cmd::EndCapture);
    }

    /// Ensure a healthy stream is open on `device_name`, reopening if the
    /// request names a different device or the current one has faulted.
    fn ensure_stream(&self, device_name: Option<&str>) -> Result<()> {
        {
            let open = self.open.lock().unwrap();
            if let Some(current) = open.as_ref() {
                let same_device = current.device_name.as_deref() == device_name;
                if same_device && current.healthy.load(Ordering::SeqCst) {
                    return Ok(());
                }
            }
        }
        self.close_stream();
        self.open_stream(device_name)
    }

    fn open_stream(&self, device_name: Option<&str>) -> Result<()> {
        let (device, resolved_name) = self.select_device(device_name)?;
        let config = self.build_config(&device)?;

        let (raw_tx, raw_rx) = mpsc::channel::<Vec<f32>>(256);
        let tx_err = raw_tx.clone();
        let healthy = Arc::new(AtomicBool::new(true));
        let healthy_err = healthy.clone();

        let channels = config.channels;
        let source_rate = config.sample_rate.0;
        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _| {
                    let chunk = resample_to_16k(data, source_rate, channels);
                    let _ = raw_tx.try_send(chunk);
                },
                move |err| {
                    error!("Audio stream error: {err}");
                    // Mark the stream unusable so it is reopened rather than
                    // silently reused as a device that delivers nothing.
                    healthy_err.store(false, Ordering::SeqCst);
                    let _ = tx_err.try_send(vec![]); // signal downstream
                },
                None,
            )
            .map_err(|e| EchoError::AudioDevice(e.to_string()))?;

        stream
            .play()
            .map_err(|e| EchoError::AudioDevice(e.to_string()))?;

        self.cmd_tx
            .send(Cmd::Attach(raw_rx))
            .map_err(|_| EchoError::AudioDevice("audio router stopped".into()))?;

        *self.open.lock().unwrap() = Some(OpenStream {
            stream,
            device_name: device_name.map(str::to_owned),
            healthy,
        });
        info!(device = %resolved_name, "Microphone open");
        Ok(())
    }

    fn close_stream(&self) {
        if let Ok(mut guard) = self.open.lock() {
            if let Some(open) = guard.take() {
                drop(open.stream);
                info!("Audio capture stopped");
            }
        }
    }

    /// Resolve a device by name, falling back to the system default.
    ///
    /// The fallback is the point: settings store a device *name*, and names go
    /// away — a headset is unplugged, a dock is disconnected, a driver renames
    /// itself after an update. Refusing to record in that case leaves the user
    /// with a dead hotkey and nothing on screen to explain it, when the machine
    /// has a perfectly good default microphone.
    fn select_device(&self, name: Option<&str>) -> Result<(Device, String)> {
        if let Some(wanted) = name.filter(|n| !n.is_empty()) {
            let found = self
                .host
                .input_devices()
                .map_err(|e| EchoError::AudioDevice(e.to_string()))?
                .find(|d| d.name().ok().as_deref() == Some(wanted));

            match found {
                Some(device) => return Ok((device, wanted.to_string())),
                None => warn!("Microphone '{wanted}' is not available; using the system default"),
            }
        }

        let device = self
            .host
            .default_input_device()
            .ok_or_else(|| EchoError::AudioDevice("No input device is available".into()))?;
        let resolved = device.name().unwrap_or_else(|_| "default".into());
        Ok((device, resolved))
    }

    fn build_config(&self, device: &Device) -> Result<StreamConfig> {
        let supported = device
            .default_input_config()
            .map_err(|e| EchoError::AudioDevice(e.to_string()))?;

        // We always capture f32 and handle format conversion ourselves.
        let config = StreamConfig {
            channels: supported.channels(),
            sample_rate: supported.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        Ok(config)
    }
}

/// Routes captured audio according to the current [`Mode`].
///
/// Runs for the life of the app. Keeping it separate from the device callback
/// is what lets the callback stay a single non-blocking `try_send`.
async fn route(mut cmd_rx: mpsc::UnboundedReceiver<Cmd>) {
    let mut mode = Mode::Idle;
    let mut raw_rx: Option<mpsc::Receiver<Vec<f32>>> = None;

    loop {
        tokio::select! {
            // Commands win over audio so a stop takes effect promptly even
            // while buffers are streaming in.
            biased;

            cmd = cmd_rx.recv() => match cmd {
                None => return, // app shutting down
                Some(Cmd::Attach(rx)) => raw_rx = Some(rx),
                Some(Cmd::Warm) => {
                    if !matches!(mode, Mode::Active(_)) {
                        mode = Mode::Warm(VecDeque::with_capacity(PRE_ROLL_SAMPLES));
                    }
                }
                // Dropping the `Active` sender here is the point: it closes the
                // channel the ASR stage reads, which is what tells it the
                // utterance is over. Leaving the recording open until the warm
                // window expired delayed every transcript by `WARM_TIMEOUT`.
                Some(Cmd::EndCapture) => {
                    mode = Mode::Warm(VecDeque::with_capacity(PRE_ROLL_SAMPLES))
                }
                Some(Cmd::Start(tx)) => {
                    // Hand over the pre-roll first so the utterance includes
                    // the moment before the key press.
                    if let Mode::Warm(ring) = &mut mode {
                        if !ring.is_empty() {
                            let pre_roll: Vec<f32> = ring.drain(..).collect();
                            let _ = tx.try_send(pre_roll);
                        }
                    }
                    mode = Mode::Active(tx);
                }
                Some(Cmd::Stop) => mode = Mode::Idle,
            },

            Some(chunk) = async { match raw_rx.as_mut() {
                Some(rx) => rx.recv().await,
                None => None,
            } }, if raw_rx.is_some() => {
                match &mut mode {
                    Mode::Idle => {}
                    Mode::Warm(ring) => {
                        // An empty chunk is the device-error sentinel; there is
                        // no recording to notify, so just stop accumulating.
                        if chunk.is_empty() {
                            ring.clear();
                        } else {
                            push_pre_roll(ring, &chunk);
                        }
                    }
                    Mode::Active(tx) => match tx.try_send(chunk) {
                        Ok(()) => {}
                        // Backlog, not an ending: the ASR stage blocks while it
                        // decodes, and a slow decode used to look exactly like
                        // a finished recording — capture stopped mid-sentence
                        // and the user went on talking into nothing.
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            warn!("Audio backlog: dropped a chunk while the decoder caught up")
                        }
                        // Receiver gone: the recording ended without a Stop.
                        Err(mpsc::error::TrySendError::Closed(_)) => mode = Mode::Idle,
                    },
                }
            }
        }
    }
}

/// Append `chunk` to the pre-roll ring, dropping the oldest samples so it never
/// holds more than [`PRE_ROLL_SAMPLES`].
fn push_pre_roll(ring: &mut VecDeque<f32>, chunk: &[f32]) {
    // A single buffer larger than the whole window: keep only its tail.
    if chunk.len() >= PRE_ROLL_SAMPLES {
        ring.clear();
        ring.extend(&chunk[chunk.len() - PRE_ROLL_SAMPLES..]);
        return;
    }
    let overflow = (ring.len() + chunk.len()).saturating_sub(PRE_ROLL_SAMPLES);
    ring.drain(..overflow);
    ring.extend(chunk);
}

/// Chunk RMS the gain control aims for: ordinary speech into a well-set
/// microphone.
const AGC_TARGET_RMS: f32 = 0.05;

/// Most the gain control will amplify (20 dB). Enough to bring a line-level or
/// unamplified input up to a normal level, and low enough that a room's noise
/// floor stays well under what [`crate::core::vad::gate`] calls speech.
///
/// ponytail: one fixed ceiling for every microphone. Per-device calibration is
/// the upgrade if a very quiet input still needs more.
const AGC_MAX_GAIN: f32 = 10.0;

/// How fast the level estimate follows audio louder than it (per chunk).
const AGC_ATTACK: f32 = 0.5;
/// …and quieter than it. Slower, so the gain does not pump between syllables,
/// but quick enough to be settled by the time the pre-roll gives way to speech.
const AGC_RELEASE: f32 = 0.05;

/// Automatic gain control over captured audio.
///
/// Speech into a quiet input arrives 30-40 dB below full scale, where the
/// meter looks dead, the neural VAD is working near its limits, and whisper
/// loses accuracy. Browser-based dictation gets this for free from WebRTC's
/// AGC; Echo captures raw, so it does its own — once, in the capture path, so
/// the meter, speech detection and the decoder all see the same lifted audio.
///
/// Only ever amplifies: audio already at a healthy level passes through
/// untouched.
pub(crate) struct Agc {
    /// Running estimate of the input's level, in RMS.
    envelope: f32,
}

impl Agc {
    pub(crate) fn new() -> Self {
        // Start at unity. The first chunks of pre-roll settle it before the
        // speaker reaches a word.
        Self {
            envelope: AGC_TARGET_RMS,
        }
    }

    /// Amplify one chunk in place and return the gain applied.
    pub(crate) fn apply(&mut self, samples: &mut [f32]) -> f32 {
        if samples.is_empty() {
            return 1.0;
        }
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        let k = if rms > self.envelope {
            AGC_ATTACK
        } else {
            AGC_RELEASE
        };
        self.envelope += (rms - self.envelope) * k;

        let gain = (AGC_TARGET_RMS / self.envelope.max(f32::EPSILON)).clamp(1.0, AGC_MAX_GAIN);
        if gain > 1.0 {
            for s in samples.iter_mut() {
                *s = (*s * gain).clamp(-1.0, 1.0);
            }
        }
        gain
    }
}

/// A channel this far (20 dB) below the loudest one in a buffer is treated as
/// dead and left out of the down-mix.
const DEAD_CHANNEL_ENERGY_RATIO: f32 = 0.01;

/// Down-mix interleaved frames to mono and resample to 16000 Hz.
/// `channels` is the source channel count so mono (1ch) input is not corrupted.
fn resample_to_16k(data: &[f32], source_rate: u32, channels: u16) -> Vec<f32> {
    let mono: Vec<f32> = if channels <= 1 {
        data.to_vec()
    } else {
        // Average only the channels carrying signal. A mono mic in a stereo
        // jack (a desktop's rear "Line In" is the usual case) arrives on one
        // channel with the other silent, and a plain average halves it —
        // soft speech then falls under every threshold downstream.
        let ch = channels as usize;
        let mut energy = vec![0.0_f32; ch];
        for frame in data.chunks_exact(ch) {
            for (e, s) in energy.iter_mut().zip(frame) {
                *e += s * s;
            }
        }
        let loudest = energy.iter().copied().fold(0.0, f32::max);
        let live: Vec<bool> = energy
            .iter()
            .map(|e| *e >= loudest * DEAD_CHANNEL_ENERGY_RATIO)
            .collect();
        let live_count = live.iter().filter(|l| **l).count().max(1) as f32;
        data.chunks_exact(ch)
            .map(|frame| {
                frame
                    .iter()
                    .zip(&live)
                    .filter(|(_, l)| **l)
                    .map(|(s, _)| s)
                    .sum::<f32>()
                    / live_count
            })
            .collect()
    };

    if source_rate == 16000 {
        return mono;
    }

    // Each output sample is the mean of the source samples it spans. A box
    // filter is a crude low-pass, but it stops everything above 8 kHz (hiss,
    // sibilance) folding back into the speech band, which picking every Nth
    // sample does not.
    let ratio = source_rate as f64 / 16000.0;
    let out_len = (mono.len() as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let end = (((i + 1) as f64 * ratio) as usize).min(mono.len());
        let start = ((i as f64 * ratio) as usize).min(end.saturating_sub(1));
        let span = &mono[start..end];
        out.push(if span.is_empty() {
            0.0
        } else {
            span.iter().sum::<f32>() / span.len() as f32
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_passthrough_16k() {
        // Mono input already at 16 kHz should pass through unchanged.
        let input = vec![0.1, -0.2, 0.3, -0.4];
        let out = resample_to_16k(&input, 16000, 1);
        assert_eq!(out, input);
    }

    #[test]
    fn resample_48k_to_16k_length() {
        // 48 kHz mono → 16 kHz should be ~1/3 the length.
        let input: Vec<f32> = (0..480).map(|i| (i as f32) * 0.001).collect();
        let out = resample_to_16k(&input, 48000, 1);
        // ratio = 3.0, ceil(480 / 3) = 160
        assert_eq!(out.len(), 160);
    }

    #[test]
    fn mono_downmix() {
        // Interleaved stereo [L, R, L, R] is averaged into mono.
        let stereo = vec![1.0, 3.0, 2.0, 4.0];
        let out = resample_to_16k(&stereo, 16000, 2);
        assert_eq!(out, vec![2.0, 3.0]); // (1+3)/2, (2+4)/2
    }

    #[test]
    fn a_silent_channel_does_not_halve_the_signal() {
        // Mono mic on the left of a stereo jack, right channel dead.
        let stereo = vec![0.5, 0.0, -0.5, 0.0001];
        let out = resample_to_16k(&stereo, 16000, 2);
        assert_eq!(out, vec![0.5, -0.5]);
    }

    #[test]
    fn downsampling_averages_instead_of_skipping() {
        // A 24 kHz tone at 48 kHz (+1, -1, ...) is inaudible to a 16 kHz
        // decoder and must not alias into it as a full-scale signal.
        let input: Vec<f32> = (0..480)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let out = resample_to_16k(&input, 48000, 1);
        assert!(out.iter().all(|s| s.abs() <= 1.0 / 3.0 + 1e-6), "{out:?}");
    }

    #[test]
    fn pre_roll_keeps_the_most_recent_audio() {
        let mut ring = VecDeque::new();
        // Fill well past the window, in realistic-sized buffers.
        for block in 0..100 {
            let chunk: Vec<f32> = (0..1024).map(|i| (block * 1024 + i) as f32).collect();
            push_pre_roll(&mut ring, &chunk);
        }
        assert_eq!(ring.len(), PRE_ROLL_SAMPLES);

        // The window must end at the newest sample — pre-roll that lags behind
        // real time would reinstate the very clipping it exists to prevent.
        let newest = (100 * 1024 - 1) as f32;
        assert_eq!(*ring.back().unwrap(), newest);
        assert_eq!(
            *ring.front().unwrap(),
            newest - (PRE_ROLL_SAMPLES - 1) as f32
        );
    }

    #[test]
    fn pre_roll_handles_a_buffer_bigger_than_the_window() {
        let mut ring = VecDeque::new();
        let huge: Vec<f32> = (0..PRE_ROLL_SAMPLES * 3).map(|i| i as f32).collect();
        push_pre_roll(&mut ring, &huge);
        assert_eq!(ring.len(), PRE_ROLL_SAMPLES);
        assert_eq!(*ring.back().unwrap(), (PRE_ROLL_SAMPLES * 3 - 1) as f32);
    }

    /// Ending a recording has to close the channel the ASR stage reads. While
    /// `Cmd::Warm` was used for this it was ignored as long as a recording was
    /// active, so every transcript waited out the whole warm window.
    #[tokio::test]
    async fn ending_capture_closes_the_recording_channel_at_once() {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        tokio::spawn(route(cmd_rx));

        let (raw_tx, raw_rx) = mpsc::channel::<Vec<f32>>(8);
        cmd_tx.send(Cmd::Attach(raw_rx)).unwrap();
        let (rec_tx, mut rec_rx) = mpsc::channel::<Vec<f32>>(8);
        cmd_tx.send(Cmd::Start(rec_tx)).unwrap();

        raw_tx.send(vec![0.5; 160]).await.unwrap();
        assert_eq!(rec_rx.recv().await, Some(vec![0.5; 160]));

        cmd_tx.send(Cmd::EndCapture).unwrap();
        // Keep the device delivering: only the recording ends, not capture.
        raw_tx.send(vec![0.5; 160]).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), rec_rx.recv()).await,
            Ok(None),
            "the recording channel must close when capture ends"
        );
    }

    #[test]
    fn pre_roll_below_the_window_keeps_everything() {
        let mut ring = VecDeque::new();
        push_pre_roll(&mut ring, &[1.0, 2.0, 3.0]);
        assert_eq!(
            ring.iter().copied().collect::<Vec<_>>(),
            vec![1.0, 2.0, 3.0]
        );
    }
}
