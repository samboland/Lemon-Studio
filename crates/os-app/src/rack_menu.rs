//! Channel rack entries of the main menus (`menu_registry` area): ADD's Misc channels (Audio clip, Automation clip,
//! Layer), made by the rack's channel code.
// spec: ui/menus.md (ADD), ui/channel-rack.md

use crate::app::App;
use crate::views::menus::ADD_MENU;

pub fn menu_handles(menu: usize, label: &str) -> bool {
    menu == ADD_MENU && matches!(label, "Audio clip" | "Automation clip" | "Layer")
}

pub fn menu_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    match label {
        "Audio clip" => crate::rack::add_audio_clip(app),
        "Automation clip" => crate::rack::add_automation_clip(app),
        "Layer" => crate::rack::add_layer(app),
        _ => None,
    }
}
