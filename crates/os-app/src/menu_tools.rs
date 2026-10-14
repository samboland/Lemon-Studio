//! TOOLS menu entries (`menu_registry` area): the macros. TOOLS' last tweaked parameter entries are the parameters'.
// spec: ui/menus.md (TOOLS), ui/unused-channels.md

use winit::keyboard::Key;

use crate::app::App;
use crate::menu_registry::Mods;
use crate::views::menus::TOOLS_MENU;

pub fn menu_handles(menu: usize, label: &str) -> bool {
    menu == TOOLS_MENU && matches!(label, "Stop sound" | "Select unused channels" | "Purge unused audio clips" | "Reset empty playlist tracks")
}

pub fn menu_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    match label {
        "Stop sound" => {
            app.transport.stop();
            Some("Sound stopped".into())
        }
        // spec: ui/unused-channels.md
        "Select unused channels" => {
            app.state.message = crate::rack::run(app, crate::rack::RCmd::SelectUnused);
            None
        }
        "Purge unused audio clips" => {
            app.state.message = crate::rack::run(app, crate::rack::RCmd::Purge);
            None
        }
        "Reset empty playlist tracks" => crate::playlist::run(app, crate::playlist::Cmd::ResetEmptyTracks),
        _ => None,
    }
}

pub fn key(_app: &mut App, _key: &Key, _mods: Mods) -> bool {
    false
}
