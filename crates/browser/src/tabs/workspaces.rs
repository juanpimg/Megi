//! Local workspaces: named, in-memory groups of tabs (phase 3 of the Zen UI plan).
//!
//! Tabs stay in one bounded `Vec` and carry a small workspace id; each workspace
//! remembers its own active tab. Switching never touches another group's tabs,
//! scroll offsets or find state. There are no services, processes, network calls
//! or new dependencies. Session persistence of the workspace id is deliberately
//! deferred to phase 5: a restored session keeps every tab in workspace 0.
//!
//! Invariants maintained by every operation in this module:
//! - every tab id is `< MAX_WORKSPACES` and the active tab belongs to `active_workspace`;
//! - `workspace_active[w]` is `Some(index)` only when `index` is a tab of `w`;
//! - a workspace that holds tabs is never left empty (its last tab is reset to blank).

use super::{CloseOutcome, Tab, TabsState};

/// Maximum number of local workspaces (the `Ctrl+Alt+W` cycle wraps after this).
pub const MAX_WORKSPACES: u8 = 3;

const SLOTS: usize = MAX_WORKSPACES as usize;

/// Identity and localized label of a local workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    /// Zero-based workspace id (`0..MAX_WORKSPACES`).
    pub id: u8,
    /// Localized name shown in the status bar and accessibility names.
    pub name: String,
}

impl Workspace {
    /// Builds the workspace with `id` and its localized label.
    pub fn new(id: u8) -> Self {
        Self {
            id,
            name: Self::label(id),
        }
    }

    /// Localized human-readable label (`Workspace 1`, `Espacio de trabajo 1`, ...).
    pub fn label(id: u8) -> String {
        let strings = crate::i18n::strings(crate::i18n::Locale::current());
        format!("{} {}", strings.workspace, id + 1)
    }
}

/// Initial active slots: workspace 0 holds the first tab of a fresh window.
pub(super) fn initial_slots() -> [Option<usize>; SLOTS] {
    let mut slots = [None; SLOTS];
    slots[0] = Some(0);
    slots
}

/// Rebuilds the per-workspace active slots from a tab list after restore.
pub(super) fn rebuild_slots(tabs: &[Tab], active: usize) -> [Option<usize>; SLOTS] {
    let mut slots = [None; SLOTS];
    for (index, tab) in tabs.iter().enumerate() {
        let workspace = usize::from(tab.workspace).min(SLOTS - 1);
        if slots[workspace].is_none() {
            slots[workspace] = Some(index);
        }
    }
    if let Some(tab) = tabs.get(active) {
        let workspace = usize::from(tab.workspace).min(SLOTS - 1);
        slots[workspace] = Some(active);
    }
    slots
}

impl TabsState {
    /// Id of the currently visible workspace.
    pub fn workspace(&self) -> u8 {
        self.workspace
    }

    /// Identity and localized label of the currently visible workspace.
    pub fn workspace_info(&self) -> Workspace {
        Workspace::new(self.workspace)
    }

    /// Whether more than one workspace currently holds tabs.
    ///
    /// The single-workspace case is byte-for-byte identical to the legacy shell:
    /// no label is painted and no accessibility string is decorated.
    pub fn has_multiple_workspaces(&self) -> bool {
        (0..SLOTS)
            .filter(|workspace| {
                self.tabs
                    .iter()
                    .any(|tab| usize::from(tab.workspace) == *workspace)
            })
            .count()
            > 1
    }

    /// Status-bar text for the active workspace, or `None` with a single group.
    pub fn workspace_status_text(&self) -> Option<String> {
        self.has_multiple_workspaces()
            .then(|| self.workspace_info().name)
    }

    /// Tabs visible in the active workspace, in their stored order.
    pub fn visible_tabs(&self) -> impl Iterator<Item = &Tab> {
        let workspace = self.workspace;
        self.tabs
            .iter()
            .filter(move |tab| tab.workspace == workspace)
    }

    /// Number of tabs visible in the active workspace.
    pub fn visible_len(&self) -> usize {
        self.visible_tabs().count()
    }

    /// Position of the active tab among the visible tabs (0-based).
    pub fn visible_active_index(&self) -> usize {
        self.tabs[..self.active]
            .iter()
            .filter(|tab| tab.workspace == self.workspace)
            .count()
    }

    /// Maps a visible position to a global index into [`TabsState::tabs`].
    pub fn visible_tab_index(&self, visible: usize) -> Option<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.workspace == self.workspace)
            .nth(visible)
            .map(|(index, _)| index)
    }

    /// Records a global tab index as active in its own workspace.
    pub(super) fn note_active(&mut self, index: usize) {
        self.active = index;
        self.workspace = self.tabs[index].workspace;
        self.workspace_active[usize::from(self.workspace)] = Some(index);
    }

    /// Steps within the active workspace only; used by `next_tab`/`previous_tab`.
    pub(super) fn step_tab(&mut self, forward: bool) -> usize {
        let positions: Vec<usize> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.workspace == self.workspace)
            .map(|(index, _)| index)
            .collect();
        if positions.is_empty() {
            return self.active;
        }
        let current = positions
            .iter()
            .position(|&index| index == self.active)
            .unwrap_or(0);
        let target = if forward {
            (current + 1) % positions.len()
        } else {
            (current + positions.len() - 1) % positions.len()
        };
        let index = positions[target];
        self.note_active(index);
        index
    }

    /// Workspace-aware implementation of [`TabsState::close`].
    pub(super) fn close_tab(&mut self, index: usize) -> CloseOutcome {
        if index >= self.tabs.len() {
            return CloseOutcome::Invalid;
        }
        let workspace_id = self.tabs[index].workspace;
        let workspace = usize::from(workspace_id);
        let peers: Vec<usize> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|&(peer, tab)| peer != index && tab.workspace == workspace_id)
            .map(|(peer, _)| peer)
            .collect();
        if peers.is_empty() {
            // The workspace's last tab becomes a blank tab instead of vanishing.
            let mut blank = Tab::blank();
            blank.workspace = workspace_id;
            self.tabs[index] = blank;
            self.workspace_active[workspace] = Some(index);
            return CloseOutcome::ResetLast;
        }
        let neighbour = peers
            .iter()
            .copied()
            .find(|&peer| peer > index)
            .map(|peer| peer - 1)
            .or_else(|| peers.iter().copied().rfind(|&peer| peer < index))
            .unwrap_or(index);
        self.tabs.remove(index);
        for slot in &mut self.workspace_active {
            *slot = slot.map(|old| if old > index { old - 1 } else { old });
        }
        self.workspace_active[workspace] = Some(neighbour);
        if index < self.active {
            self.active -= 1;
        } else if index == self.active {
            self.active = neighbour;
        }
        CloseOutcome::Closed
    }

    /// Activates `workspace`, creating a blank tab there when it is still empty.
    pub(super) fn activate_workspace(&mut self, workspace: usize) -> u8 {
        if workspace >= SLOTS {
            return self.workspace;
        }
        let id = u8::try_from(workspace).unwrap_or(MAX_WORKSPACES - 1);
        let remembered = self.workspace_active[workspace].filter(|&index| {
            index < self.tabs.len() && usize::from(self.tabs[index].workspace) == workspace
        });
        if remembered.is_none() {
            let first = self
                .tabs
                .iter()
                .position(|tab| usize::from(tab.workspace) == workspace);
            if let Some(index) = first {
                self.workspace_active[workspace] = Some(index);
            } else if !self.is_full() {
                // MAX_TABS is shared by all groups; at the cap we stay put rather than
                // dropping a tab to make room for a blank one.
                let index = self.tabs.len();
                let mut tab = Tab::blank();
                tab.workspace = id;
                self.tabs.push(tab);
                self.workspace_active[workspace] = Some(index);
            } else {
                return self.workspace;
            }
        }
        self.workspace = id;
        self.active = self.workspace_active[workspace].unwrap_or(self.active);
        self.workspace_active[workspace] = Some(self.active);
        self.workspace
    }

    /// Cycles 0 -> 1 -> 2 -> 0, preserving each group's tabs and active tab.
    pub fn cycle_workspace(&mut self) -> u8 {
        let next = (usize::from(self.workspace) + 1) % SLOTS;
        self.activate_workspace(next)
    }

    /// Switches to the first empty workspace, or cycles when all three are in use.
    pub fn new_workspace(&mut self) -> u8 {
        let current = usize::from(self.workspace);
        for offset in 1..SLOTS {
            let target = (current + offset) % SLOTS;
            let empty = !self
                .tabs
                .iter()
                .any(|tab| usize::from(tab.workspace) == target);
            if empty {
                return self.activate_workspace(target);
            }
        }
        self.cycle_workspace()
    }

    /// Closes the active workspace and switches to the next non-empty one.
    ///
    /// Returns `true` when tabs were removed; when it is the only used group the
    /// active tab is reset to blank instead (the window always keeps a tab).
    pub fn close_workspace(&mut self) -> bool {
        let current = usize::from(self.workspace);
        let used: Vec<bool> = (0..SLOTS)
            .map(|workspace| {
                self.tabs
                    .iter()
                    .any(|tab| usize::from(tab.workspace) == workspace)
            })
            .collect();
        let target = (1..SLOTS)
            .map(|offset| (current + offset) % SLOTS)
            .find(|&workspace| used[workspace]);
        let Some(target) = target else {
            let mut blank = Tab::blank();
            blank.workspace = self.workspace;
            self.tabs[self.active] = blank;
            self.workspace_active[current] = Some(self.active);
            return false;
        };

        let mut kept_indices = Vec::new();
        let mut kept = Vec::new();
        for (old, tab) in self.tabs.drain(..).enumerate() {
            if usize::from(tab.workspace) != current {
                kept_indices.push(old);
                kept.push(tab);
            }
        }
        let mut slots = [None; SLOTS];
        for (workspace, slot) in slots
            .iter_mut()
            .enumerate()
            .filter(|&(index, _)| index != current)
        {
            *slot = self.workspace_active[workspace]
                .and_then(|old| kept_indices.iter().position(|&kept| kept == old))
                .or_else(|| {
                    kept_indices
                        .iter()
                        .position(|&old| usize::from(kept[old].workspace) == workspace)
                });
        }
        self.tabs = kept;
        self.workspace_active = slots;
        self.workspace = u8::try_from(target).unwrap_or(0);
        self.active = slots[target].unwrap_or(0);
        true
    }

    /// Moves the active tab to `workspace` and activates it there.
    pub fn move_active_to_workspace(&mut self, workspace: u8) -> bool {
        let target = usize::from(workspace);
        if target >= SLOTS || workspace == self.workspace {
            return false;
        }
        let source = usize::from(self.workspace);
        let index = self.active;
        let peers: Vec<usize> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|&(peer, tab)| peer != index && tab.workspace == self.workspace)
            .map(|(peer, _)| peer)
            .collect();
        if peers.is_empty() && self.is_full() {
            return false;
        }
        let mut moved = self.tabs[index].clone();
        moved.workspace = workspace;
        if peers.is_empty() {
            let mut blank = Tab::blank();
            blank.workspace = self.workspace;
            self.tabs[index] = blank;
            self.workspace_active[source] = Some(index);
        } else {
            let neighbour = peers
                .iter()
                .copied()
                .find(|&peer| peer > index)
                .map(|peer| peer - 1)
                .or_else(|| peers.iter().copied().rfind(|&peer| peer < index))
                .unwrap_or(index);
            self.tabs.remove(index);
            for slot in &mut self.workspace_active {
                *slot = slot.map(|old| if old > index { old - 1 } else { old });
            }
            self.workspace_active[source] = Some(neighbour);
        }
        let moved_index = self.tabs.len();
        self.tabs.push(moved);
        self.workspace_active[target] = Some(moved_index);
        self.workspace = workspace;
        self.active = moved_index;
        true
    }
}
