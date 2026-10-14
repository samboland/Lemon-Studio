//! ADD menu entries (`menu_registry` area): new channels, the plugin picker and the plugin list. ADD's settings entry
//! (Manage plugins) is OPTIONS', its Pattern entry PATTERNS' and its last tweaked parameter entry the parameters'.
// spec: ui/menus.md (ADD), ui/plugin-menu.md §1

use winit::keyboard::Key;

use crate::app::App;
use crate::menu_registry::Mods;
use crate::views::menus::ADD_MENU;

pub fn menu_handles(menu: usize, label: &str) -> bool {
    menu == ADD_MENU && matches!(label, "Sampler" | "More plugins..." | "Refresh plugin list (fast scan)")
}

pub fn menu_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    match label {
        // ADD > Sampler: a new Lemon Sampler channel at the bottom of the rack (spec: ui/plugin-menu.md §1).
        "Sampler" => {
            if let Some(index) = app.plugins.index_of(crate::plugins::LEMON_SAMPLER) {
                app.state.message = crate::plugin_menu::add_generator(app, index, None);
            }
        }
        "More plugins..." => {
            let at = app.mouse;
            app.open_plugin_menu(crate::plugin_menu::Target::Gen(None), at);
        }
        "Refresh plugin list (fast scan)" => app.start_scan(crate::plugin_db::Mode::Changed),
        _ => {}
    }
    None
}

pub fn key(_app: &mut App, _key: &Key, _mods: Mods) -> bool {
    false
}
