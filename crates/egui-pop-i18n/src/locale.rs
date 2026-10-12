//! Locale tags: their canonical form, which one to use, and the chain a
//! lookup falls back along.

use unic_langid::LanguageIdentifier;

/// The source language: the English written in the code and in the `en`
/// catalogue, where every chain ends.
pub const ENGLISH: &str = "en";

/// The pseudo-locale: English, accented and lengthened, to see text that
/// is not translated and layouts that do not absorb longer strings.
pub const PSEUDO: &str = "x-pseudo";

/// A locale tag in its canonical form (`fr-CA`, `x-pseudo`), from a tag or
/// a POSIX locale name (`fr_CA.UTF-8@euro`); `None` when it names no
/// language. `C` and `POSIX` are English.
///
/// ```
/// use egui_pop_i18n::locale::normalize;
///
/// assert_eq!(normalize("fr_CA.UTF-8").as_deref(), Some("fr-CA"));
/// assert_eq!(normalize("C").as_deref(), Some("en"));
/// assert_eq!(normalize("??"), None);
/// ```
pub fn normalize(tag: &str) -> Option<String> {
    let tag = tag.trim();
    let tag = tag.split(['.', '@']).next().unwrap_or_default();
    if tag.is_empty() {
        return None;
    }
    if tag == "C" || tag == "POSIX" {
        return Some(ENGLISH.into());
    }
    let tag = tag.replace('_', "-");
    if tag.eq_ignore_ascii_case(PSEUDO) {
        return Some(PSEUDO.into());
    }
    tag.parse::<LanguageIdentifier>()
        .ok()
        .filter(|l| l.language.as_str() != "und")
        .map(|l| l.to_string())
}

/// The locale to use: `env` (an environment variable's value), then
/// `setting` (the user's choice), then `system`, then English. A value
/// that names no language is skipped.
pub fn choose(env: Option<&str>, setting: Option<&str>, system: Option<&str>) -> String {
    [env, setting, system]
        .into_iter()
        .flatten()
        .find_map(normalize)
        .unwrap_or_else(|| ENGLISH.into())
}

/// The system's language, if it says.
pub fn system() -> Option<String> {
    sys_locale::get_locale()
}

/// The language subtag of a locale (`fr` for `fr-CA`).
pub fn language(locale: &str) -> &str {
    locale.split('-').next().unwrap_or(locale)
}

/// The locales a lookup tries, most specific first, ending with English:
/// `zh-Hant-TW` gives `zh-Hant-TW`, `zh-Hant`, `zh`, `en`. When the
/// language has no catalogue of its own but a regional one is
/// `available` (`pt-BR` for `pt-PT`), that one is tried after the
/// language. The pseudo-locale reads English.
///
/// ```
/// use egui_pop_i18n::locale::chain;
///
/// assert_eq!(chain("fr-CA", &[]), ["fr-CA", "fr", "en"]);
/// assert_eq!(chain("pt-PT", &["pt-BR"]), ["pt-PT", "pt", "pt-BR", "en"]);
/// ```
pub fn chain(locale: &str, available: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !out.contains(&s) {
            out.push(s);
        }
    };
    if let Some(mut l) = (locale != PSEUDO)
        .then(|| locale.parse::<LanguageIdentifier>().ok())
        .flatten()
    {
        push(l.to_string());
        l.clear_variants();
        push(l.to_string());
        l.region = None;
        push(l.to_string());
        l.script = None;
        let lang = l.to_string();
        push(lang.clone());
        if !available.contains(&lang.as_str()) {
            let mut regional: Vec<&str> = available
                .iter()
                .copied()
                .filter(|a| language(a) == lang && *a != lang)
                .collect();
            regional.sort_unstable();
            if let Some(r) = regional.first() {
                push((*r).to_string());
            }
        }
    }
    push(ENGLISH.into());
    out
}

/// A language's own name (`Français` for `fr`), for the common ones;
/// the tag itself otherwise.
pub fn autonym(locale: &str) -> String {
    let name = match locale {
        "x-pseudo" => "Pseudo-locale",
        "pt-BR" => "Português (Brasil)",
        "zh-Hans" => "中文（简体）",
        "zh-Hant" => "中文（繁體）",
        _ => match language(locale) {
            "en" => "English",
            "fr" => "Français",
            "de" => "Deutsch",
            "es" => "Español",
            "it" => "Italiano",
            "pt" => "Português",
            "nl" => "Nederlands",
            "pl" => "Polski",
            "cs" => "Čeština",
            "sv" => "Svenska",
            "da" => "Dansk",
            "nb" => "Norsk bokmål",
            "fi" => "Suomi",
            "hu" => "Magyar",
            "ro" => "Română",
            "tr" => "Türkçe",
            "el" => "Ελληνικά",
            "ru" => "Русский",
            "uk" => "Українська",
            "ja" => "日本語",
            "ko" => "한국어",
            "zh" => "中文",
            "ar" => "العربية",
            "he" => "עברית",
            _ => return locale.to_string(),
        },
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_wins_then_the_setting_then_the_system() {
        assert_eq!(choose(Some("fr_FR.UTF-8"), Some("de"), Some("it")), "fr-FR");
        assert_eq!(choose(None, Some("de"), Some("it-IT")), "de");
        assert_eq!(choose(Some(""), None, Some("it-IT")), "it-IT");
        assert_eq!(choose(Some("??"), Some(" "), None), "en");
        assert_eq!(choose(Some("C"), Some("de"), None), "en");
        assert_eq!(choose(Some("x-pseudo"), None, None), PSEUDO);
    }

    #[test]
    fn the_chain_drops_subtags_then_ends_in_english() {
        assert_eq!(chain("fr-CA", &[]), ["fr-CA", "fr", "en"]);
        assert_eq!(
            chain("zh-Hant-TW", &[]),
            ["zh-Hant-TW", "zh-Hant", "zh", "en"]
        );
        assert_eq!(chain("en-GB", &[]), ["en-GB", "en"]);
        assert_eq!(chain(PSEUDO, &[]), ["en"]);
        assert_eq!(chain("pt", &["pt", "pt-BR"]), ["pt", "en"]);
        assert_eq!(chain("pt", &["pt-PT", "pt-BR"]), ["pt", "pt-BR", "en"]);
    }

    #[test]
    fn autonyms() {
        assert_eq!(autonym("fr-CA"), "Français");
        assert_eq!(autonym("en"), "English");
        assert_eq!(autonym("tlh"), "tlh");
    }
}
