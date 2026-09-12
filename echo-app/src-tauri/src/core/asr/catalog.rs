//! The single list of cloud ASR providers Echo knows about.
//!
//! This exists to stop one list from becoming four. Before it, adding a
//! provider meant editing the boot loop in `lib.rs`, the `match` in
//! `commands::providers`, the key-entry array in `CloudProviders.tsx` and the
//! engine `<select>` in `SettingsPanel.tsx` — four edits, four chances to
//! forget one, and a provider that half-exists if you did. Everything now reads
//! this table, and the frontend reads it too via `list_cloud_providers`, so a
//! new provider is one row plus (sometimes) one file.
//!
//! **Secrets are not in here.** A row describes the *shape* of a provider —
//! where it lives, what models it offers, what extra field it needs. The API
//! key lives in the OS keychain and the user's choices live in the settings
//! table; see [`crate::commands::providers`].

use serde::Serialize;

/// How a provider's HTTP conversation actually goes.
///
/// The variants are grouped by request shape rather than by vendor, because
/// that is what determines whether a vendor costs new code: everything
/// [`ProviderKind::OpenAiCompatible`] shares one implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// `POST {base}/audio/transcriptions`, multipart, bearer auth.
    /// OpenAI, Groq, Mistral, and every self-hosted clone.
    OpenAiCompatible,
    /// Raw-body POST with query params, `Token` auth. Also the only provider
    /// with a real streaming socket.
    Deepgram,
    /// Multipart with an `xi-api-key` header and `model_id`/`file` fields.
    ElevenLabs,
    /// Multipart `audio` + `definition` JSON against a regional host.
    AzureSpeech,
    /// Upload bytes, create a job, then poll it.
    AssemblyAi,
    /// Multipart job submission, then poll.
    Speechmatics,
    /// JSON with base64 audio inline, key in the query string.
    GoogleStt,
}

/// One provider's shape. Everything here is public, static and non-secret.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ProviderKind,
    /// API root, not the full transcription path. Empty when the user must
    /// supply it (`needs_endpoint`).
    pub default_endpoint: &'static str,
    /// Suggested models, most common first. `models[0]` is the default, so
    /// reordering this changes what existing users get — don't, casually.
    /// Empty means the provider has no model choice, or only a free-text one.
    pub models: &'static [&'static str],
    /// The user must provide the endpoint; there is no sensible default.
    pub needs_endpoint: bool,
    /// The user must provide a region — the host itself depends on it.
    pub needs_region: bool,
    pub docs_url: &'static str,
    /// Shown under the provider in Settings. Use it for the things people
    /// discover the hard way: latency, hard limits, where the key travels.
    pub note: &'static str,
}

impl ProviderSpec {
    /// The model to use when the user has not picked one.
    pub fn default_model(&self) -> Option<&'static str> {
        self.models.first().copied()
    }
}

/// Every provider Echo can talk to. Order is the order Settings shows them:
/// fastest and most common first, because that is what most people want.
pub const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: "openai",
        label: "OpenAI",
        kind: ProviderKind::OpenAiCompatible,
        default_endpoint: "https://api.openai.com/v1",
        models: &["whisper-1", "gpt-4o-mini-transcribe", "gpt-4o-transcribe"],
        needs_endpoint: false,
        needs_region: false,
        docs_url: "https://platform.openai.com/api-keys",
        note: "Max 25 MB per request. gpt-4o-mini-transcribe is cheaper and more accurate than whisper-1.",
    },
    ProviderSpec {
        id: "groq",
        label: "Groq",
        kind: ProviderKind::OpenAiCompatible,
        default_endpoint: "https://api.groq.com/openai/v1",
        models: &["whisper-large-v3", "whisper-large-v3-turbo"],
        needs_endpoint: false,
        needs_region: false,
        docs_url: "https://console.groq.com/keys",
        note: "Usually the fastest cloud option. Turbo is cheaper and faster at a small accuracy cost.",
    },
    ProviderSpec {
        id: "deepgram",
        label: "Deepgram",
        kind: ProviderKind::Deepgram,
        default_endpoint: "https://api.deepgram.com",
        models: &["nova-2", "nova-3"],
        needs_endpoint: false,
        needs_region: false,
        docs_url: "https://console.deepgram.com/",
        note: "The only provider with live streaming — words appear as you speak.",
    },
    ProviderSpec {
        id: "mistral",
        label: "Mistral (Voxtral)",
        kind: ProviderKind::OpenAiCompatible,
        default_endpoint: "https://api.mistral.ai/v1",
        models: &["voxtral-mini-latest"],
        needs_endpoint: false,
        needs_region: false,
        docs_url: "https://console.mistral.ai/api-keys",
        note: "Inexpensive, around $0.18 per hour of audio.",
    },
    ProviderSpec {
        id: "elevenlabs",
        label: "ElevenLabs Scribe",
        kind: ProviderKind::ElevenLabs,
        default_endpoint: "https://api.elevenlabs.io/v1",
        models: &["scribe_v2", "scribe_v1"],
        needs_endpoint: false,
        needs_region: false,
        docs_url: "https://elevenlabs.io/app/settings/api-keys",
        note: "High accuracy across 99 languages. Batch only — no live streaming.",
    },
    ProviderSpec {
        id: "assemblyai",
        label: "AssemblyAI",
        kind: ProviderKind::AssemblyAi,
        default_endpoint: "https://api.assemblyai.com",
        models: &["universal-3-5-pro", "universal-2"],
        needs_endpoint: false,
        needs_region: false,
        docs_url: "https://www.assemblyai.com/app/account",
        note: "Uploads, queues and polls — expect a few seconds even for a short phrase.",
    },
    ProviderSpec {
        id: "speechmatics",
        label: "Speechmatics",
        kind: ProviderKind::Speechmatics,
        default_endpoint: "https://asr.api.speechmatics.com",
        models: &["enhanced", "standard"],
        needs_endpoint: false,
        needs_region: false,
        docs_url: "https://portal.speechmatics.com/",
        note: "Strong multilingual accuracy. Submits a job and polls — expect a few seconds.",
    },
    ProviderSpec {
        id: "azure",
        label: "Azure AI Speech",
        kind: ProviderKind::AzureSpeech,
        default_endpoint: "",
        models: &[],
        needs_endpoint: false,
        needs_region: true,
        docs_url: "https://portal.azure.com/",
        note: "Needs the region of your Speech resource (for example westeurope), not just a key.",
    },
    ProviderSpec {
        id: "google",
        label: "Google Speech-to-Text",
        kind: ProviderKind::GoogleStt,
        default_endpoint: "https://speech.googleapis.com/v1",
        models: &["latest_short", "latest_long", "default"],
        needs_endpoint: false,
        needs_region: false,
        docs_url: "https://console.cloud.google.com/apis/credentials",
        note: "Audio must be under 60 seconds. Google requires the key in the URL, so it may appear in proxy logs.",
    },
    ProviderSpec {
        id: "custom",
        label: "Custom (OpenAI-compatible)",
        kind: ProviderKind::OpenAiCompatible,
        default_endpoint: "",
        models: &[],
        needs_endpoint: true,
        needs_region: false,
        docs_url: "https://github.com/vedantnimbarte/Echo/blob/main/README.md#cloud-providers",
        note: "Point Echo at any OpenAI-compatible endpoint: LiteLLM, OpenRouter, vLLM, or a self-hosted Whisper server.",
    },
];

/// Look up a provider by id.
pub fn find(id: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique() {
        // A duplicate id would silently shadow a provider in `find`, and the
        // keychain would hand both rows the same key.
        let ids: HashSet<_> = PROVIDERS.iter().map(|p| p.id).collect();
        assert_eq!(
            ids.len(),
            PROVIDERS.len(),
            "duplicate provider id in catalog"
        );
    }

    #[test]
    fn every_provider_can_be_found_by_its_own_id() {
        for spec in PROVIDERS {
            assert!(find(spec.id).is_some(), "{} is not findable", spec.id);
        }
        assert!(
            find("local").is_none(),
            "local is an engine, not a cloud provider"
        );
        assert!(find("none").is_none());
    }

    #[test]
    fn a_provider_either_ships_an_endpoint_or_asks_for_one() {
        // Otherwise it would build a request against an empty URL and fail
        // with something unreadable instead of a settings field.
        for spec in PROVIDERS {
            let has_default = !spec.default_endpoint.is_empty();
            assert!(
                has_default || spec.needs_endpoint || spec.needs_region,
                "{} has no endpoint and never asks for one",
                spec.id
            );
        }
    }

    #[test]
    fn the_defaults_that_shipped_before_the_catalog_did_not_move() {
        // Existing users have no stored model, so they resolve to models[0].
        // Reordering these silently changes what they get mid-sentence.
        assert_eq!(find("openai").unwrap().default_model(), Some("whisper-1"));
        assert_eq!(
            find("groq").unwrap().default_model(),
            Some("whisper-large-v3")
        );
        assert_eq!(find("deepgram").unwrap().default_model(), Some("nova-2"));
    }

    #[test]
    fn every_provider_explains_itself() {
        for spec in PROVIDERS {
            assert!(!spec.label.is_empty(), "{} has no label", spec.id);
            assert!(!spec.note.is_empty(), "{} has no note", spec.id);
            assert!(spec.docs_url.starts_with("https://"), "{} docs", spec.id);
        }
    }
}
