//! The plugin window's ▸ menu (the title bar's menu arrow) and what its entries do. The menu is a popup of the app
//! (`playlist::Popup`); its commands are `Cmd::Window(WCmd)`, run here.
// spec: ui/plugin-window.md "Docking and the window menu"

use crate::app::App;
use crate::playlist::{Cmd, Mark, PItem, Popup};

/// Commands of the plugin window menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WCmd {
    /// Detached: rack row, new state.
    Detach(usize, bool),
}

/// The plugin window's ▸ menu (spec: ui/plugin-window.md "Docking and the window menu"): FL's entries, only Detached
/// works yet.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn window_menu(row: usize, detached: bool, x: f32, y: f32) -> Popup {
    let items = vec![
        PItem::not_yet("Piano roll"),
        PItem::not_yet("Replace"),
        PItem::not_yet("Delete..."),
        PItem::head("Presets"),
        PItem::not_yet("Presets"),
        PItem::not_yet("Browse presets"),
        PItem::not_yet("Save preset as..."),
        PItem::not_yet("Add to plugin database (flag as favorite)"),
        PItem::head("Spare state"),
        PItem::not_yet("Store in spare state"),
        PItem::not_yet("Flip with spare state"),
        PItem::head("Parameters"),
        PItem::not_yet("Link all parameters..."),
        PItem::new("Browse parameters", Cmd::Param(crate::params::PCmd::Browse(row))),
        PItem::head("CPU"),
        PItem::not_yet("Allow threaded processing"),
        PItem::not_yet("Smart disable"),
        PItem::sep(),
        PItem::not_yet("Rename, color and icon...").key("F2"),
        PItem::not_yet("Change color..."),
        PItem::not_yet("Random color"),
        PItem::not_yet("Change icon..."),
        PItem::new("Detached", Cmd::Window(WCmd::Detach(row, !detached))).mark(Mark::Check(detached)),
        PItem::col(),
        PItem::head("Channel"),
        PItem::not_yet("Save channel state as..."),
        PItem::not_yet("Browse channel states"),
        PItem::sep(),
        PItem::not_yet("Route to free mixer track").key("Ctrl+L"),
    ];
    Popup { x, y, items, sub: None }
}

/// Run a plugin window menu command; returns a message, if any.
pub fn run(app: &mut App, c: WCmd) -> Option<String> {
    match c {
        WCmd::Detach(row, on) => {
            app.plugins.set_detached(row, on);
            app.state.raise(crate::app::Win::Hosted(row));
            app.invalidate();
            None
        }
    }
}
