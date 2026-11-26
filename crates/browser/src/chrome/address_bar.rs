//! Address bar: editing state, URL resolution, geometry and painting, plus the
//! visual history button that lives at the right edge of the address strip.

use megi_webview::layout::Rect;

use super::drawing::*;
use super::*;

/// Address bar edition state: text, caret, selection anchor and an **internal process
/// clipboard** (Ctrl+C/X/V copy within the shell; the system clipboard is not touched).
///
/// `Ctrl+L` opens the bar with the whole URL selected, so typing replaces it. The caret is
/// painted as a solid deterministic bar (no time-based blinking, so screenshots are stable).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddressBar {
    text: String,
    caret: usize,
    anchor: usize,
    editing: bool,
    clipboard: String,
}

impl AddressBar {
    /// Creates a closed address bar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens edition pre-filled with `url` and selects the whole URL (Ctrl+L behaviour).
    pub fn open(&mut self, url: &str) {
        self.text = url.to_string();
        self.caret = self.text.len();
        self.anchor = 0;
        self.editing = true;
    }

    /// Closes edition (cancelling any unsubmitted text; the internal clipboard survives).
    pub fn cancel(&mut self) {
        self.editing = false;
        self.text.clear();
        self.caret = 0;
        self.anchor = 0;
    }

    /// Whether the address bar is being edited.
    pub const fn is_editing(&self) -> bool {
        self.editing
    }

    /// Current text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Caret byte offset.
    pub const fn caret(&self) -> usize {
        self.caret
    }

    /// Selection as a byte range (`start..end`), empty when collapsed.
    pub fn selection_range(&self) -> std::ops::Range<usize> {
        let start = self.anchor.min(self.caret);
        let end = self.anchor.max(self.caret);
        start..end
    }

    /// Whether a non-empty selection exists.
    pub fn has_selection(&self) -> bool {
        self.anchor != self.caret
    }

    /// Selected text.
    pub fn selected_text(&self) -> &str {
        let range = self.selection_range();
        &self.text[range]
    }

    /// Selects everything.
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }

    pub(crate) fn delete_selection(&mut self) -> bool {
        let range = self.selection_range();
        if range.is_empty() {
            return false;
        }
        self.text.replace_range(range.clone(), "");
        self.caret = range.start;
        self.anchor = self.caret;
        true
    }

    /// Inserts text, replacing the selection (caret ends after the inserted text).
    pub fn insert(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.delete_selection();
        self.text.insert_str(self.caret, text);
        self.caret += text.len();
        self.anchor = self.caret;
        true
    }

    /// Deletes the character before the caret (or the selection).
    pub fn backspace(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let index = prev_char_index(&self.text, self.caret);
        self.text.replace_range(index..self.caret, "");
        self.caret = index;
        self.anchor = index;
        true
    }

    /// Deletes the character after the caret (or the selection).
    pub fn delete_forward(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret >= self.text.len() {
            return false;
        }
        let next = next_char_index(&self.text, self.caret);
        self.text.replace_range(self.caret..next, "");
        true
    }

    /// Deletes the word before the caret (`Ctrl+Backspace`), or the selection.
    pub fn delete_word_back(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let mut index = self.caret;
        // Skip trailing whitespace, then the whole word.
        while index > 0 {
            let previous = prev_char_index(&self.text, index);
            if !self.text[previous..index].chars().all(char::is_whitespace) {
                break;
            }
            index = previous;
        }
        while index > 0 {
            let previous = prev_char_index(&self.text, index);
            if self.text[previous..index].chars().all(char::is_whitespace) {
                break;
            }
            index = previous;
        }
        if index == self.caret {
            return false;
        }
        self.text.replace_range(index..self.caret, "");
        self.caret = index;
        self.anchor = index;
        true
    }

    /// Moves the caret one character left/right; without `extend` the selection collapses.
    pub fn move_caret(&mut self, right: bool, extend: bool) -> bool {
        if !extend && let Some(edge) = self.selection_edge(right) {
            self.caret = edge;
            self.anchor = edge;
            return true;
        }
        let next = if right {
            if self.caret >= self.text.len() {
                return false;
            }
            next_char_index(&self.text, self.caret)
        } else {
            if self.caret == 0 {
                return false;
            }
            prev_char_index(&self.text, self.caret)
        };
        self.caret = next;
        if !extend {
            self.anchor = next;
        }
        true
    }

    fn selection_edge(&self, right: bool) -> Option<usize> {
        if !self.has_selection() {
            return None;
        }
        let range = self.selection_range();
        Some(if right { range.end } else { range.start })
    }

    /// Moves the caret to the start/end of the text (Home/End), extending the selection
    /// when `extend` is set.
    pub fn move_home(&mut self, extend: bool) -> bool {
        let target = 0;
        if self.caret == target && !self.has_selection() {
            return false;
        }
        self.caret = target;
        if !extend {
            self.anchor = target;
        }
        true
    }

    /// Moves the caret to the end of the text.
    pub fn move_end(&mut self, extend: bool) -> bool {
        let target = self.text.len();
        if self.caret == target && !self.has_selection() {
            return false;
        }
        self.caret = target;
        if !extend {
            self.anchor = target;
        }
        true
    }

    /// Copies the selection into the internal clipboard.
    pub fn copy(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }
        self.clipboard = self.selected_text().to_string();
        true
    }

    /// Cuts the selection into the internal clipboard.
    pub fn cut(&mut self) -> bool {
        if !self.copy() {
            return false;
        }
        self.delete_selection()
    }

    /// Pastes the internal clipboard, replacing the selection.
    pub fn paste(&mut self) -> bool {
        if self.clipboard.is_empty() {
            return false;
        }
        let clipboard = self.clipboard.clone();
        self.insert(&clipboard)
    }

    /// Internal clipboard contents (documented: not the system clipboard).
    pub fn clipboard(&self) -> &str {
        &self.clipboard
    }

    /// First unique history URL that starts with the current text (case-insensitive, longer
    /// than the text, at least 2 chars typed). Returns the full completion candidate.
    pub fn autocomplete(&self, history: &[String]) -> Option<String> {
        let typed = self.text.trim();
        if typed.chars().count() < 2 {
            return None;
        }
        let needle = typed.to_ascii_lowercase();
        history
            .iter()
            .find(|url| {
                // A completion must extend the typed text: `contains` matched in the middle
                // ("exa" vs "https://example.com/") and `accept_completion` then selected
                // garbage (`ps://example.com/`, leaving "httm").
                url.len() > typed.len() && url.to_ascii_lowercase().starts_with(&needle)
            })
            .cloned()
    }

    /// Accepts an autocomplete candidate (Tab): completes the text and selects the suffix.
    pub fn accept_completion(&mut self, history: &[String]) -> bool {
        let Some(candidate) = self.autocomplete(history) else {
            return false;
        };
        // `autocomplete` compares the *trimmed* text with the candidate prefix, so the
        // selection anchor must use that trimmed length: with leading/trailing spaces the
        // raw length could exceed the caret and `selected_text` then panicked on a
        // non-boundary/out-of-range slice.
        let typed = self.text.trim().len().min(candidate.len());
        self.text = candidate;
        self.anchor = typed;
        self.caret = self.text.len();
        true
    }

    /// Closes edition and returns whether it was open.
    pub fn finish(&mut self) -> bool {
        let was = self.editing;
        self.editing = false;
        self.anchor = self.caret;
        was
    }
}

/// Previous UTF-8 char boundary before `index`.
fn prev_char_index(text: &str, index: usize) -> usize {
    text[..index]
        .char_indices()
        .next_back()
        .map_or(0, |(offset, _)| offset)
}

/// Next UTF-8 char boundary after `index`.
fn next_char_index(text: &str, index: usize) -> usize {
    text[index..]
        .chars()
        .next()
        .map_or(text.len(), |character| index + character.len_utf8())
}

/// Resolves address-bar text into a URL: absolute URLs are used as-is; bare hosts/domains
/// without a scheme (e.g. `wikipedia.org`, `localhost:8080`) default to `https://` (or
/// `http://` for loopback); explicit relative references (`/path`, `./page`) join `base`.
pub fn resolve_address(
    input: &str,
    base: Option<&megi_webview::net::Url>,
) -> Result<megi_webview::net::Url, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty address".to_string());
    }
    if let Ok(url) = megi_webview::net::Url::parse(input) {
        return Ok(url);
    }
    if !input.starts_with('/')
        && !input.starts_with("./")
        && !input.starts_with("../")
        && !input.starts_with('?')
        && !input.starts_with('#')
        && !input.contains(char::is_whitespace)
    {
        let authority = input.split(['/', '?', '#']).next().unwrap_or("");
        let host = authority.split(':').next().unwrap_or("");
        let is_loopback = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1";
        let looks_like_file_ext = base.is_some()
            && !input.contains('/')
            && host.rsplit_once('.').is_some_and(|(_, ext)| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "html"
                        | "htm"
                        | "xhtml"
                        | "txt"
                        | "xml"
                        | "json"
                        | "css"
                        | "js"
                        | "png"
                        | "jpg"
                        | "jpeg"
                        | "gif"
                        | "webp"
                        | "svg"
                        | "pdf"
                )
            });
        if !looks_like_file_ext && (is_loopback || host.contains('.') || authority.starts_with('['))
        {
            let scheme = if is_loopback { "http://" } else { "https://" };
            if let Ok(url) = megi_webview::net::Url::parse(&format!("{scheme}{input}")) {
                return Ok(url);
            }
        }
    }
    match base {
        Some(base) => base
            .join(input)
            .map_err(|error| format!("cannot resolve address `{input}`: {error}")),
        None => Err(format!("cannot parse address `{input}` as a URL")),
    }
}

/// Address bar rectangle for a chrome width.
pub fn address_bar_rect(width: f32) -> Rect {
    Rect::new(
        4.0,
        TAB_STRIP_HEIGHT + 2.0,
        (width - 8.0).max(1.0),
        ADDRESS_BAR_HEIGHT - 4.0,
    )
}

/// Splits a URL into scheme, authority and path for the address bar's visual hierarchy.
/// Non-hierarchical URLs (`data:`, `about:`) keep everything in the authority part.
pub(crate) fn split_url_for_display(url: &str) -> (&str, &str, &str) {
    let Some(scheme_end) = url.find("://") else {
        return ("", url, "");
    };
    let scheme_end = scheme_end + 3;
    let rest = &url[scheme_end..];
    let authority_end = rest.find('/').unwrap_or(rest.len());
    (
        &url[..scheme_end],
        &rest[..authority_end],
        &rest[authority_end..],
    )
}

/// CSS x where address text (or the favicon that precedes it) starts.
pub(crate) fn address_text_x(width: f32) -> f32 {
    let nav = navigation_buttons(width);
    nav.reload.x + nav.reload.width + 6.0
}

/// History button rectangle (right side of the address bar); only painted when
/// [`ChromeView::history_button`] is set so headless chrome goldens stay unchanged.
pub fn history_button_rect(css_width: f32) -> Rect {
    Rect::new(
        (css_width - 34.0).max(2.0),
        TAB_STRIP_HEIGHT + 5.0,
        28.0,
        28.0,
    )
}

/// Hit-tests the visual history button.
pub fn history_button_hit(css_width: f32, x: f32, y: f32) -> bool {
    contains(history_button_rect(css_width), x, y)
}
