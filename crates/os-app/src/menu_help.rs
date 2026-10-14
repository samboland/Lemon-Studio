//! HELP menu entries (`menu_registry` area).
// spec: ui/menus.md (HELP)

use winit::keyboard::Key;

use crate::app::App;
use crate::menu_registry::Mods;
use crate::views::menus::HELP_MENU;

pub fn menu_handles(menu: usize, label: &str) -> bool {
    menu == HELP_MENU && label == "About..."
}

pub fn menu_run(_app: &mut App, _menu: usize, label: &str) -> Option<String> {
    match label {
        "About..." => Some(concat!("Lemon Studio ", env!("CARGO_PKG_VERSION"), " (pre-alpha), free software under the GNU GPL v3 or later").into()),
        _ => None,
    }
}

pub fn key(_app: &mut App, _key: &Key, _mods: Mods) -> bool {
    false
}
