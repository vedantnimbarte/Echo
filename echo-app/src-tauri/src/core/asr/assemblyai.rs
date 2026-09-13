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
    /// Turn-by-turn speech, only filled in when `speaker_labels` was on. The
    /// API sends `null` rather than omitting it otherwise, hence the `Option`.
    #[serde(default)]
    utterances: Option<Vec<Utterance>>,
}

#[derive(Debug, Deserialize)]
struct Utterance {
    /// "A", "B", … — `null` when diarization is off.
    #[serde(default)]
    speaker: Option<String>,
    #[serde(default)]
    text: String,
}

/// What one poll of a job means.
enum JobState {
    Done(TranscriptResponse),
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

    /// Upload `audio`, queue a job for it and poll until it is finished.
    ///
    /// `speakers` is the import path: it turns on `speaker_labels` and trades
    /// the utterance-sized timeouts for [`super::http::IMPORT_TIMEOUT`], since
    /// both the upload and the job for a whole recording outlast them.
    async fn run(
        &self,
        audio: Vec<u8>,
        language: Option<&str>,
        speakers: bool,
    ) -> Result<TranscriptResponse> {
        let client = super::http::client();

        // 1. Upload the audio so there is a URL to transcribe.
        let upload_url = self.url("v2/upload");
        crate::core::egress::record(&upload_url, "cloud transcription");
        let mut upload = client
            .post(&upload_url)
            .header("authorization", &self.api_key)
            .header("content-type", "application/octet-stream")
            .body(audio);
        if speakers {
            upload = upload.timeout(super::http::IMPORT_TIMEOUT);
        }
        let resp = upload
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
        let body = job_body(&uploaded.upload_url, &self.model, language, speakers);

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
        let limit = if speakers {
            super::http::IMPORT_TIMEOUT
        } else {
            super::http::POLL_DEADLINE
        };
        super::http::poll_until("assemblyai transcription", limit, || {
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
                    JobState::Done(done) => Some(done),
                    JobState::Working => None,
                })
            }
        })
        .await
    }
}

/// The JSON that queues a job.
fn job_body(
    audio_url: &str,
    model: &str,
    language: Option<&str>,
    speakers: bool,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "audio_url": audio_url,
        "speech_model": model,
    });
    // Naming a language turns detection off; the two are mutually
    // exclusive and sending both is rejected.
    match language {
        Some(lang) => body["language_code"] = serde_json::Value::String(lang.to_string()),
        None => body["language_detection"] = serde_json::Value::Bool(true),
    }
    if speakers {
        body["speaker_labels"] = serde_json::Value::Bool(true);
    }
    body
}

/// Read one poll response, distinguishing "still queued" from "done" and from
/// a job that failed.
///
/// A failed job must become an `Err` rather than empty text: empty text reads
/// as silence and the utterance is dropped, while an error hands the audio to
/// the offline engine.
fn job_state(resp: TranscriptResponse) -> Result<JobState> {
    match resp.status.as_str() {
        "completed" => Ok(JobState::Done(resp)),
        "error" => Err(EchoError::AsrProvider(format!(
            "assemblyai job failed: {}",
            resp.error.unwrap_or_else(|| "no reason given".into())
        ))),
        // "queued", "processing", and anything new they add later.
        _ => Ok(JobState::Working),
    }
}

fn segment_from_response(resp: TranscriptResponse) -> TranscriptSegment {
    TranscriptSegment {
        text: resp.text.unwrap_or_default().trim().to_string(),
        is_final: true,
        language: resp.language_code,
        confidence: resp.confidence,
    }
}

/// Speaker turns out of a finished job, in spoken order.
///
/// `utterances` missing or unlabelled means the job ran without diarization,
/// which is an error rather than a transcript attributed to nobody.
fn turns_from_response(resp: TranscriptResponse) -> Result<Vec<(String, String)>> {
    let missing = || EchoError::AsrProvider("assemblyai returned no speaker labels".into());
    resp.utterances
        .ok_or_else(missing)?
        .into_iter()
        .filter(|u| !u.text.trim().is_empty())
        .map(|u| Ok((u.speaker.ok_or_else(missing)?, u.text.trim().to_string())))
        .collect()
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
        Ok(segment_from_response(self.run(wav, language, false).await?))
    }

    async fn transcribe_speakers(
        &self,
        audio: Vec<u8>,
        _mime: &str,
        language: Option<&str>,
    ) -> Result<Vec<(String, String)>> {
        // The upload is an opaque octet stream; AssemblyAI sniffs the format
        // itself, so the MIME type has nowhere to go.
        turns_from_response(self.run(audio, language, true).await?)
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
            JobState::Done(done) => {
                let seg = segment_from_response(done);
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
            assert!(
                matches!(state(&json).unwrap(), JobState::Working),
                "{status}"
            );
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

    #[test]
    fn speaker_labels_are_requested_only_for_an_import() {
        let import = job_body("https://cdn/x", "universal-2", None, true);
        assert_eq!(import["speaker_labels"], true);
        let dictation = job_body("https://cdn/x", "universal-2", Some("en"), false);
        assert!(dictation.get("speaker_labels").is_none(), "{dictation}");
    }

    /// Shaped like the documented `GET /v2/transcript/{id}` answer with
    /// `speaker_labels` on: turns in `utterances`, speakers lettered "A", "B".
    #[test]
    fn speaker_turns_are_read_out_of_the_utterances() {
        let resp: TranscriptResponse = serde_json::from_str(
            r#"{
              "id": "5551722-f677-48a6-9287-39c0aafd9ac1",
              "status": "completed",
              "language_code": "en_us",
              "text": "Smoke from hundreds of wildfires. Is it bad? It is.",
              "confidence": 0.93,
              "speaker_labels": true,
              "utterances": [
                {"speaker": "A", "start": 250, "end": 2650, "confidence": 0.95,
                 "text": "Smoke from hundreds of wildfires.",
                 "words": [{"text": "Smoke", "start": 250, "end": 650,
                            "confidence": 0.97, "speaker": "A"}]},
                {"speaker": "B", "start": 2900, "end": 3600, "confidence": 0.91,
                 "text": "Is it bad?", "words": []},
                {"speaker": "A", "start": 3800, "end": 4300, "confidence": 0.92,
                 "text": "It is.", "words": []}
              ]
            }"#,
        )
        .unwrap();
        let turns = turns_from_response(resp).unwrap();
        assert_eq!(
            turns,
            vec![
                ("A".into(), "Smoke from hundreds of wildfires.".into()),
                ("B".into(), "Is it bad?".into()),
                ("A".into(), "It is.".into()),
            ]
        );
    }

    #[test]
    fn a_job_without_utterances_is_an_error_not_an_unlabelled_transcript() {
        let resp: TranscriptResponse = serde_json::from_str(
            r#"{"id":"1","status":"completed","text":"hi","utterances":null}"#,
        )
        .unwrap();
        assert!(turns_from_response(resp).is_err());
    }
}
