//! A resident whisper.cpp model, behind whisper.cpp's own HTTP front-end.
//!
//! The CLI reloads the model from disk on every invocation. For dictation —
//! many short utterances, seconds apart — that load dominates the wall-clock
//! cost of transcribing, and it is pure waste: it is the same model every time.
//! `whisper-server` loads once and answers requests over loopback, so the
//! second and every later utterance skips it entirely.
//!
//! The process is supervised by a *signature*: the model, thread count, GPU
//! choice and binary that a running server was started with. A request whose
//! signature matches is served by the existing process; one that differs
//! restarts it. Without that, changing a setting would either be ignored or
//! would tear the server down on every single request.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::decode_opts::DecodeConfig;
use crate::error::{EchoError, Result};

/// How long to wait for a CPU server to bind its port. The model is read from
/// disk before it listens, so this covers a cold page cache on a slow disk.
const CPU_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// GPU startup additionally uploads weights to the device and compiles kernels
/// on first run, which is minutes-slow on some drivers rather than seconds.
const GPU_STARTUP_TIMEOUT: Duration = Duration::from_secs(120);

const READY_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Floor for a transcription request, plus [`TIMEOUT_PER_AUDIO_SECOND`] for
/// each second of audio. A fixed timeout either kills long dictations or lets a
/// wedged server hang a short one for far too long.
const BASE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TIMEOUT_PER_AUDIO_SECOND: u32 = 3;

/// Cap on retained stderr. Enough for a stack of whisper.cpp init lines, small
/// enough that a server looping on an error cannot grow it without bound.
const MAX_STDERR_BYTES: usize = 4096;

/// Everything about a server process that, if changed, requires a restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// The `whisper-server` executable — changes when a GPU pack is installed
    /// or when we fall back from an accelerated pack to the CPU one.
    pub binary: PathBuf,
    pub model: PathBuf,
    pub decode: DecodeConfigKey,
}

/// [`DecodeConfig`] reduced to the fields that affect the *process*, so that
/// per-request options never trigger a restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeConfigKey {
    pub threads: usize,
    pub use_gpu: bool,
}

impl From<DecodeConfig> for DecodeConfigKey {
    fn from(c: DecodeConfig) -> Self {
        Self {
            threads: c.threads,
            use_gpu: c.use_gpu,
        }
    }
}

struct Running {
    child: Child,
    port: u16,
    sig: Signature,
}

/// Supervises at most one `whisper-server` child process.
pub struct WhisperServer {
    running: Mutex<Option<Running>>,
    http: reqwest::Client,
}

impl Default for WhisperServer {
    fn default() -> Self {
        Self::new()
    }
}

impl WhisperServer {
    pub fn new() -> Self {
        Self {
            running: Mutex::new(None),
            http: reqwest::Client::new(),
        }
    }

    /// Transcribe one utterance, starting or restarting the server if needed.
    ///
    /// `wav` is a complete WAV file; `prompt` biases the decoder toward known
    /// vocabulary (see [`crate::core::dictionary::DictionaryEngine::prompt_terms`]).
    pub async fn transcribe(
        &self,
        sig: &Signature,
        wav: Vec<u8>,
        audio_seconds: u32,
        language: &str,
        prompt: Option<String>,
    ) -> Result<String> {
        let port = self.ensure(sig).await?;
        self.infer(port, wav, audio_seconds, language, prompt).await
    }

    /// Stop the server if one is running. Used when switching away from the
    /// local engine so an idle process is not left holding the model in RAM.
    pub async fn shutdown(&self) {
        if let Some(mut running) = self.running.lock().await.take() {
            let _ = running.child.kill().await;
        }
    }

    /// The port of a server matching `sig`, starting one if necessary.
    async fn ensure(&self, sig: &Signature) -> Result<u16> {
        let mut guard = self.running.lock().await;

        if let Some(running) = guard.as_mut() {
            // `try_wait` is what distinguishes "still serving" from "exited
            // while we weren't looking" — a crashed server leaves a struct
            // behind that otherwise looks perfectly healthy.
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
        language: &str,
        prompt: Option<String>,
    ) -> Result<String> {
        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("language", language.to_string())
            .text("response_format", "json")
            // Sent per request rather than baked into the process arguments:
            // they cost nothing here and keep the server's startup signature
            // free of decoder tuning, so changing a threshold never forces a
            // model reload.
            .text("entropy_thold", super::decode_opts::ENTROPY_THOLD)
            .text("logprob_thold", super::decode_opts::LOGPROB_THOLD);

        if let Some(prompt) = prompt {
            form = form.text("prompt", prompt);
        }

        let timeout =
            BASE_REQUEST_TIMEOUT + Duration::from_secs((audio_seconds * TIMEOUT_PER_AUDIO_SECOND) as u64);

        let resp = self
            .http
            .post(format!("http://127.0.0.1:{port}/inference"))
            .timeout(timeout)
            .multipart(form)
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(format!("whisper-server request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "whisper-server returned {status}: {}",
                body.trim()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(format!("whisper-server sent invalid JSON: {e}")))?;

        let text = body
            .get("text")
            .and_then(|t| t.as_str())
            .ok_or_else(|| EchoError::AsrProvider("whisper-server response had no text".into()))?;

        Ok(super::whisper_cli::clean_transcript(text))
    }
}

/// Spawn a server for `sig` and wait until it accepts connections.
async fn start(sig: &Signature) -> Result<Running> {
    let port = free_port()?;

    let decode = DecodeConfig {
        threads: sig.decode.threads,
        use_gpu: sig.decode.use_gpu,
    };

    let mut cmd = Command::new(&sig.binary);
    cmd.arg("-m")
        .arg(&sig.model)
        .args(["--host", "127.0.0.1"])
        .args(["--port", &port.to_string()])
        .args(decode.args())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        // The server must not outlive the app. Without this, a crash or a
        // force-quit leaves an orphan holding the model in RAM and the port.
        .kill_on_drop(true);

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| {
        EchoError::AsrProvider(format!(
            "failed to launch whisper-server at {}: {e}",
            sig.binary.display()
        ))
    })?;

    // Drain stderr continuously. Whisper writes its startup banner and any load
    // error there, and an undrained pipe would eventually block the child.
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

    let timeout = if sig.decode.use_gpu {
        GPU_STARTUP_TIMEOUT
    } else {
        CPU_STARTUP_TIMEOUT
    };

    match wait_until_ready(&mut child, port, timeout).await {
        Ok(()) => {
            tracing::info!(
                port,
                gpu = sig.decode.use_gpu,
                threads = sig.decode.threads,
                model = %sig.model.display(),
                "whisper-server ready"
            );
            Ok(Running {
                child,
                port,
                sig: sig.clone(),
            })
        }
        Err(e) => {
            let _ = child.kill().await;
            let detail = stderr
                .lock()
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "no output".into());
            Err(EchoError::AsrProvider(format!("{e}: {detail}")))
        }
    }
}

/// Poll until the server accepts a connection, it exits, or we run out of time.
async fn wait_until_ready(child: &mut Child, port: u16, timeout: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        // A server that has exited will never bind, so check this first —
        // otherwise a bad model path costs the full startup timeout before the
        // CPU fallback gets its turn.
        if let Ok(Some(status)) = child.try_wait() {
            return Err(EchoError::AsrProvider(format!(
                "whisper-server exited during startup with {status}"
            )));
        }

        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return Ok(());
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(EchoError::AsrProvider(format!(
                "whisper-server did not start within {}s",
                timeout.as_secs()
            )));
        }

        tokio::time::sleep(READY_POLL_INTERVAL).await;
    }
}

/// Ask the OS for an unused loopback port.
///
/// There is an unavoidable gap between releasing this and the server binding
/// it. Losing that race is rare, self-announcing (the child exits immediately
/// with "bind failed"), and recovered by the caller's fallback — which is a
/// better trade than scanning a hardcoded range and colliding with whatever
/// else the user happens to be running.
fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| EchoError::AsrProvider(format!("could not reserve a port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| EchoError::AsrProvider(e.to_string()))?
        .port();
    drop(listener);
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(threads: usize, use_gpu: bool) -> Signature {
        Signature {
            binary: PathBuf::from("whisper-server"),
            model: PathBuf::from("base.en.bin"),
            decode: DecodeConfigKey { threads, use_gpu },
        }
    }

    #[test]
    fn identical_settings_reuse_the_same_server() {
        assert_eq!(sig(4, true), sig(4, true));
    }

    #[test]
    fn changing_a_process_level_setting_forces_a_restart() {
        assert_ne!(sig(4, true), sig(8, true));
        assert_ne!(sig(4, true), sig(4, false));

        let mut other_model = sig(4, true);
        other_model.model = PathBuf::from("small.en.bin");
        assert_ne!(sig(4, true), other_model);

        // Swapping to a GPU pack's binary must restart even when nothing else moved.
        let mut other_binary = sig(4, true);
        other_binary.binary = PathBuf::from("cuda12/whisper-server");
        assert_ne!(sig(4, true), other_binary);
    }

    #[test]
    fn free_port_returns_something_bindable() {
        let port = free_port().unwrap();
        assert!(port > 0);
        // Released back to the OS, so it must be bindable again right away.
        std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    }
}
