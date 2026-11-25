//! Interface translations. Command ids, document text and layout names stay stable; only
//! interface chrome is translated. Untranslated labels fall back to English, so coverage can grow
//! incrementally.
//!
//! The language is process-wide (one window per process shows one language): `DECKCRAFT_LANG`
//! (`en`, `es`, `pt`) wins, then the system language, then English. The control channel's
//! `app.language.set` switches it while running. Tests that never call [`set`] see English.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

mod catalog;

/// English label → (Spanish, Brazilian Portuguese).
fn table() -> &'static HashMap<&'static str, (&'static str, &'static str)> {
    static TABLE: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> = OnceLock::new();
    TABLE.get_or_init(|| catalog::CATALOG.iter().map(|(en, es, pt)| (*en, (*es, *pt))).collect())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    En,
    Es,
    PtBr,
}

// One language per process; tests run in parallel threads, so each test thread keeps its own and
// a test that switches to Spanish never changes what another test sees.
#[cfg(not(test))]
static CURRENT: AtomicU8 = AtomicU8::new(0);
#[cfg(test)]
thread_local! {
    static CURRENT: AtomicU8 = const { AtomicU8::new(0) };
}

fn load() -> u8 {
    #[cfg(not(test))]
    {
        CURRENT.load(Ordering::Relaxed)
    }
    #[cfg(test)]
    {
        CURRENT.with(|c| c.load(Ordering::Relaxed))
    }
}

fn store(v: u8) {
    #[cfg(not(test))]
    CURRENT.store(v, Ordering::Relaxed);
    #[cfg(test)]
    CURRENT.with(|c| c.store(v, Ordering::Relaxed));
}

impl Language {
    pub const ALL: [Self; 3] = [Self::En, Self::Es, Self::PtBr];

    /// The code saved and taken by `app.language.set`.
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
            Self::PtBr => "pt",
        }
    }

    /// `es`, `es-AR`, `es_419`, `pt-BR`, `pt`, `en-US`… Anything else is `None`.
    pub fn parse(tag: &str) -> Option<Self> {
        let primary = tag.trim().split(['-', '_', '.']).next().unwrap_or("").to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Self::En),
            "es" => Some(Self::Es),
            "pt" => Some(Self::PtBr),
            _ => None,
        }
    }

    /// The language at startup: `DECKCRAFT_LANG`, then the system's, then English.
    pub fn from_environment() -> Self {
        if let Some(l) = std::env::var("DECKCRAFT_LANG").ok().as_deref().and_then(Self::parse) {
            return l;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(l) = sys_locale::get_locale().as_deref().and_then(Self::parse) {
            return l;
        }
        Self::En
    }

    /// `text` in this language, or `text` itself when it has no translation.
    pub fn tr(self, text: &str) -> &str {
        match self {
            Self::En => text,
            Self::Es => table().get(text).map_or(text, |t| t.0),
            Self::PtBr => table().get(text).map_or(text, |t| t.1),
        }
    }

    const fn index(self) -> u8 {
        match self {
            Self::En => 0,
            Self::Es => 1,
            Self::PtBr => 2,
        }
    }
}

/// The interface language for the whole process.
pub fn current() -> Language {
    match load() {
        1 => Language::Es,
        2 => Language::PtBr,
        _ => Language::En,
    }
}

pub fn set(language: Language) {
    store(language.index());
}

/// `text` in the current interface language.
pub fn tr(text: &str) -> &str {
    current().tr(text)
}

/// `text` with its shortcuts written for this platform: «⇧⌘P» is Mac only, Windows reads «Ctrl+Shift+P»
/// (Zenda's E2E of Work 0.2.30 heard the Mac keys on Windows).
pub fn keys(text: &str) -> std::borrow::Cow<'_, str> {
    if cfg!(target_os = "macos") || !text.contains('⌘') {
        return text.into();
    }
    text.replace("⇧⌘", "Ctrl+Shift+").replace("⌥⌘", "Ctrl+Alt+").replace('⌘', "Ctrl+").into()
}

/// The accessible name of a split control's dropdown half («Bullets options»).
pub fn options_of(name: &str) -> String {
    match current() {
        Language::En => format!("{name} options"),
        Language::Es => format!("Opciones de {name}"),
        Language::PtBr => format!("Opções de {name}"),
    }
}

/// Every English label with a translation, for the coverage test.
pub fn catalog_keys() -> Vec<&'static str> {
    catalog::CATALOG.iter().map(|t| t.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_parse_to_the_three_work_languages() {
        assert_eq!(Language::parse("es-AR"), Some(Language::Es));
        assert_eq!(Language::parse("es_419"), Some(Language::Es));
        assert_eq!(Language::parse("pt-BR"), Some(Language::PtBr));
        assert_eq!(Language::parse("en-US"), Some(Language::En));
        assert_eq!(Language::parse("fr-FR"), None);
    }

    #[test]
    fn untranslated_text_falls_back_to_english() {
        assert_eq!(Language::Es.tr("Home"), "Inicio");
        assert_eq!(Language::PtBr.tr("Home"), "Página Inicial");
        assert_eq!(Language::Es.tr("No such label 123"), "No such label 123");
        assert_eq!(Language::En.tr("Home"), "Home");
    }

    /// One row per English label, line breaks kept (a two-line ribbon label stays two lines),
    /// ellipses kept (a «…» still says a dialog follows), and most rows really translated.
    #[test]
    fn the_catalog_is_well_formed() {
        let keys = catalog_keys();
        assert!(keys.len() >= 200, "catalog shrank: {}", keys.len());
        let unique: std::collections::HashSet<_> = keys.iter().collect();
        assert_eq!(unique.len(), keys.len(), "duplicate English label");
        let mut same = 0;
        for (en, es, pt) in catalog::CATALOG {
            for v in [es, pt] {
                assert!(!v.trim().is_empty(), "empty translation: {en:?}");
                assert_eq!(en.contains('\n'), v.contains('\n'), "line break: {en:?} -> {v:?}");
                assert_eq!(en.ends_with('…'), v.ends_with('…'), "ellipsis: {en:?} -> {v:?}");
            }
            same += usize::from(en == es);
        }
        assert!(same * 20 < keys.len(), "{same} Spanish rows equal English");
    }
}
