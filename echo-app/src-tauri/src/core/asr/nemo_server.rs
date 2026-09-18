//! Supervises `nemo-speech serve` and talks to it over HTTP.
//!
//! Same shape as [`super::whisper_server`], and for the same reason: loading a
//! 700 MB model takes seconds, so the process stays resident between
//! utterances and only restarts when the model or the device changes.
//!
//! The differences from the whisper server are all in the protocol. Readiness
//! is a real `/health` endpoint rather than a bare TCP accept, transcription is
//! the OpenAI-compatible `/v1/audio/transcriptions`, and there is no decoder
//! tuning to pass: a transducer does not have whisper's temperature-fallback
//! loop, which is also why it does not hallucinate sentences into silence.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::whisper_server::{free_port, spawn_contained};
use crate::error::{EchoError, Result};

/// How long to wait for the server to answer `/health` after launch.
///
/// Generous because this covers reading the weights off a cold disk, and on a
/// GPU it also covers CUDA context creation.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Request timeout, scaled the same way the whisper server's is.
const BASE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TIMEOUT_PER_AUDIO_SECOND: u32 = 2;

/// Cap on retained stderr, so a server that fails in a loop cannot grow the
/// error message without bound.
const MAX_STDERR_BYTES: usize = 8 * 1024;

/// What a running server was started for. A request that does not match it
/// restarts the process — the model and the device are baked in at launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub binary: PathBuf,
    pub model: PathBuf,
    pub device: Device,
}

/// Which backend the server should run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Auto,
    Cpu,
}

impl Device {
    fn as_arg(self) -> &'static str {
        match self {
            Device::Auto => "auto",
            Device::Cpu => "cpu",
        }
    }
}

struct Running {
    child: Child,
    port: u16,
    sig: Signature,
}

pub struct NemoServer {
    running: Mutex<Option<Running>>,
    http: reqwest::Client,
}

impl Default for NemoServer {
    fn default() -> Self {
        Self::new()
    }
}

impl NemoServer {
    pub fn new() -> Self {
        Self {
            running: Mutex::new(None),
            http: reqwest::Client::new(),
        }
    }

    /// Transcribe one utterance, starting the server if it is not up.
    pub async fn transcribe(
        &self,
        sig: &Signature,
        wav: Vec<u8>,
        audio_seconds: u32,
        language: Option<&str>,
    ) -> Result<String> {
        let port = self.ensure(sig).await?;
        self.infer(port, wav, audio_seconds, language).await
    }

    /// Start the server for `sig` without decoding anything, so the weights are
    /// resident before the first utterance needs them.
    pub async fn warm(&self, sig: &Signature) -> Result<()> {
        self.ensure(sig).await.map(|_| ())
    }

    /// Stop the server, releasing the model from memory. Used when switching
    /// away from this engine.
    pub async fn shutdown(&self) {
        if let Some(mut running) = self.running.lock().await.take() {
            let _ = running.child.kill().await;
        }
    }

    async fn ensure(&self, sig: &Signature) -> Result<u16> {
        let mut guard = self.running.lock().await;

        if let Some(running) = guard.as_mut() {
            // `try_wait` distinguishes "still serving" from "exited while we
            // weren't looking": a crashed server leaves a struct that looks
            // perfectly healthy from here.
            let alive = matches!(running.child.try_wait(), Ok(None));
            if alive && running.sig == *sig {
                return Ok(running.port);
            }
            let _ = running.child.kill().await;
            *guard = None;
        }

        let running = start(sig).await?;
        let port = running.port;
        *guard = Some(running);
        Ok(port)
    }

    async fn infer(
        &self,
        port: u16,
        wav: Vec<u8>,
        audio_seconds: u32,
        language: Option<&str>,
    ) -> Result<String> {
        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("response_format", "json");
        if let Some(language) = language {
            form = form.text("language", language.to_string());
        }

        let timeout = BASE_REQUEST_TIMEOUT
            + Duration::from_secs((audio_seconds * TIMEOUT_PER_AUDIO_SECOND) as u64);

        let resp = self
            .http
            .post(format!("http://127.0.0.1:{port}/v1/audio/transcriptions"))
            .timeout(timeout)
            .multipart(form)
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(format!("nemo-speech request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "nemo-speech returned {status}: {}",
                body.trim()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(format!("nemo-speech sent invalid JSON: {e}")))?;

        let text = body
            .get("text")
            .and_then(|t| t.as_str())
            .ok_or_else(|| EchoError::AsrProvider("nemo-speech response had no text".into()))?;

        Ok(super::whisper_cli::clean_transcript(text))
    }
}

/// Spawn a server for `sig` and wait until `/health` answers.
async fn start(sig: &Signature) -> Result<Running> {
    let port = free_port()?;

    let mut cmd = Command::new(&sig.binary);
    cmd.arg("serve")
        .arg("--asr-model")
        .arg(&sig.model)
        .args(["--host", "127.0.0.1"])
        .args(["--port", &port.to_string()])
        .args(["--device", sig.device.as_arg()])
        // Nothing here drives a browser, and the playground would serve a UI
        // on a port the user never asked to open.
        .arg("--no-ui")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = spawn_contained(cmd).await.map_err(|e| {
        EchoError::AsrProvider(format!(
            "failed to launch nemo-speech at {}: {e}",
            sig.binary.display()
        ))
    })?;

    // Drain stderr continuously: the model banner and any load failure go
    // there, and an undrained pipe eventually blocks the child.
    let stderr = Arc::new(StdMutex::new(String::new()));
    if let Some(pipe) = child.stderr.take() {
        let sink = stderr.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(pipe).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(mut buf) = sink.lock() {
                    if buf.len() < MAX_STDERR_BYTES {
                        buf.push_str(&line);
                        buf.push('\n');
                    }
                }
            }
        });
    }

    match wait_until_healthy(&mut child, port).await {
        Ok(()) => {
            tracing::info!(
                port,
                device = sig.device.as_arg(),
                model = %sig.model.display(),
                "nemo-speech ready"
            );
            Ok(Running {
                child,
                port,
                sig: sig.clone(),
            })
        }
        Err(e) => {
            let _ = child.kill().await;
            let log = stderr.lock().map(|s| s.clone()).unwrap_or_default();
            Err(EchoError::AsrProvider(if log.trim().is_empty() {
                e.to_string()
            } else {
                format!("{e}\n{}", log.trim())
            }))
        }
    }
}

/// Poll `/health` until the server answers, the child exits, or time runs out.
async fn wait_until_healthy(child: &mut Child, port: u16) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/health");
    let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;

    loop {
        // A model path that does not load fails in a second; without this the
        // caller would wait out the whole startup timeout to hear about it.
        if let Ok(Some(status)) = child.try_wait() {
            return Err(EchoError::AsrProvider(format!(
                "nemo-speech exited during startup ({status})"
            )));
        }

        if let Ok(resp) = client
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            if resp.status().is_success() {
                return Ok(());
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(EchoError::AsrProvider(format!(
                "nemo-speech did not become ready within {}s",
                STARTUP_TIMEOUT.as_secs()
            )));
        }
        tokio::time::sleep(READY_POLL_INTERVAL).await;
    }
}

/// Whether `dir` holds a runnable nemo-speech binary.
pub fn binary_in(dir: &Path) -> Option<PathBuf> {
    let candidate = dir.join(super::nemo::BINARY_NAME);
    candidate.exists().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A signature is what decides whether the resident model can be reused, so
    /// every field in it has to count.
    #[test]
    fn a_different_model_or_device_is_a_different_server() {
        let base = Signature {
            binary: PathBuf::from("nemo-speech.exe"),
            model: PathBuf::from("a.gguf"),
            device: Device::Auto,
        };
        assert_eq!(base, base.clone());
        assert_ne!(
            base,
            Signature {
                model: PathBuf::from("b.gguf"),
                ..base.clone()
            }
        );
        assert_ne!(
            base,
            Signature {
                device: Device::Cpu,
                ..base.clone()
            }
        );
    }

    #[tokio::test]
    async fn a_missing_binary_fails_instead_of_hanging() {
        let err = start(&Signature {
            binary: PathBuf::from("definitely-not-a-real-nemo-speech-binary"),
            model: PathBuf::from("none.gguf"),
            device: Device::Cpu,
        })
        .await
        .err()
        .expect("a binary that does not exist cannot start");
        assert!(
            format!("{err}").contains("failed to launch nemo-speech"),
            "{err}"
        );
    }
}
