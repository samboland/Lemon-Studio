//! VIEW menu entries (`menu_registry` area): window toggles, the plugin picker, closing windows.
// spec: ui/menus.md (VIEW), ui/close-windows.md

use winit::keyboard::Key;

use crate::app::{Action, App, CloseAll, Win};
use crate::menu_registry::Mods;
use crate::views::menus::{ADD_MENU, VIEW};

/// VIEW's window entries toggle their window (the dropdown sends `Action::ToggleWin` for them directly).
pub fn win_of(label: &str) -> Option<Win> {
    Some(match label {
        "Playlist" => Win::Playlist,
        "Piano roll" => Win::PianoRoll,
        "Channel rack" => Win::ChannelRack,
        "Mixer" => Win::Mixer,
        "Browser" => Win::Browser,
        "Plugin performance monitor" => Win::Perf,
        // spec: ui/tempo-tapper.md §1
        "Tempo tapper" => Win::Tapper,
        _ => return None,
    })
}

pub fn menu_handles(menu: usize, label: &str) -> bool {
    (menu == VIEW && (win_of(label).is_some() || CloseAll::from_label(label).is_some()))
        || ((menu == VIEW || menu == ADD_MENU) && label == "Plugin picker")
}

pub fn menu_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    if label == "Plugin picker" {
        let at = app.mouse;
        app.open_plugin_menu(crate::plugin_menu::Target::All, at);
    } else if let Some(what) = CloseAll::from_label(label) {
        app.close_windows(what);
    } else if let Some(w) = win_of(label) {
        app.act(Action::ToggleWin(w));
    }
    None
}

pub fn key(_app: &mut App, _key: &Key, _mods: Mods) -> bool {
    false
}
