//! Turning Echo's language codes into the locales some providers demand.
//!
//! Echo speaks bare ISO-639 (`en`, `fr`, `pt`) because that is what Whisper
//! takes. Azure and Google want BCP-47 (`en-US`, `fr-FR`, `pt-BR`) and do not
//! accept the short form — Azure rejects it, and Google quietly transcribes as
//! US English regardless of what was actually said. Neither failure explains
//! itself, so the expansion happens here rather than in each provider.
//!
//! The region attached to each language is a *guess at the most common one*,
//! which is the honest framing: `pt` becomes `pt-BR` because Brazil has far
//! more speakers, not because Portugal was considered and rejected. A user who
//! needs a different region can type the full locale into the language setting
//! and it passes through untouched.

/// Expand a bare language code into a locale, passing through anything that is
/// already one.
pub fn to_locale(language: Option<&str>) -> String {
    let Some(lang) = language else {
        // No language pinned means auto-detect, which these providers do not
        // offer. US English is the least surprising default for a fallback
        // nobody asked for — and the UI says the language is unset.
        return "en-US".to_string();
    };

    // Already a locale ("en-GB", "zh-Hant"): the user was specific, so respect it.
    if lang.contains('-') {
        return lang.to_string();
    }

    match lang.to_ascii_lowercase().as_str() {
        "en" => "en-US",
        "es" => "es-ES",
        "fr" => "fr-FR",
        "de" => "de-DE",
        "it" => "it-IT",
        "pt" => "pt-BR",
        "nl" => "nl-NL",
        "pl" => "pl-PL",
        "ru" => "ru-RU",
        "tr" => "tr-TR",
        "ar" => "ar-SA",
        "hi" => "hi-IN",
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        "zh" => "zh-CN",
        "sv" => "sv-SE",
        "da" => "da-DK",
        "no" | "nb" => "nb-NO",
        "fi" => "fi-FI",
        "cs" => "cs-CZ",
        "el" => "el-GR",
        "he" => "he-IL",
        "id" => "id-ID",
        "th" => "th-TH",
        "uk" => "uk-UA",
        "vi" => "vi-VN",
        "ro" => "ro-RO",
        "hu" => "hu-HU",
        // An unknown code is more likely a real language Echo has not tabulated
        // than a typo. Doubling it ("ca" → "ca-CA") is wrong far more often
        // than passing it through, which at least lets the provider decide.
        other => return other.to_string(),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_codes_gain_their_most_common_region() {
        assert_eq!(to_locale(Some("en")), "en-US");
        assert_eq!(to_locale(Some("de")), "de-DE");
        assert_eq!(to_locale(Some("pt")), "pt-BR");
    }

    #[test]
    fn a_locale_the_user_already_specified_is_left_alone() {
        // Someone who typed en-GB meant en-GB, not en-US.
        assert_eq!(to_locale(Some("en-GB")), "en-GB");
        assert_eq!(to_locale(Some("pt-PT")), "pt-PT");
        assert_eq!(to_locale(Some("zh-Hant")), "zh-Hant");
    }

    #[test]
    fn case_does_not_change_the_answer() {
        assert_eq!(to_locale(Some("EN")), "en-US");
        assert_eq!(to_locale(Some("Fr")), "fr-FR");
    }

    #[test]
    fn no_language_falls_back_rather_than_sending_nothing() {
        assert_eq!(to_locale(None), "en-US");
    }

    #[test]
    fn an_untabulated_code_passes_through_instead_of_being_doubled() {
        assert_eq!(to_locale(Some("ca")), "ca");
        assert_eq!(to_locale(Some("sw")), "sw");
    }
}
