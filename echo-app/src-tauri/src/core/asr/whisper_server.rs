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
//!
//! The server must also never outlive Echo. It holds the model in RAM, which is
//! gigabytes for the larger ones, and a copy orphaned by every crash adds up
//! fast now that crash recovery makes relaunching after one routine. An orderly
//! exit is handled by [`WhisperServer::shutdown`]; the rest is
//! [`spawn_contained`].

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
        // Once per process, before this process has started a server of its
        // own, so anything the sweep finds is a leftover and never ours.
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            static SWEEP: std::sync::Once = std::sync::Once::new();
            SWEEP.call_once(pidfile::sweep);
        }
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
            .text("logprob_thold", super::decode_opts::LOGPROB_THOLD)
            // Per request, not a startup flag: the right context depends on how
            // long *this* utterance is, and putting it in the server's
            // signature would restart the model on every sentence.
            .text(
                "audio_ctx",
                super::decode_opts::audio_ctx_for(audio_seconds).to_string(),
            );

        if let Some(prompt) = prompt {
            form = form.text("prompt", prompt);
        }

        let timeout = BASE_REQUEST_TIMEOUT
            + Duration::from_secs((audio_seconds * TIMEOUT_PER_AUDIO_SECOND) as u64);

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

        let body: serde_json::Value = resp.json().await.map_err(|e| {
            EchoError::AsrProvider(format!("whisper-server sent invalid JSON: {e}"))
        })?;

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
        // Covers the struct being dropped while we are still running: a
        // restart that errors half-way, a cancelled future. Destructors do not
        // run when the process dies, which is what `spawn_contained` is for.
        .kill_on_drop(true);

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = spawn_contained(cmd).await.map_err(|e| {
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

/// Spawn `cmd` so that the child dies with this process however this process
/// ends — a crash or a `taskkill /F` included, where no destructor and no
/// `RunEvent::Exit` ever runs.
///
/// Each OS gets its own mechanism, because there is no portable one:
///
/// - **Windows:** the child joins a Job Object flagged `KILL_ON_JOB_CLOSE`. We
///   hold the only handle to the job and never close it, so the kernel closes
///   it when this process ends for any reason, and closing the last handle
///   kills everything in the job.
/// - **Linux:** `PR_SET_PDEATHSIG` has the kernel SIGKILL the child when its
///   parent dies. See [`linux::spawn`] for the thread caveat.
/// - **macOS:** nothing in the kernel fires in the child without the child's
///   cooperation, and whisper-server does not cooperate — it never reads
///   stdin, so a closing pipe tells it nothing. The server is recorded in a
///   pidfile instead, and the next launch kills it ([`pidfile::sweep`]).
///
/// ponytail: on macOS an orphan therefore lives until Echo next starts, not
/// until Echo dies. Closing that gap needs a watchdog process of our own
/// (kqueue `NOTE_EXIT` on Echo, then kill the server): a second binary to sign
/// and ship, for a crash the user then does not relaunch from.
async fn spawn_contained(cmd: Command) -> std::io::Result<Child> {
    #[cfg(target_os = "linux")]
    let child = linux::spawn(cmd).await?;
    #[cfg(not(target_os = "linux"))]
    let child = {
        let mut cmd = cmd;
        cmd.spawn()?
    };

    #[cfg(target_os = "windows")]
    windows_job::assign(&child);

    // Recorded on Linux too, where the kernel already has it covered: it costs
    // nothing, and it is what lets Linux CI exercise the macOS path at all.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if let Some(pid) = child.id() {
        pidfile::record(pid);
    }

    Ok(child)
}

#[cfg(target_os = "windows")]
mod windows_job {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    use tokio::process::Child;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// Put `child` in the process-wide kill-on-close job.
    ///
    /// Failure is logged and otherwise ignored: a server a crash might orphan
    /// is still far better than no server.
    ///
    /// The child runs outside the job for the instant between `spawn` returning
    /// and this call. Starting it suspended would close that window, but std
    /// does not hand back the thread handle needed to resume it, and a crash in
    /// those few microseconds is not worth reimplementing `spawn` for.
    pub fn assign(child: &Child) {
        let (Some(job), Some(process)) = (job(), child.raw_handle()) else {
            return;
        };
        // SAFETY: both handles are live — the job is never closed, and `child`
        // owns the process handle for as long as we borrow it here.
        if let Err(e) =
            unsafe { AssignProcessToJobObject(HANDLE(job as *mut c_void), HANDLE(process)) }
        {
            tracing::warn!("whisper-server not tied to Echo's lifetime: {e}");
        }
    }

    /// The job, created on first use and deliberately never closed: its handle
    /// closing *is* the kill switch, so it has to live exactly as long as this
    /// process. Kept as an address because `HANDLE` is not `Sync`.
    ///
    /// The handle is not inheritable (no `SECURITY_ATTRIBUTES`), and that
    /// matters: a child holding its own copy would keep the job open after we
    /// die, and nothing would be killed.
    fn job() -> Option<usize> {
        static JOB: OnceLock<Option<usize>> = OnceLock::new();
        *JOB.get_or_init(|| {
            // SAFETY: plain Win32 calls with a correctly sized, initialized
            // struct; the handle is checked by the `Result`.
            let created = unsafe {
                CreateJobObjectW(None, PCWSTR::null()).and_then(|job| {
                    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                    SetInformationJobObject(
                        job,
                        JobObjectExtendedLimitInformation,
                        &info as *const _ as *const c_void,
                        std::mem::size_of_val(&info) as u32,
                    )
                    .map(|()| job)
                })
            };
            match created {
                Ok(job) => Some(job.0 as usize),
                Err(e) => {
                    tracing::warn!("could not create a job object for whisper-server: {e}");
                    None
                }
            }
        })
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io;
    use std::sync::{mpsc, OnceLock};

    use tokio::process::{Child, Command};
    use tokio::runtime::Handle;
    use tokio::sync::oneshot;

    type Request = (Command, Handle, oneshot::Sender<io::Result<Child>>);

    /// Spawn with `PR_SET_PDEATHSIG = SIGKILL`, from a thread that never exits.
    ///
    /// The caveat: to the kernel, "parent" means the *thread* that forked, not
    /// the process. Spawned from a tokio blocking-pool thread, which exits
    /// after ten idle seconds, the server would be SIGKILLed mid-session for no
    /// visible reason and silently reloaded on the next utterance. Every caller
    /// today happens to be on a runtime worker or the main thread, both of
    /// which live as long as the process, but nothing enforces that. So the
    /// fork happens on one dedicated thread that never returns. The runtime
    /// handle travels with each request so the child's pipes still register
    /// with the caller's reactor.
    pub async fn spawn(mut cmd: Command) -> io::Result<Child> {
        let parent = std::process::id();
        // SAFETY: runs between fork and exec, so it may only make
        // async-signal-safe calls and must not allocate. `prctl`, `getppid`
        // and an `io::Error` built from an errno are all of that.
        unsafe {
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == -1 {
                    return Err(io::Error::last_os_error());
                }
                // Had Echo died between the fork and the line above, the
                // signal was armed too late to ever fire, and our parent is no
                // longer the process that forked us. Refuse to start an orphan.
                if libc::getppid() as u32 != parent {
                    return Err(io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }

        static SPAWNER: OnceLock<mpsc::Sender<Request>> = OnceLock::new();
        let spawner = SPAWNER.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Request>();
            std::thread::Builder::new()
                .name("whisper-spawn".into())
                .spawn(move || {
                    for (mut cmd, runtime, reply) in rx {
                        let _entered = runtime.enter();
                        let _ = reply.send(cmd.spawn());
                    }
                })
                .expect("could not start the whisper-server spawn thread");
            tx
        });

        let gone = || io::Error::other("whisper-server spawn thread is gone");
        let (reply, result) = oneshot::channel();
        spawner
            .send((cmd, Handle::current(), reply))
            .map_err(|_| gone())?;
        result.await.map_err(|_| gone())?
    }
}

/// A record of the running server, so the next launch can kill one a crash
/// left behind. The only mechanism on macOS; see [`spawn_contained`].
///
/// One file per Echo process, named by its pid, holding the server's pid and
/// executable. A file is only acted on once the Echo that wrote it is gone —
/// `echo --transcribe` running beside the app must not kill the app's server —
/// and a pid is only killed while it still runs that same executable, so a pid
/// the OS has since handed to something else is left alone.
///
/// This cannot catch servers orphaned by builds from before the file existed.
/// Those live until they are killed or the machine restarts.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod pidfile {
    use std::os::unix::fs::MetadataExt;
    use std::path::PathBuf;

    const PREFIX: &str = "echo-whisper-server-";

    /// On macOS `temp_dir` is per-user. On Linux it is a shared `/tmp`, which
    /// is why [`sweep`] skips files anyone else owns and only ever kills
    /// something named whisper-server: a planted file must not be able to aim
    /// Echo at an arbitrary process of ours.
    fn path_for(owner: u32) -> PathBuf {
        std::env::temp_dir().join(format!("{PREFIX}{owner}.pid"))
    }

    pub fn record(pid: u32) {
        let Some(exe) = exe_of(pid) else { return };
        let path = path_for(std::process::id());
        if let Err(e) = std::fs::write(&path, format!("{pid}\n{}", exe.display())) {
            tracing::warn!("could not record whisper-server in {}: {e}", path.display());
        }
    }

    pub fn sweep() {
        let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        let me = std::process::id();
        // SAFETY: `getuid` has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };

        for entry in entries.flatten() {
            let Some(owner) = entry.file_name().to_str().and_then(|name| {
                name.strip_prefix(PREFIX)?
                    .strip_suffix(".pid")?
                    .parse::<u32>()
                    .ok()
            }) else {
                continue;
            };
            if entry.metadata().map_or(true, |m| m.uid() != uid) {
                continue;
            }
            // Our own pid on a file means an earlier Echo had it: this runs
            // before we have started anything.
            if owner != me && alive(owner) {
                continue;
            }

            let path = entry.path();
            let recorded = std::fs::read_to_string(&path).ok().and_then(|s| {
                let (pid, exe) = s.split_once('\n')?;
                // Never 0 or negative: `kill` reads those as "our whole process
                // group" and "every process we may signal".
                let pid = pid.parse::<i32>().ok().filter(|&p| p > 0)?;
                Some((pid, exe.to_owned()))
            });
            if let Some((pid, exe)) = recorded {
                let same = exe_of(pid as u32).is_some_and(|p| {
                    p.display().to_string() == exe
                        && p.file_name().is_some_and(|n| n == "whisper-server")
                });
                if same {
                    // SAFETY: a positive pid just confirmed to be our server.
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                    tracing::info!(pid, "killed a whisper-server left behind by an earlier run");
                }
            }
            let _ = std::fs::remove_file(&path);
        }
    }

    /// Fails safe: a pid we are not allowed to signal is somebody's live process.
    fn alive(pid: u32) -> bool {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 only checks existence and permission.
        let exists = unsafe { libc::kill(pid, 0) == 0 };
        exists || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    /// The executable a live process is running. `None` for a zombie, which is
    /// what a server the kernel already killed looks like until it is reaped.
    #[cfg(target_os = "linux")]
    pub fn exe_of(pid: u32) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }

    #[cfg(target_os = "macos")]
    pub fn exe_of(pid: u32) -> Option<PathBuf> {
        use std::os::unix::ffi::OsStrExt;
        let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: the buffer is exactly as large as we say.
        let len =
            unsafe { libc::proc_pidpath(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32) };
        (len > 0).then(|| PathBuf::from(std::ffi::OsStr::from_bytes(&buf[..len as usize])))
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

    /// A long-running child that is harmless to orphan if the test fails: it
    /// exits on its own after ten minutes.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn long_running() -> Command {
        #[cfg(target_os = "windows")]
        let mut cmd = Command::new("ping");
        #[cfg(target_os = "windows")]
        cmd.args(["-n", "600", "127.0.0.1"]);
        #[cfg(target_os = "linux")]
        let mut cmd = Command::new("sleep");
        #[cfg(target_os = "linux")]
        cmd.arg("600");
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }

    #[cfg(target_os = "windows")]
    fn process_alive(pid: u32) -> bool {
        use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: the handle is checked by the `Result` and closed below.
        unsafe {
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return false;
            };
            let mut code = 0u32;
            let running =
                GetExitCodeProcess(process, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32;
            let _ = CloseHandle(process);
            running
        }
    }

    #[cfg(target_os = "linux")]
    fn process_alive(pid: u32) -> bool {
        pidfile::exe_of(pid).is_some()
    }

    /// The reason `spawn_contained` exists: kill the parent the way a crash or
    /// `taskkill /F` does — no destructors, no exit event — and the child has
    /// to go with it.
    ///
    /// The parent must be a separate process, so the test re-runs its own
    /// binary with only itself selected, and in that copy the env var casts it
    /// as Echo.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[tokio::test]
    async fn a_force_killed_parent_takes_its_child_along() {
        use std::io::{BufRead, Write};

        const PARENT_ROLE: &str = "ECHO_TEST_CONTAINED_PARENT";
        const NAME: &str =
            "core::asr::whisper_server::tests::a_force_killed_parent_takes_its_child_along";

        if std::env::var_os(PARENT_ROLE).is_some() {
            let child = spawn_contained(long_running()).await.unwrap();
            println!("child={}", child.id().unwrap());
            std::io::stdout().flush().unwrap();
            tokio::time::sleep(Duration::from_secs(600)).await;
            return;
        }

        let mut parent = std::process::Command::new(std::env::current_exe().unwrap())
            .args([NAME, "--exact", "--nocapture", "--test-threads=1"])
            .env(PARENT_ROLE, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdout = std::io::BufReader::new(parent.stdout.take().unwrap());
        let child_pid: u32 = stdout
            .lines()
            .map_while(|line| line.ok())
            .find_map(|line| {
                let rest = line.split("child=").nth(1)?;
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse().ok()
            })
            .expect("the parent never reported its child");
        assert!(
            process_alive(child_pid),
            "the child should run before the kill"
        );

        // TerminateProcess on Windows, SIGKILL on Linux: nothing in the parent
        // gets to run.
        parent.kill().unwrap();
        parent.wait().unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while process_alive(child_pid) {
            assert!(
                std::time::Instant::now() < deadline,
                "child {child_pid} outlived its force-killed parent"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// What the next launch does with a server a crash left behind. The copy of
    /// `sleep` is named whisper-server because nothing else is ever killed.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[tokio::test]
    async fn the_startup_sweep_kills_a_leftover_server() {
        let dir = std::env::temp_dir().join(format!("echo-sweep-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("whisper-server");
        std::fs::copy("/bin/sleep", &fake).unwrap();

        let mut cmd = Command::new(&fake);
        cmd.arg("600").stdin(Stdio::null()).stdout(Stdio::null());
        let mut child = spawn_contained(cmd).await.unwrap();

        // The file carries our own pid, which the sweep reads as an earlier
        // Echo that happened to have it — a crashed one, as far as it knows.
        pidfile::sweep();

        let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .expect("the sweep left the server running")
            .unwrap();
        assert!(!status.success());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn free_port_returns_something_bindable() {
        let port = free_port().unwrap();
        assert!(port > 0);
        // Released back to the OS, so it must be bindable again right away.
        std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    }
}
