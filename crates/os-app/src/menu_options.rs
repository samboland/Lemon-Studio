//! OPTIONS menu entries (`menu_registry` area): the settings tabs (also ADD > Manage plugins) and the switches.
// spec: ui/menus.md (OPTIONS), ui/settings.md §1

use winit::keyboard::Key;

use crate::app::App;
use crate::menu_registry::Mods;
use crate::views::menus::{ADD_MENU, AUTOSCROLL_LABEL, GROUPS_LABEL, METRONOME_LABEL, OPTIONS_MENU, TYPING_LABEL};

/// The switches toggle wherever their entry is.
fn switch(label: &str) -> bool {
    matches!(label, TYPING_LABEL | METRONOME_LABEL | AUTOSCROLL_LABEL | GROUPS_LABEL) || midi_switch(label).is_some()
}

/// OPTIONS > MIDI's switches (spec: ui/menus.md OPTIONS, ui/settings.md "MIDI"): kept in `midi.txt` with the MIDI
/// settings. Remote control is kept for controller linking; output and master sync rule the devices Settings > MIDI
/// switches on (`midi_out`).
pub const MIDI_REMOTE_LABEL: &str = "Enable MIDI remote control";
pub const MIDI_OUTPUT_LABEL: &str = "Enable MIDI output";
pub const MIDI_SYNC_LABEL: &str = "Enable MIDI master sync";

/// A MIDI switch's field in the settings, by its entry's label.
fn midi_switch(label: &str) -> Option<fn(&mut crate::midi_in::Cfg) -> &mut bool> {
    Some(match label {
        MIDI_REMOTE_LABEL => |c| &mut c.remote,
        MIDI_OUTPUT_LABEL => |c| &mut c.output,
        MIDI_SYNC_LABEL => |c| &mut c.sync,
        _ => return None,
    })
}

/// Whether a MIDI switch is on, by its entry's label (its tick); `None` for other entries.
pub fn midi_tick(label: &str) -> Option<bool> {
    let f = midi_switch(label)?;
    Some(*f(&mut crate::midi_in::cfg()))
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
        l if midi_switch(l).is_some() => {
            let f = midi_switch(l)?;
            crate::midi_in::change(|c| *f(c) = !*f(c));
            return Some(format!("{l}: {}", if midi_tick(l) == Some(true) { "on" } else { "off" }));
        }
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
