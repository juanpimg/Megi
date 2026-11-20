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

/// All user-visible strings of the generated pages and shell chrome for one locale.
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
    /// Short label for an empty history menu in the chrome UI.
    pub no_history_menu: &'static str,
    /// Fallback title for a blank/new tab.
    pub new_tab: &'static str,
    // Context menu / chrome labels.
    pub open_link_in_new_tab: &'static str,
    pub copy_link_address: &'static str,
    pub save_image: &'static str,
    pub copy_image_unsupported: &'static str,
    pub reload: &'static str,
    pub save_page_as_png: &'static str,
    pub save_page_as_html: &'static str,
    pub save_page_resources: &'static str,
    /// Marker appended to the window title when the page was clipped.
    pub capped_marker: &'static str,
    /// Singular warning label in the window title.
    pub warning: &'static str,
    /// Plural warning label in the window title.
    pub warnings: &'static str,
    /// Singular script-error label in the window title.
    pub script_error: &'static str,
    /// Plural script-error label in the window title.
    pub script_errors: &'static str,
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
            no_history_menu: "No history yet",
            new_tab: "New Tab",
            open_link_in_new_tab: "Open link in new tab",
            copy_link_address: "Copy link address",
            save_image: "Save image",
            copy_image_unsupported: "Copy image (not supported)",
            reload: "Reload",
            save_page_as_png: "Save page as PNG",
            save_page_as_html: "Save page as HTML",
            save_page_resources: "Download page resources",
            capped_marker: " [clipped]",
            warning: "warning",
            warnings: "warnings",
            script_error: "script error",
            script_errors: "script errors",
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
            no_history_menu: "Aún no hay historial",
            new_tab: "Nueva pestaña",
            open_link_in_new_tab: "Abrir enlace en una pestaña nueva",
            copy_link_address: "Copiar dirección del enlace",
            save_image: "Guardar imagen",
            copy_image_unsupported: "Copiar imagen (no disponible)",
            reload: "Recargar",
            save_page_as_png: "Guardar página como PNG",
            save_page_as_html: "Guardar página como HTML",
            save_page_resources: "Descargar recursos de la página",
            capped_marker: " [recortado]",
            warning: "aviso",
            warnings: "avisos",
            script_error: "error de script",
            script_errors: "errores de script",
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
                s.press_r_html,
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
                s.no_history_menu,
                s.new_tab,
                s.open_link_in_new_tab,
                s.copy_link_address,
                s.save_image,
                s.copy_image_unsupported,
                s.reload,
                s.save_page_as_png,
                s.save_page_as_html,
                s.save_page_resources,
                s.capped_marker,
                s.warning,
                s.warnings,
                s.script_error,
                s.script_errors,
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
