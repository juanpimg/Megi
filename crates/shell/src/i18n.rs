//! Locale selection and the built-in-page strings table (#1523).
//!
//! The generated `about:`/error/directory pages no longer hardcode a mix of Spanish and
//! English. `Locale::current()` reads `MEGI_LANG` (or `LANG`/`LC_MESSAGES`) so a host can
//! choose `es` or `en`; the default stays Spanish for backwards compatibility.

use std::env;

/// UI locales supported by the built-in pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    /// English UI strings.
    En,
    /// Spanish UI strings (the historical default).
    Es,
}

impl Locale {
    /// Resolves the locale from `MEGI_LANG`, `LANG` or `LC_MESSAGES`; unknown values and
    /// the C/POSIX default fall back to Spanish.
    pub fn current() -> Self {
        for key in ["MEGI_LANG", "LANG", "LC_MESSAGES"] {
            let Ok(value) = env::var(key) else {
                continue;
            };
            let value = value.trim().to_ascii_lowercase();
            if value.starts_with("en") {
                return Self::En;
            }
            if value.starts_with("es") {
                return Self::Es;
            }
        }
        Self::Es
    }

    /// The `lang` attribute value for generated pages.
    pub fn html_lang(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
        }
    }
}

/// All user-visible strings of the generated pages for one locale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strings {
    pub error_loading: &'static str,
    pub error_code: &'static str,
    pub retry: &'static str,
    pub press_r: &'static str,
    /// `press_r` with the `R` key wrapped in `<code>` for the generated HTML.
    pub press_r_html: &'static str,
    pub history: &'static str,
    pub no_history: &'static str,
    pub title_column: &'static str,
    pub url_column: &'static str,
    pub read_from: &'static str,
    pub index_of: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub size: &'static str,
    pub directory: &'static str,
    pub file: &'static str,
    pub truncated: &'static str,
}

/// Returns the strings table for `locale`.
pub fn strings(locale: Locale) -> Strings {
    match locale {
        Locale::En => Strings {
            error_loading: "Error loading page",
            error_code: "Error code",
            retry: "Retry",
            press_r: "Press R to retry.",
            press_r_html: "Press <code>R</code> to retry.",
            history: "History",
            no_history: "No history yet. Navigate to some pages first.",
            title_column: "Title",
            url_column: "URL",
            read_from: "Read from",
            index_of: "Index of",
            name: "Name",
            kind: "Type",
            size: "Size",
            directory: "Directory",
            file: "File",
            truncated: "listing truncated",
        },
        Locale::Es => Strings {
            error_loading: "Error al cargar la página",
            error_code: "Código de error",
            retry: "Reintentar",
            press_r: "Pulsa R para reintentar.",
            press_r_html: "Pulsa <code>R</code> para reintentar.",
            history: "Historial",
            no_history: "Aún no hay historial. Navega a algunas páginas primero.",
            title_column: "Título",
            url_column: "URL",
            read_from: "Leído desde",
            index_of: "Índice de",
            name: "Nombre",
            kind: "Tipo",
            size: "Tamaño",
            directory: "Directorio",
            file: "Archivo",
            truncated: "listado truncado",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_locales_cover_every_string() {
        for locale in [Locale::En, Locale::Es] {
            let s = strings(locale);
            for value in [
                s.error_loading,
                s.error_code,
                s.retry,
                s.press_r,
                s.history,
                s.no_history,
                s.title_column,
                s.url_column,
                s.read_from,
                s.index_of,
                s.name,
                s.kind,
                s.size,
                s.directory,
                s.file,
                s.truncated,
            ] {
                assert!(!value.is_empty(), "{locale:?} has an empty string");
            }
        }
    }

    #[test]
    fn locale_matches_html_lang() {
        assert_eq!(Locale::En.html_lang(), "en");
        assert_eq!(Locale::Es.html_lang(), "es");
    }
}
