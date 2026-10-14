//! EDIT menu entries (`menu_registry` area).
// spec: ui/menus.md (EDIT)

use winit::keyboard::Key;

use crate::app::{Action, App};
use crate::menu_registry::Mods;
use crate::views::menus::EDIT_MENU;

pub fn menu_handles(menu: usize, label: &str) -> bool {
    menu == EDIT_MENU && label == "Undo"
}

pub fn menu_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    // EDIT > Undo = Ctrl+Z (piano-roll undo).
    if label == "Undo" {
        app.act(Action::PianoUndo);
    }
    None
}

pub fn key(_app: &mut App, _key: &Key, _mods: Mods) -> bool {
    false
}
