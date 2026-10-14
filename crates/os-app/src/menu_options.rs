//! OPTIONS menu entries (`menu_registry` area): the settings tabs (also ADD > Manage plugins) and the switches.
// spec: ui/menus.md (OPTIONS), ui/settings.md §1

use winit::keyboard::Key;

use crate::app::App;
use crate::menu_registry::Mods;
use crate::views::menus::{ADD_MENU, AUTOSCROLL_LABEL, GROUPS_LABEL, METRONOME_LABEL, OPTIONS_MENU, TYPING_LABEL};

/// The switches toggle wherever their entry is.
fn switch(label: &str) -> bool {
    matches!(label, TYPING_LABEL | METRONOME_LABEL | AUTOSCROLL_LABEL | GROUPS_LABEL)
}

pub fn menu_handles(menu: usize, label: &str) -> bool {
    ((menu == OPTIONS_MENU || menu == ADD_MENU) && crate::views::settings::tab_for_menu(label).is_some()) || switch(label)
}

pub fn menu_run(app: &mut App, menu: usize, label: &str) -> Option<String> {
    match label {
        TYPING_LABEL => app.toggle_typing(),
        METRONOME_LABEL => app.toggle_metronome(),
        AUTOSCROLL_LABEL => app.toggle_autoscroll(),
        GROUPS_LABEL => app.toggle_groups(),
        _ => {
            if let Some(tab) = crate::views::settings::tab_for_menu(label).filter(|_| menu == OPTIONS_MENU || menu == ADD_MENU) {
                app.open_settings(tab);
            }
        }
    }
    None
}

pub fn key(_app: &mut App, _key: &Key, _mods: Mods) -> bool {
    false
}
