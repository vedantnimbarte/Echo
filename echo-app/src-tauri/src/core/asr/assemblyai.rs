//! AssemblyAI.
//!
//! Three round trips for one sentence: upload the bytes, create a transcript
//! job, then poll until it finishes. There is no inline path — `/v2/transcript`
//! only takes a URL, and `/v2/upload` exists to give it one.
//!
//! That shape is worth naming, because it is a poor fit for a voice keyboard.
//! Providers that answer in a single request return in a few hundred
//! milliseconds; this one queues. It is offered because the accuracy is good
//! and users ask for it, and the catalog note warns about the latency rather
//! than letting people discover it mid-sentence.

use async_trait::async_trait;
use serde::Deserialize;

use super::openai::join_url;
use super::wav::pcm_f32_to_wav;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

pub struct AssemblyAiProvider {
    base_url: String,
    model: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct UploadResponse {
    upload_url: String,
}

#[derive(Debug, Deserialize)]
struct TranscriptResponse {
    id: String,
    status: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    language_code: Option<String>,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    error: Option<String>,
}

/// What one poll of a job means.
enum JobState {
    Done(TranscriptSegment),
    Working,
}

impl AssemblyAiProvider {
    pub fn new(base_url: &str, model: impl Into<String>, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.into(),
            api_key,
        }
    }

    fn url(&self, path: &str) -> String {
        join_url(&self.base_url, path)
    }
}

/// Read one poll response, distinguishing "still queued" from "done" and from
/// a job that failed.
///
/// A failed job must become an `Err` rather than empty text: empty text reads
/// as silence and the utterance is dropped, while an error hands the audio to
/// the offline engine.
fn job_state(resp: TranscriptResponse) -> Result<JobState> {
    match resp.status.as_str() {
        "completed" => Ok(JobState::Done(TranscriptSegment {
            text: resp.text.unwrap_or_default().trim().to_string(),
            is_final: true,
            language: resp.language_code,
            confidence: resp.confidence,
        })),
        "error" => Err(EchoError::AsrProvider(format!(
            "assemblyai job failed: {}",
            resp.error.unwrap_or_else(|| "no reason given".into())
        ))),
        // "queued", "processing", and anything new they add later.
        _ => Ok(JobState::Working),
    }
}

#[async_trait]
impl AsrProvider for AssemblyAiProvider {
    fn name(&self) -> &str {
        "assemblyai"
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        let wav = pcm_f32_to_wav(&audio, 16_000)?;
        let client = super::http::client();

        // 1. Upload the audio so there is a URL to transcribe.
        let upload_url = self.url("v2/upload");
        crate::core::egress::record(&upload_url, "cloud transcription");
        let resp = client
            .post(&upload_url)
            .header("authorization", &self.api_key)
            .header("content-type", "application/octet-stream")
            .body(wav)
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "assemblyai upload error {status}: {body}"
            )));
        }
        let uploaded: UploadResponse = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        // 2. Queue the job.
        let mut body = serde_json::json!({
            "audio_url": uploaded.upload_url,
            "speech_model": self.model,
        });
        // Naming a language turns detection off; the two are mutually
        // exclusive and sending both is rejected.
        match language {
            Some(lang) => body["language_code"] = serde_json::Value::String(lang.to_string()),
            None => body["language_detection"] = serde_json::Value::Bool(true),
        }

        let create_url = self.url("v2/transcript");
        let resp = client
            .post(&create_url)
            .header("authorization", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "assemblyai job error {status}: {body}"
            )));
        }
        let created: TranscriptResponse = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        // A job can already be finished on creation; poll handles that on the
        // first pass rather than sleeping first.
        let poll_url = self.url(&format!("v2/transcript/{}", created.id));
        super::http::poll_until("assemblyai transcription", || {
            let client = client.clone();
            let poll_url = poll_url.clone();
            let key = self.api_key.clone();
            async move {
                let resp = client
                    .get(&poll_url)
                    .header("authorization", &key)
                    .send()
                    .await
                    .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(EchoError::AsrProvider(format!(
                        "assemblyai poll error {status}: {body}"
                    )));
                }
                let parsed: TranscriptResponse = resp
                    .json()
                    .await
                    .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
                Ok(match job_state(parsed)? {
                    JobState::Done(seg) => Some(seg),
                    JobState::Working => None,
                })
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(json: &str) -> Result<JobState> {
        job_state(serde_json::from_str(json).expect("valid json"))
    }

    #[test]
    fn a_completed_job_yields_its_transcript() {
        let s = state(r#"{"id":"1","status":"completed","text":"  hello  ","language_code":"en","confidence":0.9}"#)
            .unwrap();
        match s {
            JobState::Done(seg) => {
                assert_eq!(seg.text, "hello");
                assert_eq!(seg.language.as_deref(), Some("en"));
                assert_eq!(seg.confidence, Some(0.9));
            }
            JobState::Working => panic!("completed job read as still working"),
        }
    }

    #[test]
    fn a_queued_job_is_not_mistaken_for_silence() {
        // The dangerous confusion: "processing" has no text yet, and treating
        // that as an empty transcript would drop the utterance on the first poll.
        for status in ["queued", "processing"] {
            let json = format!(r#"{{"id":"1","status":"{status}"}}"#);
            assert!(matches!(state(&json).unwrap(), JobState::Working), "{status}");
        }
    }

    #[test]
    fn a_failed_job_errors_so_the_offline_engine_gets_a_turn() {
        let err = state(r#"{"id":"1","status":"error","error":"audio too short"}"#);
        assert!(err.is_err());
        assert!(format!("{:?}", err.err().unwrap()).contains("audio too short"));
    }

    #[test]
    fn an_unfamiliar_status_waits_rather_than_failing() {
        // A status added after this shipped should stall the poll until the
        // deadline, not crash the utterance.
        assert!(matches!(
            state(r#"{"id":"1","status":"reprocessing"}"#).unwrap(),
            JobState::Working
        ));
    }

    #[test]
    fn paths_are_built_off_the_configured_base() {
        let p = AssemblyAiProvider::new("https://api.assemblyai.com/", "universal-2", "k".into());
        assert_eq!(p.url("v2/upload"), "https://api.assemblyai.com/v2/upload");
    }
}
