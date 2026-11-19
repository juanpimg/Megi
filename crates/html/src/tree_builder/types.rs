//! Shared WHATWG tree-builder types and builder state.

use crate::fast_hash::{FastMap, FastSet};
use crate::scripting::ScriptExecutor;
use megi_core::{Document, Namespace, NodeId};

/// All 21 insertion modes of the WHATWG HTML Living Standard (§13.2.6.4.1 – §13.2.6.4.21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum InsertionMode {
    /// 13.2.6.4.1 The "initial" insertion mode
    #[default]
    Initial,
    /// 13.2.6.4.2 The "before html" insertion mode
    BeforeHtml,
    /// 13.2.6.4.3 The "before head" insertion mode
    BeforeHead,
    /// 13.2.6.4.4 The "in head" insertion mode
    InHead,
    /// 13.2.6.4.5 The "in head noscript" insertion mode
    InHeadNoscript,
    /// 13.2.6.4.6 The "after head" insertion mode
    AfterHead,
    /// 13.2.6.4.7 The "in body" insertion mode
    InBody,
    /// 13.2.6.4.8 The "text" insertion mode
    Text,
    /// 13.2.6.4.9 The "in table" insertion mode
    InTable,
    /// 13.2.6.4.10 The "in table text" insertion mode
    InTableText,
    /// 13.2.6.4.11 The "in caption" insertion mode
    InCaption,
    /// 13.2.6.4.12 The "in column group" insertion mode
    InColumnGroup,
    /// 13.2.6.4.13 The "in table body" insertion mode
    InTableBody,
    /// 13.2.6.4.14 The "in row" insertion mode
    InRow,
    /// 13.2.6.4.15 The "in cell" insertion mode
    InCell,
    /// 13.2.6.4.16 The "in template" insertion mode
    InTemplate,
    /// 13.2.6.4.17 The "after body" insertion mode
    AfterBody,
    /// 13.2.6.4.18 The "in frameset" insertion mode
    InFrameset,
    /// 13.2.6.4.19 The "after frameset" insertion mode
    AfterFrameset,
    /// 13.2.6.4.20 The "after after body" insertion mode
    AfterAfterBody,
    /// 13.2.6.4.21 The "after after frameset" insertion mode
    AfterAfterFrameset,
}

/// Entry in the List of Active Formatting Elements (§13.2.4.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormattingEntry {
    Marker,
    Element {
        node_id: NodeId,
        tag_name: String,
        attributes: Vec<(String, String)>,
    },
}

/// Insertion location determined by "appropriate place for inserting a node" (§13.2.6.1).
#[derive(Clone, Copy, Debug)]
pub(crate) struct AdjustedInsertionLocation {
    pub(crate) parent: NodeId,
    pub(crate) insert_before_sibling: Option<NodeId>,
}

/// Hard parsing limits applied by `parse_document_with_options` /
/// `parse_fragment_with_options`.
///
/// These are **anti-DoS defenses**, not conformance knobs: real-world pages and the
/// complete `html5lib-tests` corpus stay far below every default, so enabling the
/// defaults never changes the resulting tree for conforming input. Hostile input that
/// exceeds a limit is handled by *ignoring* further insertions and counting a
/// [`LimitViolations`] entry instead of panicking, aborting or recursing without bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseOptions {
    /// Maximum element nesting depth (length of the stack of open elements). Deeper
    /// start tags are dropped. Bounds the recursion depth of downstream consumers
    /// (layout, paint) as well.
    pub max_depth: usize,
    /// Maximum number of nodes allocated in the document arena. Further start tags,
    /// comments and text are dropped.
    pub max_nodes: usize,
    /// Maximum number of attributes kept on a single element. Excess attributes are
    /// dropped (the first ones win, matching the duplicate-attribute rule).
    pub max_attributes_per_element: usize,
    /// Maximum number of input bytes tokenized. The input is truncated on a UTF-8
    /// character boundary; a violation is recorded in the outcome.
    pub max_input_bytes: usize,
    /// Maximum number of text bytes inserted into the tree. Character data coalesces into a
    /// single text node, so the node budget alone does not bound a document made of one huge
    /// text run; excess bytes are dropped and counted (#642).
    pub max_text_bytes: usize,
    /// Maximum number of bytes a single parse may receive through `document.write`
    /// (parser-inserted scripts). Each written chunk is re-tokenized at the current position
    /// and may write again, so without a budget a self-reproducing script loops forever (#639).
    pub max_written_bytes: usize,
    /// Maximum number of parser-inserted script executions in one parse (#639).
    pub max_script_executions: usize,
    /// Maximum number of bytes of script source handed to the executor in one script (#639).
    /// The text is the concatenation of the script element's text children, and the executor
    /// is a host callback that may copy it.
    pub max_script_text_bytes: usize,
    /// Maximum number of entries in the active formatting list. The Noah's Ark clause only
    /// removes a fourth *identical* entry, so crafters grow the list with distinct
    /// `(tag, attributes)` pairs and then pay for `reconstruct_active_formatting_elements`
    /// on every character run (#643).
    pub max_active_formatting: usize,
}

impl ParseOptions {
    /// Generous defaults, well above every official html5lib fixture (largest fixture is
    /// a few KB, at most a handful of attributes and nesting levels), but far below the
    /// input sizes that make the tokenizer/tree builder consume seconds of CPU or
    /// exhaust the stack.
    pub const DEFAULT: Self = Self {
        max_depth: 4096,
        max_nodes: 1_000_000,
        max_attributes_per_element: 4096,
        max_input_bytes: 16 * 1024 * 1024,
        max_text_bytes: 16 * 1024 * 1024,
        max_active_formatting: 256,
        max_written_bytes: 4 * 1024 * 1024,
        max_script_executions: 1024,
        max_script_text_bytes: 4 * 1024 * 1024,
    };
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Counter of parsing limits hit while processing a document; see [`ParseOptions`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LimitViolations {
    /// The input was truncated because it exceeded [`ParseOptions::max_input_bytes`].
    pub input_truncated: bool,
    /// Number of start tags ignored because [`ParseOptions::max_depth`] was reached.
    pub depth_exceeded: u64,
    /// Number of tokens ignored because [`ParseOptions::max_nodes`] was reached.
    pub nodes_exceeded: u64,
    /// Number of attributes dropped because
    /// [`ParseOptions::max_attributes_per_element`] was exceeded.
    pub attributes_dropped: u64,
    /// Number of tokens discarded because the adjusted insertion location was the
    /// document root (e.g. after a script detached the document element).
    pub detached_insertions: u64,
    /// Text bytes dropped because [`ParseOptions::max_text_bytes`] was reached (#642).
    pub text_bytes_dropped: u64,
    /// Formatting entries dropped because [`ParseOptions::max_active_formatting`] was
    /// reached (#643).
    pub formatting_dropped: u64,
    /// Bytes refused because [`ParseOptions::max_written_bytes`] was reached (#639).
    pub written_bytes_dropped: u64,
    /// Script executions skipped because [`ParseOptions::max_script_executions`] was
    /// reached (#639).
    pub scripts_skipped: u64,
}

impl LimitViolations {
    /// Returns `true` when no limit was exceeded.
    pub const fn is_empty(&self) -> bool {
        !self.input_truncated
            && self.depth_exceeded == 0
            && self.nodes_exceeded == 0
            && self.text_bytes_dropped == 0
            && self.attributes_dropped == 0
            && self.detached_insertions == 0
            && self.formatting_dropped == 0
            && self.written_bytes_dropped == 0
            && self.scripts_skipped == 0
    }

    /// Total number of recorded violations (the input truncation counts as one).
    pub const fn total(&self) -> u64 {
        let truncated = self.input_truncated as u64;
        truncated
            + self.depth_exceeded
            + self.nodes_exceeded
            + self.attributes_dropped
            + self.detached_insertions
            + self.text_bytes_dropped
            + self.formatting_dropped
            + self.written_bytes_dropped
            + self.scripts_skipped
    }
}

/// Result of a limited parse: the produced tree plus which (if any) safety limits fired.
#[derive(Debug)]
pub struct ParseOutcome {
    /// The parsed document (possibly truncated/downgraded by [`ParseOptions`]).
    pub document: Document,
    /// Limits that were exceeded while parsing.
    pub limit_violations: LimitViolations,
    /// Non-fatal errors from parser-inserted scripts (WP-6.4). Empty when no executor is
    /// installed, scripting is off, or every script ran successfully.
    pub script_errors: Vec<String>,
}

/// WHATWG HTML §13.2.6 Tree Builder state machine.
pub struct TreeBuilder {
    pub doc: Document,
    pub mode: InsertionMode,
    pub original_mode: InsertionMode,
    pub open_elements: Vec<NodeId>,
    pub active_formatting: Vec<FormattingEntry>,
    pub template_modes: Vec<InsertionMode>,
    pub head_element: Option<NodeId>,
    pub form_element: Option<NodeId>,
    pub scripting_enabled: bool,
    pub frameset_ok: bool,
    pub foster_parenting: bool,
    pub skip_next_lf: bool,
    pub fragment_context: Option<(Namespace, String)>,
    pub fragment_context_node: Option<NodeId>,
    pub pending_table_characters: Vec<char>,
    /// Encoded byte length of `pending_table_characters` (#1737).
    pub pending_table_char_bytes: usize,
    /// Hard safety limits for this parse (anti-DoS).
    pub parse_options: ParseOptions,
    /// Limits that have been exceeded so far.
    pub limit_violations: LimitViolations,
    /// Text bytes already inserted into the tree (bounded by
    /// [`ParseOptions::max_text_bytes`], see #642).
    pub(crate) text_bytes: usize,
    /// Whether any HTML `<select>` element was inserted; lets the end-of-parse
    /// `<selectedcontent>` fixup (which walks the whole tree) be skipped entirely for
    /// the overwhelmingly common documents without a `<select>`.
    pub(crate) has_select_element: bool,
    /// Anti-quadratic fast path for §13.2.4.2 scope checks: number of currently-open
    /// HTML-namespace elements per local name (audit finding F2). "The tag is not open at
    /// all" answers `has_element_in_*_scope` in O(1) instead of scanning the whole stack.
    pub(crate) open_html_counts: FastMap<String, u32>,
    /// O(1) membership test for the stack of open elements (used by the active-formatting
    /// reconstruction, which previously scanned the stack once per character token).
    pub(crate) open_ids: FastSet<NodeId>,
    /// Number of currently-open non-HTML (SVG/MathML) elements. Allows the per-token
    /// foreign-content dispatch to skip all node lookups for HTML-only pages.
    pub(crate) open_foreign_count: u32,
    /// WP-6.4: host-provided executor for parser-inserted `<script>` elements. `None` keeps
    /// the parser byte-for-byte identical to the pre-WP-6.4 parser (scripts are not run).
    pub(crate) script_executor: Option<Box<dyn ScriptExecutor>>,
    /// HTML written through `document.write` by the script processed in the current token.
    /// The tokenizer driver moves it into the tokenizer right after the token is handled.
    pub(crate) pending_written_html: String,
    /// Bytes accepted through `document.write` during this parse (#639).
    pub(crate) written_bytes: usize,
    /// Parser-inserted scripts executed during this parse (#639).
    pub(crate) script_executions: usize,
    /// Non-fatal parser-inserted script errors, exposed through [`ParseOutcome::script_errors`].
    pub(crate) script_errors: Vec<String>,
    /// Whether this parse ever created an `<html>` element. A script may detach it
    /// (`document.documentElement.remove()`); the end-of-parse synthetic html/head/body fixup
    /// must then be skipped, or it would resurrect a detached skeleton (`scripted_foster01`).
    pub(crate) html_element_seen: bool,
}
