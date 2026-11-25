//! Dispatch for shortcuts migrated out of the keyboard event handler.
//!
//! Extracted from `events.rs` (which is close to the 500-line cap) so the rail
//! toggle can join the migrated actions without changing any existing binding.

use super::shortcuts::ShellAction;
use super::*;

/// Applies a migrated shell action and reports whether the key was consumed.
pub(super) fn dispatch(app: &mut WindowApp, action: Option<ShellAction>) -> bool {
    match action {
        Some(ShellAction::ToggleReader) => app.open_reader(),
        Some(ShellAction::ToggleHistory) => {
            if app.history_menu.is_some() {
                app.history_menu = None;
            } else {
                let rect = chrome::address_bar_rect(app.metrics.css_width as f32);
                app.history_menu = Some(HistoryMenu::open(
                    app.recent_history.menu_entries(),
                    rect.x,
                    rect.y + rect.height,
                ));
            }
            app.request_repaint();
        }
        Some(ShellAction::SaveFrame) => app.save_frame_png(),
        Some(ShellAction::SaveAs) => {
            // Standard "Save As" shortcut opens the visual dialog (#988).
            app.open_save_dialog(true);
        }
        Some(ShellAction::FocusAddressBar) => app.open_address_bar(),
        Some(ShellAction::OpenFind) => app.open_find(),
        Some(ShellAction::PrintDocumentInfo) => app.print_document_info(),
        Some(ShellAction::NewTab) => app.open_new_tab(),
        Some(ShellAction::CloseTab) => app.close_active_tab(),
        Some(ShellAction::CycleTab { next }) => app.cycle_tab(next),
        Some(ShellAction::ActivateTab(index)) => app.activate_tab(index),
        Some(ShellAction::ToggleRail) => app.toggle_rail(),
        // Compact mode and workspaces land in later phases: resolve only.
        Some(ShellAction::ToggleCompact) | Some(ShellAction::CycleWorkspace) => {}
        _ => {}
    }
    // Only migrated actions return early; all other bindings keep falling through
    // to the legacy handlers below, preserving their original behaviour.
    matches!(
        action,
        Some(
            ShellAction::ToggleReader
                | ShellAction::ToggleHistory
                | ShellAction::SaveFrame
                | ShellAction::SaveAs
                | ShellAction::FocusAddressBar
                | ShellAction::OpenFind
                | ShellAction::PrintDocumentInfo
                | ShellAction::NewTab
                | ShellAction::CloseTab
                | ShellAction::CycleTab { .. }
                | ShellAction::ActivateTab(_)
                | ShellAction::ToggleRail
        )
    )
}
