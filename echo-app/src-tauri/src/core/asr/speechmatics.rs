//! Speechmatics batch transcription.
//!
//! Submit a job as multipart (`config` JSON + `data_file`), then poll for the
//! transcript. Like [`super::assemblyai`] this queues rather than answering
//! inline, so it is slower than a single-request provider — the catalog note
//! says so.
//!
//! The `model` in the catalog maps to Speechmatics' *operating point*
//! (`enhanced` / `standard`), which is an accuracy-versus-cost dial rather than
//! a model name. It is carried in the `model` field because that is the field
//! the settings UI already offers, and inventing a second one for a single
//! provider would be worse than the slight misnomer.

use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;

use super::openai::join_url;
use super::wav::pcm_f32_to_wav;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

pub struct SpeechmaticsProvider {
    base_url: String,
    operating_point: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct CreateJobResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct TranscriptResponse {
    #[serde(default)]
    job: Option<JobInfo>,
    #[serde(default)]
    results: Vec<ResultItem>,
    #[serde(default)]
    metadata: Option<Metadata>,
}

#[derive(Debug, Deserialize)]
struct JobInfo {
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    #[serde(default)]
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResultItem {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    #[serde(default)]
    content: String,
    #[serde(default)]
    confidence: Option<f32>,
    /// "S1", "S2", … or "UU" for unknown; only present with diarization on.
    #[serde(default)]
    speaker: Option<String>,
}

impl SpeechmaticsProvider {
    pub fn new(base_url: &str, operating_point: impl Into<String>, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            operating_point: operating_point.into(),
            api_key,
        }
    }

    fn url(&self, path: &str) -> String {
        join_url(&self.base_url, path)
    }
}

/// Rebuild a sentence from Speechmatics' token stream.
///
/// Results are per-token, and punctuation arrives as its own item. Joining
/// everything with spaces would produce "hello , world ." — so punctuation
/// attaches to the preceding word instead.
fn segment_from_transcript(parsed: TranscriptResponse) -> TranscriptSegment {
    let mut text = String::new();
    let mut confidences: Vec<f32> = Vec::new();

    for item in &parsed.results {
        let Some(alt) = item.alternatives.first() else {
            continue;
        };
        if alt.content.is_empty() {
            continue;
        }
        let is_punctuation = item.kind.as_deref() == Some("punctuation");
        if !text.is_empty() && !is_punctuation {
            text.push(' ');
        }
        text.push_str(&alt.content);
        if let Some(c) = alt.confidence {
            confidences.push(c);
        }
    }

    // One number for a whole utterance is necessarily a summary; the mean is
    // the least misleading one available from per-token scores.
    let confidence = (!confidences.is_empty())
        .then(|| confidences.iter().sum::<f32>() / confidences.len() as f32);

    TranscriptSegment {
        text,
        is_final: true,
        language: parsed.metadata.and_then(|m| m.language),
        confidence,
    }
}

/// Whether a polled job is finished, still running, or broken.
fn job_done(parsed: &TranscriptResponse) -> Result<bool> {
    match parsed.job.as_ref().and_then(|j| j.status.as_deref()) {
        Some("rejected") | Some("expired") => Err(EchoError::AsrProvider(
            "speechmatics job was rejected".into(),
        )),
        Some("running") => Ok(false),
        // "done", or a transcript body with no job block at all.
        _ => Ok(true),
    }
}

/// Speaker turns out of a diarized transcript, in spoken order.
///
/// Every token carries its own `speaker`, so a turn is a run of tokens with the
/// same one, rebuilt with the same punctuation rule as
/// [`segment_from_transcript`]. Two things are not a change of speaker: a
/// punctuation mark, which belongs to the word before it whatever it is
/// labelled, and `UU`, Speechmatics' "could not tell" — starting a new turn for
/// it would split one person's sentence around a single unclear word.
///
/// A word with no `speaker` at all means diarization did not run, which is an
/// error rather than a transcript attributed to nobody.
fn turns_from_transcript(parsed: &TranscriptResponse) -> Result<Vec<(String, String)>> {
    let mut turns: Vec<(String, String)> = Vec::new();

    for item in &parsed.results {
        let Some(alt) = item.alternatives.first() else {
            continue;
        };
        if alt.content.is_empty() {
            continue;
        }
        let is_punctuation = item.kind.as_deref() == Some("punctuation");
        let speaker = match (&alt.speaker, is_punctuation) {
            (_, true) => None,
            (Some(s), false) if s == "UU" => None,
            (Some(s), false) => Some(s.as_str()),
            (None, false) => {
                return Err(EchoError::AsrProvider(
                    "speechmatics returned no speaker labels".into(),
                ))
            }
        };

        match turns.last_mut() {
            Some((current, text)) if speaker.is_none_or(|s| s == current) => {
                if !is_punctuation {
                    text.push(' ');
                }
                text.push_str(&alt.content);
            }
            _ => turns.push((speaker.unwrap_or("UU").to_string(), alt.content.clone())),
        }
    }
    Ok(turns)
}

impl SpeechmaticsProvider {
    /// Submit one job and poll it to a finished transcript.
    ///
    /// `speakers` is the import path: it asks for `"diarization": "speaker"`
    /// and trades the utterance-sized timeouts for
    /// [`super::http::IMPORT_TIMEOUT`], since both the upload and the job for a
    /// whole recording outlast them.
    async fn run(
        &self,
        part: multipart::Part,
        language: Option<&str>,
        speakers: bool,
    ) -> Result<TranscriptResponse> {
        let client = super::http::client();

        let form = multipart::Form::new()
            .text(
                "config",
                job_config(language, &self.operating_point, speakers),
            )
            .part("data_file", part);

        let create_url = self.url("v2/jobs");
        crate::core::egress::record(&create_url, "cloud transcription");

        let mut create = client
            .post(&create_url)
            .bearer_auth(&self.api_key)
            .multipart(form);
        if speakers {
            create = create.timeout(super::http::IMPORT_TIMEOUT);
        }
        let resp = create
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "speechmatics job error {status}: {body}"
            )));
        }
        let created: CreateJobResponse = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        let poll_url = self.url(&format!("v2/jobs/{}/transcript?format=json-v2", created.id));
        let limit = if speakers {
            super::http::IMPORT_TIMEOUT
        } else {
            super::http::POLL_DEADLINE
        };
        super::http::poll_until("speechmatics transcription", limit, || {
            let client = client.clone();
            let poll_url = poll_url.clone();
            let key = self.api_key.clone();
            async move {
                let resp = client
                    .get(&poll_url)
                    .bearer_auth(&key)
                    .send()
                    .await
                    .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

                // A job that is not finished answers 404 or 202 rather than a
                // transcript. Treating that as an error would abandon every
                // utterance on its first poll.
                if resp.status() == reqwest::StatusCode::NOT_FOUND
                    || resp.status() == reqwest::StatusCode::ACCEPTED
                {
                    return Ok(None);
                }
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(EchoError::AsrProvider(format!(
                        "speechmatics poll error {status}: {body}"
                    )));
                }

                let parsed: TranscriptResponse = resp
                    .json()
                    .await
                    .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
                if !job_done(&parsed)? {
                    return Ok(None);
                }
                Ok(Some(parsed))
            }
        })
        .await
    }
}

/// The job's `config` JSON.
///
/// Speechmatics has no auto-detect on the standard batch path, so an unset
/// language becomes English rather than a rejected job.
fn job_config(language: Option<&str>, operating_point: &str, speakers: bool) -> String {
    let mut config = serde_json::json!({
        "type": "transcription",
        "transcription_config": {
            "language": language.unwrap_or("en"),
            "operating_point": operating_point,
        },
    });
    if speakers {
        config["transcription_config"]["diarization"] = "speaker".into();
    }
    config.to_string()
}

#[async_trait]
impl AsrProvider for SpeechmaticsProvider {
    fn name(&self) -> &str {
        "speechmatics"
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        let wav = pcm_f32_to_wav(&audio, 16_000)?;
        let part = multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        Ok(segment_from_transcript(
            self.run(part, language, false).await?,
        ))
    }

    async fn transcribe_speakers(
        &self,
        audio: Vec<u8>,
        mime: &str,
        language: Option<&str>,
    ) -> Result<Vec<(String, String)>> {
        // The file name is only a hint, but a `.wav` name on mp3 bytes is the
        // kind of lie a decoder might believe, so it follows the MIME type.
        let name = format!("audio.{}", mime.rsplit('/').next().unwrap_or("bin"));
        let part = multipart::Part::bytes(audio)
            .file_name(name)
            .mime_str(mime)
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        turns_from_transcript(&self.run(part, language, true).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> TranscriptResponse {
        serde_json::from_str(json).expect("valid json")
    }

    #[test]
    fn tokens_are_rejoined_into_a_sentence() {
        let seg = segment_from_transcript(parse(
            r#"{"results":[
                {"type":"word","alternatives":[{"content":"hello","confidence":0.9}]},
                {"type":"word","alternatives":[{"content":"there","confidence":0.8}]}
            ],"metadata":{"language":"en"}}"#,
        ));
        assert_eq!(seg.text, "hello there");
        assert_eq!(seg.language.as_deref(), Some("en"));
    }

    #[test]
    fn punctuation_attaches_to_the_word_before_it() {
        // Joining every token with a space is the obvious implementation and
        // it produces "hello , world ." — wrong in a way that reaches the
        // user's document.
        let seg = segment_from_transcript(parse(
            r#"{"results":[
                {"type":"word","alternatives":[{"content":"hello"}]},
                {"type":"punctuation","alternatives":[{"content":","}]},
                {"type":"word","alternatives":[{"content":"world"}]},
                {"type":"punctuation","alternatives":[{"content":"."}]}
            ]}"#,
        ));
        assert_eq!(seg.text, "hello, world.");
    }

    #[test]
    fn confidence_is_averaged_across_tokens() {
        let seg = segment_from_transcript(parse(
            r#"{"results":[
                {"type":"word","alternatives":[{"content":"a","confidence":1.0}]},
                {"type":"word","alternatives":[{"content":"b","confidence":0.5}]}
            ]}"#,
        ));
        assert_eq!(seg.confidence, Some(0.75));
    }

    #[test]
    fn a_running_job_is_not_treated_as_a_finished_empty_one() {
        assert!(!job_done(&parse(r#"{"job":{"status":"running"}}"#)).unwrap());
        assert!(job_done(&parse(r#"{"job":{"status":"done"},"results":[]}"#)).unwrap());
    }

    #[test]
    fn a_rejected_job_errors_rather_than_polling_to_the_deadline() {
        assert!(job_done(&parse(r#"{"job":{"status":"rejected"}}"#)).is_err());
    }

    #[test]
    fn diarization_is_requested_only_for_an_import() {
        let import: serde_json::Value =
            serde_json::from_str(&job_config(None, "enhanced", true)).unwrap();
        assert_eq!(import["transcription_config"]["diarization"], "speaker");
        let dictation: serde_json::Value =
            serde_json::from_str(&job_config(Some("de"), "enhanced", false)).unwrap();
        assert!(dictation["transcription_config"]
            .get("diarization")
            .is_none());
        assert_eq!(dictation["transcription_config"]["language"], "de");
    }

    /// Shaped like the documented json-v2 transcript for a job with
    /// `"diarization": "speaker"`: every token, punctuation included, carries
    /// an `S#` speaker, and `UU` marks one it could not attribute.
    #[test]
    fn speaker_turns_are_rebuilt_from_labelled_tokens() {
        let parsed = parse(
            r#"{
              "format": "2.9",
              "job": {"id": "a1b2c3", "status": "done", "data_name": "interview.mp3",
                      "duration": 6, "created_at": "2026-09-13T10:00:00.000Z"},
              "metadata": {"created_at": "2026-09-13T10:00:05.000Z", "type": "transcription",
                           "language": "en",
                           "transcription_config": {"language": "en", "diarization": "speaker"}},
              "results": [
                {"type": "word", "start_time": 0.1, "end_time": 0.4,
                 "alternatives": [{"content": "hello", "confidence": 0.93, "language": "en", "speaker": "S1"}]},
                {"type": "punctuation", "start_time": 0.4, "end_time": 0.4, "is_eos": true,
                 "attaches_to": "previous",
                 "alternatives": [{"content": ".", "confidence": 1.0, "language": "en", "speaker": "S2"}]},
                {"type": "word", "start_time": 1.0, "end_time": 1.2,
                 "alternatives": [{"content": "hi", "confidence": 1.0, "language": "en", "speaker": "S2"}]},
                {"type": "word", "start_time": 1.2, "end_time": 1.5,
                 "alternatives": [{"content": "there", "confidence": 0.41, "language": "en", "speaker": "UU"}]},
                {"type": "punctuation", "start_time": 1.5, "end_time": 1.5, "is_eos": true,
                 "attaches_to": "previous",
                 "alternatives": [{"content": ".", "confidence": 1.0, "language": "en", "speaker": "S2"}]},
                {"type": "word", "start_time": 2.0, "end_time": 2.3,
                 "alternatives": [{"content": "welcome", "confidence": 0.9, "language": "en", "speaker": "S1"}]}
              ]
            }"#,
        );
        assert_eq!(
            turns_from_transcript(&parsed).unwrap(),
            vec![
                ("S1".into(), "hello.".into()),
                ("S2".into(), "hi there.".into()),
                ("S1".into(), "welcome".into()),
            ]
        );
    }

    #[test]
    fn a_transcript_without_speakers_is_an_error_not_one_long_monologue() {
        let parsed = parse(r#"{"results":[{"type":"word","alternatives":[{"content":"hi"}]}]}"#);
        assert!(turns_from_transcript(&parsed).is_err());
    }

    #[test]
    fn silence_produces_empty_text_not_a_crash() {
        let seg = segment_from_transcript(parse(r#"{"results":[]}"#));
        assert_eq!(seg.text, "");
        assert!(seg.confidence.is_none());
    }
}
