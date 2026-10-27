//! Context-menu state (M11 slice 1).
//!
//! A small, UI-framework-free model: the app builds the items (localized
//! labels, shortcuts, enabled flags), renders them, and forwards arrow/Enter
//! events here. `ContextAction` is the closed set of commands the editor
//! context menu can execute; separators are items without an action.
//!
//! The menu is deliberately not a native NSMenu: GPUI 0.2 has no reusable
//! context-menu API, and building it in-app keeps the same state machine
//! testable and usable by the explorer/tab menus later.

use crate::new_file;
use crate::screenplay;

/// One executable action in the editor context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    ProjectSearch,
    Propose,
    Convert(new_file::Kind),
    SetElement(screenplay::ElementType),
}

/// One rendered row: an action, or a divider when `action` is `None`.
#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub action: Option<ContextAction>,
    pub label: String,
    pub shortcut: &'static str,
    pub enabled: bool,
}

impl ContextMenuItem {
    pub fn action(
        action: ContextAction,
        label: impl Into<String>,
        shortcut: &'static str,
        enabled: bool,
    ) -> Self {
        Self {
            action: Some(action),
            label: label.into(),
            shortcut,
            enabled,
        }
    }

    pub fn separator() -> Self {
        Self {
            action: None,
            label: String::new(),
            shortcut: "",
            enabled: false,
        }
    }

    fn selectable(&self) -> bool {
        self.action.is_some() && self.enabled
    }
}

/// An open menu anchored at window coordinates.
#[derive(Debug, Clone)]
pub struct ContextMenuState {
    pub x: f32,
    pub y: f32,
    pub items: Vec<ContextMenuItem>,
    pub selected: usize,
}

impl ContextMenuState {
    pub fn new(x: f32, y: f32, items: Vec<ContextMenuItem>) -> Self {
        let selected = items
            .iter()
            .position(ContextMenuItem::selectable)
            .unwrap_or(0);
        Self {
            x,
            y,
            items,
            selected,
        }
    }

    /// Move the highlight to the next selectable row, wrapping. Separators and
    /// disabled actions are skipped; a menu with nothing selectable keeps the
    /// current index.
    pub fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let len = self.items.len() as isize;
        let mut index = self.selected as isize;
        for _ in 0..len {
            index = (index + delta).rem_euclid(len);
            if self.items[index as usize].selectable() {
                self.selected = index as usize;
                return;
            }
        }
    }

    /// The action under Enter, if any (disabled rows and separators do not
    /// activate).
    pub fn activate_selected(&self) -> Option<ContextAction> {
        self.items
            .get(self.selected)
            .filter(|item| item.selectable())
            .and_then(|item| item.action)
    }

    /// Test-only index activation; the UI reads `item.action` because it
    /// builds the row before dispatching.
    #[cfg(test)]
    pub fn activate(&self, index: usize) -> Option<ContextAction> {
        self.items
            .get(index)
            .filter(|item| item.selectable())
            .and_then(|item| item.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::action(ContextAction::Cut, "Cortar", "⌘X", true),
            ContextMenuItem::separator(),
            ContextMenuItem::action(ContextAction::Find, "Buscar", "⌘F", true),
            ContextMenuItem::action(
                ContextAction::Convert(new_file::Kind::Character),
                "Personaje",
                "",
                false,
            ),
            ContextMenuItem::action(ContextAction::SelectAll, "Todo", "⌘A", true),
        ]
    }

    #[test]
    fn selection_starts_on_the_first_enabled_action_and_skips_separators() {
        let mut menu = ContextMenuState::new(10.0, 20.0, items());
        assert_eq!(menu.selected, 0);
        assert_eq!(menu.activate_selected(), Some(ContextAction::Cut));
        menu.move_selection(1);
        assert_eq!(menu.selected, 2, "separator skipped");
        menu.move_selection(1);
        assert_eq!(menu.selected, 4, "disabled action skipped");
        menu.move_selection(1);
        assert_eq!(menu.selected, 0, "wraps");
        menu.move_selection(-1);
        assert_eq!(menu.selected, 4, "wraps backwards");
    }

    #[test]
    fn disabled_rows_and_separators_never_activate() {
        let menu = ContextMenuState::new(0.0, 0.0, items());
        assert_eq!(menu.activate(1), None);
        assert_eq!(menu.activate(3), None);
        assert_eq!(menu.activate(99), None);
        let mut all_disabled = ContextMenuState::new(
            0.0,
            0.0,
            vec![
                ContextMenuItem::separator(),
                ContextMenuItem::action(ContextAction::Paste, "Pegar", "⌘V", false),
            ],
        );
        assert_eq!(all_disabled.selected, 0);
        assert_eq!(all_disabled.activate_selected(), None);
        all_disabled.move_selection(1);
        assert_eq!(all_disabled.activate_selected(), None);
    }
}
