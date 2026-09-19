/*!
 * SOURCE OF TRUTH KEYWORDS: BlockedPhrase, DropRule, blocklist_for, UNIVERSAL,
 *   ENGLISH, always, when_silent
 * WHAT:  The per-language table of phrases whisper is known to invent, and how
 *        confidently each may be removed.
 * WHY:   Data, kept apart from the matching logic in hallucination.rs, because
 *        this table is the part that grows. Every new entry is a phrase someone
 *        watched appear in their document, and adding one should be a line in a
 *        list rather than a change to a function that decides what gets
 *        deleted.
 *        The phrases are stored ALREADY NORMALISED — lowercase, no punctuation,
 *        single-spaced — because they are compared against normalised segment
 *        text. An entry written with its original punctuation would silently
 *        never match, which is the failure mode that makes a blocklist look
 *        like it works.
 * WHERE: Read by core/asr/hallucination.rs::is_hallucination. The table is
 *        ported from murmur, whose entries were each a phrase someone watched
 *        appear in their own document.
 */

/**
 * SOURCE OF TRUTH KEYWORDS: DropRule
 * WHAT:  How confidently a blocklisted phrase can be removed.
 * WHY:   Two tiers, because the entries differ in risk. Nobody dictates
 *        "Subtitles by the Amara.org community" as their whole utterance, so
 *        that is unconditional. Somebody might dictate "You." — so that one
 *        also needs the chunk to have been too quiet to contain speech before
 *        we act on it.
 * WHERE: Carried by every BlockedPhrase; read by is_hallucination.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropRule {
    /// Not a plausible dictation, whatever the audio looked like.
    Always,
    /// Plausible as speech; drop only when the audio was too quiet to be any.
    WhenLikelySilence,
}

#[derive(Debug, Clone, Copy)]
pub struct BlockedPhrase {
    /// Already normalised: lowercase, punctuation stripped, single-spaced.
    pub text: &'static str,
    pub rule: DropRule,
}

const fn always(text: &'static str) -> BlockedPhrase {
    BlockedPhrase {
        text,
        rule: DropRule::Always,
    }
}

const fn when_silent(text: &'static str) -> BlockedPhrase {
    BlockedPhrase {
        text,
        rule: DropRule::WhenLikelySilence,
    }
}

/// Subtitle furniture and credit strings. Language-independent because whisper
/// emits them regardless of the pinned language.
///
/// Ported from murmur, whose table was built from phrases people watched appear
/// in their own documents. Echo covers more languages in its UI than this list
/// does; a language with no entry falls back to UNIVERSAL rather than to
/// English, which is the safe direction — see `blocklist_for`.
const UNIVERSAL: &[BlockedPhrase] = &[
    always("subtitles by the amaraorg community"),
    always("subtitles by the amara org community"),
    always("subtitles by"),
    always("subtitles amaraorg"),
    always("amaraorg"),
    always("music"),
    always("applause"),
    always("laughter"),
    always("silence"),
    always("blank audio"),
    always("inaudible"),
    always("beep"),
];

const ENGLISH: &[BlockedPhrase] = &[
    always("thanks for watching"),
    always("thank you for watching"),
    always("thanks for watching everyone"),
    always("please subscribe"),
    always("please subscribe to my channel"),
    always("subscribe to my channel"),
    always("like and subscribe"),
    always("dont forget to subscribe"),
    always("see you in the next video"),
    always("see you next time"),
    always("thank you so much for watching"),
    when_silent("thank you"),
    when_silent("thank you very much"),
    when_silent("thanks"),
    when_silent("you"),
    when_silent("bye"),
    when_silent("bye bye"),
    when_silent("okay"),
    when_silent("the end"),
];

const SPANISH: &[BlockedPhrase] = &[
    always("subtitulos realizados por la comunidad de amaraorg"),
    always("gracias por ver el video"),
    always("suscribete al canal"),
    when_silent("gracias"),
    when_silent("muchas gracias"),
];

const FRENCH: &[BlockedPhrase] = &[
    always("sous titres realises par la communaute damaraorg"),
    always("merci davoir regarde cette video"),
    when_silent("merci"),
    when_silent("merci beaucoup"),
    when_silent("au revoir"),
];

const GERMAN: &[BlockedPhrase] = &[
    always("untertitel von stephanie geiges"),
    always("untertitelung aufgrund der amaraorg community"),
    always("vielen dank furs zuschauen"),
    when_silent("danke"),
    when_silent("vielen dank"),
    when_silent("tschuss"),
];

const PORTUGUESE: &[BlockedPhrase] = &[
    always("legendas pela comunidade amaraorg"),
    always("obrigado por assistir"),
    when_silent("obrigado"),
];

const ITALIAN: &[BlockedPhrase] = &[
    always("sottotitoli e revisione a cura di qtss"),
    always("grazie per aver guardato il video"),
    when_silent("grazie"),
    when_silent("grazie mille"),
];

const RUSSIAN: &[BlockedPhrase] = &[
    always("субтитры сделал димтриж"),
    always("редактор субтитров"),
    always("продолжение следует"),
    when_silent("спасибо"),
    when_silent("спасибо за просмотр"),
];

const JAPANESE: &[BlockedPhrase] = &[
    always("ご視聴ありがとうございました"),
    always("チャンネル登録お願いします"),
    when_silent("ありがとうございました"),
    when_silent("おやすみなさい"),
];

const KOREAN: &[BlockedPhrase] = &[
    always("시청해주셔서 감사합니다"),
    always("구독과 좋아요 부탁드립니다"),
    when_silent("감사합니다"),
];

const CHINESE: &[BlockedPhrase] = &[
    always("请不吝点赞 订阅 转发 打赏支持明镜与点点栏目"),
    always("感谢观看"),
    always("字幕由amaraorg社区提供"),
    when_silent("谢谢"),
    when_silent("谢谢大家"),
];

const HINDI: &[BlockedPhrase] = &[
    always("देखने के लिए धन्यवाद"),
    when_silent("धन्यवाद"),
    when_silent("शुक्रिया"),
];

const ARABIC: &[BlockedPhrase] = &[
    always("ترجمة نانسي قنقر"),
    always("شكرا للمشاهدة"),
    when_silent("شكرا"),
    when_silent("شكرا لكم"),
];

/**
 * WHAT:  The blocklist that applies to a segment in the given language.
 * WHY:   Languages fall back to the universal list rather than to English —
 *        English single-word entries applied to a language we have no list for
 *        would be dropping words on a guess.
 * WHERE: Called by is_hallucination.
 */
pub fn blocklist_for(language: Option<&str>) -> impl Iterator<Item = &'static BlockedPhrase> {
    let specific: &'static [BlockedPhrase] = match language {
        Some("en") => ENGLISH,
        Some("es") => SPANISH,
        Some("fr") => FRENCH,
        Some("de") => GERMAN,
        Some("pt") => PORTUGUESE,
        Some("it") => ITALIAN,
        Some("ru") => RUSSIAN,
        Some("ja") => JAPANESE,
        Some("ko") => KOREAN,
        Some("zh") => CHINESE,
        Some("hi") => HINDI,
        Some("ar") => ARABIC,
        _ => &[],
    };
    UNIVERSAL.iter().chain(specific.iter())
}
