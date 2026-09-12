//! What the decoder should expect to hear next.
//!
//! whisper's `initial_prompt` biases token choice before a word is committed,
//! which is the only point where a mishearing can still be prevented rather
//! than repaired. Two things belong in it and neither was reaching it:
//!
//! 1. **The vocabulary for the app being dictated into.** Dictionary entries
//!    scoped to a profile were filtered out of the hint, so per-app vocabulary
//!    biased nothing — the very terms most likely to be jargon.
//! 2. **What was just said.** `initial_prompt` is designed to take the text
//!    preceding the audio, so the previous sentence carries its own names and
//!    terminology into the next one for free.
//!
//! **Why the app is sampled at recording start.** Delivery resolves the focused
//! app when the transcript is *ready*, because focus can move while you talk
//! (see [`crate::core::appcontext`]). A prompt is needed before that, when the
//! audio is handed to the decoder. Sampling at the start is therefore an
//! approximation — and an acceptable one, because a prompt is a hint: guessing
//! the wrong app costs a weaker bias, never a wrong transcript, while delivery
//! keeps its exact answer.

use std::sync::Mutex;

use crate::core::lock::LockLive;

/// Room reserved for the preceding transcript inside whisper's prompt budget.
///
/// Small on purpose. The vocabulary hint is the more valuable half — it is the
/// spellings we actively want produced — so context takes what is left rather
/// than crowding terms out.
const MAX_PREVIOUS_CHARS: usize = 200;

/// Live decoder context, shared between the recording pipeline (which writes
/// it) and the local whisper provider (which reads it per utterance).
#[derive(Default)]
pub struct PromptContext {
    inner: Mutex<Inner>,
}

#[derive(Default, Clone)]
struct Inner {
    /// Focused app at recording start, lowercased, if it could be determined.
    app: Option<String>,
    /// Dictionary profile that app selects, if any.
    profile: Option<i64>,
    /// The transcript immediately before this one, in the same app.
    previous: Option<String>,
}

impl PromptContext {
    /// Note which app is being dictated into, and the dictionary profile it
    /// selects.
    ///
    /// Changing app drops the carried-over sentence: text from a code editor is
    /// not context for an email, and biasing one with the other is worse than
    /// no context at all.
    pub fn set_app(&self, app: Option<String>, profile: Option<i64>) {
        let mut inner = self.inner.lock_live();
        if inner.app != app {
            inner.previous = None;
            inner.app = app;
        }
        inner.profile = profile;
    }

    /// Record the transcript just produced, as context for the next one.
    pub fn set_previous(&self, text: &str) {
        let text = text.trim();
        let mut inner = self.inner.lock_live();
        inner.previous = (!text.is_empty()).then(|| tail(text, MAX_PREVIOUS_CHARS));
    }

    /// Forget the carried-over sentence, keeping the app and profile.
    ///
    /// Used when the last delivery is taken back: a transcript the user just
    /// rejected is the last thing that should bias the retry.
    pub fn clear_previous(&self) {
        self.inner.lock_live().previous = None;
    }

    pub fn profile(&self) -> Option<i64> {
        self.inner.lock_live().profile
    }

    pub fn previous(&self) -> Option<String> {
        self.inner.lock_live().previous.clone()
    }
}

/// The last `max` characters of `text`, cut at a word boundary so the prompt
/// never opens mid-word.
fn tail(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text
        .chars()
        .skip(text.chars().count() - max)
        .collect::<String>();
    match cut.find(' ') {
        Some(i) => cut[i + 1..].to_string(),
        None => cut,
    }
}

/// Assemble the decoder prompt: vocabulary first, then the preceding sentence
/// nearest the audio it precedes.
pub fn compose(terms: Option<String>, previous: Option<String>) -> Option<String> {
    match (terms, previous) {
        (Some(t), Some(p)) => Some(format!("{t}. {p}")),
        (Some(t), None) => Some(t),
        (None, Some(p)) => Some(p),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_to_another_app_drops_the_carried_sentence() {
        let ctx = PromptContext::default();
        ctx.set_app(Some("code.exe".into()), Some(1));
        ctx.set_previous("deploying the ingress controller");
        assert!(ctx.previous().is_some());

        // Same app again: context survives, which is the whole point.
        ctx.set_app(Some("code.exe".into()), Some(1));
        assert!(ctx.previous().is_some());

        ctx.set_app(Some("outlook.exe".into()), None);
        assert_eq!(ctx.previous(), None, "editor text must not bias an email");
        assert_eq!(ctx.profile(), None);
    }

    #[test]
    fn an_undone_transcript_stops_biasing_the_retry() {
        let ctx = PromptContext::default();
        ctx.set_app(Some("code.exe".into()), None);
        ctx.set_previous("wrong words");
        ctx.clear_previous();
        assert_eq!(ctx.previous(), None);
    }

    #[test]
    fn context_is_capped_and_never_opens_mid_word() {
        let ctx = PromptContext::default();
        let long = "alpha bravo ".repeat(60);
        ctx.set_previous(&long);
        let kept = ctx.previous().unwrap();
        assert!(kept.chars().count() <= MAX_PREVIOUS_CHARS, "{}", kept.len());
        assert!(
            ["alpha", "bravo"].contains(&kept.split(' ').next().unwrap()),
            "started mid-word: {kept:?}"
        );
    }

    #[test]
    fn blank_transcripts_are_not_context() {
        let ctx = PromptContext::default();
        ctx.set_previous("   ");
        assert_eq!(ctx.previous(), None);
    }

    #[test]
    fn compose_keeps_whichever_halves_exist() {
        assert_eq!(
            compose(Some("Kubernetes".into()), Some("we deployed it".into())),
            Some("Kubernetes. we deployed it".into())
        );
        assert_eq!(
            compose(Some("Kubernetes".into()), None),
            Some("Kubernetes".into())
        );
        assert_eq!(
            compose(None, Some("we deployed it".into())),
            Some("we deployed it".into())
        );
        assert_eq!(compose(None, None), None);
    }
}
