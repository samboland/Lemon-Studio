//! VIEW menu entries (`menu_registry` area): window toggles, the plugin and project pickers, the toolbar's panels,
//! arranging, aligning and closing windows.
// spec: ui/menus.md (VIEW), ui/close-windows.md, ui/main-window.md §1-3

use os_ui::Rect;
use winit::keyboard::{Key, NamedKey};

use crate::app::{Action, App, CloseAll, Win};
use crate::menu_registry::Mods;
use crate::playlist::{Cmd, Mark, PItem, Popup};
use crate::views::menus::{ADD_MENU, TOOLS_MENU, VIEW};
use crate::views::toolbar::Panel;

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

/// VIEW's own entries (beside the window toggles and the close commands).
const OWN: &[&str] = &["Project picker", "Toolbars", "Arrange windows", "Align all channel editors"];

pub fn menu_handles(menu: usize, label: &str) -> bool {
    (menu == VIEW && (win_of(label).is_some() || CloseAll::from_label(label).is_some() || OWN.contains(&label)))
        || ((menu == VIEW || menu == ADD_MENU) && label == "Plugin picker")
}

pub fn menu_run(app: &mut App, _menu: usize, label: &str) -> Option<String> {
    let at = app.mouse;
    match label {
        "Plugin picker" => app.open_plugin_menu(crate::plugin_menu::Target::All, at),
        "Project picker" => app.project_picker(at),
        // The main menu has no submenus of its own: these open at the mouse (as TOOLS > Last tweaked does).
        "Toolbars" => toolbars_menu(app, at),
        "Arrange windows" => arrange_menu(app, at),
        "Align all channel editors" => return align(app),
        _ => {
            if let Some(what) = CloseAll::from_label(label) {
                app.close_windows(what);
            } else if let Some(w) = win_of(label) {
                app.act(Action::ToggleWin(w));
            }
        }
    }
    None
}

/// VIEW shortcuts: Ctrl+F8 the project picker, Shift+F12 align the windows; Ctrl+H TOOLS > Stop sound (the entry's
/// shortcut, which had no key yet).
pub fn key(app: &mut App, key: &Key, mods: Mods) -> bool {
    match key {
        Key::Named(NamedKey::F8) if mods.ctrl && !mods.alt && !mods.shift => {
            let at = app.mouse;
            app.project_picker(at);
        }
        Key::Named(NamedKey::F12) if mods.shift && !mods.ctrl && !mods.alt => {
            if let Some(m) = align(app) {
                app.state.message = Some(m);
            }
        }
        Key::Character(c) if mods.ctrl && !mods.alt && !mods.shift && c.eq_ignore_ascii_case("h") => {
            crate::menu_registry::run(app, TOOLS_MENU, "Stop sound");
        }
        _ => return false,
    }
    true
}

// ---------------------------------------------------------------- submenu commands

/// What VIEW's submenus and the project picker do (`Cmd::View`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VCmd {
    /// Toolbars: show / hide a panel; show every panel.
    Panel(Panel),
    AllPanels,
    /// Arrange windows: a layout.
    Arrange(Layout),
    /// Project picker: open recent project n; FILE > Open...; forget the list.
    OpenRecent(usize),
    OpenDialog,
    ClearRecent,
}

pub fn run(app: &mut App, c: VCmd) -> Option<String> {
    app.invalidate();
    match c {
        VCmd::Panel(p) => {
            let on = crate::views::toolbar::toggle_panel(p);
            Some(format!("{}: {}", p.label(), if on { "shown" } else { "hidden" }))
        }
        VCmd::AllPanels => {
            crate::views::toolbar::show_all_panels();
            Some("Every toolbar panel shown".into())
        }
        VCmd::Arrange(l) => {
            arrange(app, l);
            Some(format!("Windows arranged: {}", l.label()))
        }
        VCmd::OpenRecent(i) => app.open_recent(i),
        VCmd::OpenDialog => {
            crate::menu_registry::run(app, crate::views::menus::FILE_MENU, "Open...");
            None
        }
        VCmd::ClearRecent => {
            crate::project_picker::clear();
            Some("Recent projects cleared".into())
        }
    }
}

/// VIEW > Toolbars: a tick per panel (spec: ui/main-window.md §1-3).
fn toolbars_menu(app: &mut App, at: (f32, f32)) {
    let mut items = vec![PItem::head("Toolbar panels")];
    items.extend(Panel::ALL.iter().map(|&p| PItem::new(p.label(), Cmd::View(VCmd::Panel(p))).mark(Mark::Check(crate::views::toolbar::shown(p)))));
    items.extend([PItem::sep(), PItem::new("Show all", Cmd::View(VCmd::AllPanels))]);
    app.state.playlist.popup = Some(Popup { x: at.0, y: at.1, items, sub: None });
}

// ---------------------------------------------------------------- Arrange windows, Align all channel editors

/// VIEW > Arrange windows: where the Playlist, Piano roll, Mixer and Channel rack go. Lemon Studio choice: FL keeps
/// layouts the user saved; Lemon has these presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// As Lemon Studio starts: the editors maximized, the rack and the channel window where they open.
    Default,
    /// The playlist over the mixer.
    PlaylistMixer,
    /// The playlist over the piano roll, the channel rack beside the piano roll.
    PlaylistPiano,
    /// The playlist and the piano roll side by side.
    SideBySide,
}

impl Layout {
    pub const ALL: [Layout; 4] = [Layout::Default, Layout::PlaylistMixer, Layout::PlaylistPiano, Layout::SideBySide];

    pub fn label(self) -> &'static str {
        match self {
            Layout::Default => "Default",
            Layout::PlaylistMixer => "Playlist over mixer",
            Layout::PlaylistPiano => "Playlist over piano roll",
            Layout::SideBySide => "Playlist and piano roll side by side",
        }
    }
}

fn arrange_menu(app: &mut App, at: (f32, f32)) {
    let mut items = vec![PItem::head("Arrange windows")];
    items.extend(Layout::ALL.iter().map(|&l| PItem::new(l.label(), Cmd::View(VCmd::Arrange(l)))));
    app.state.playlist.popup = Some(Popup { x: at.0, y: at.1, items, sub: None });
}

/// Floating window `w` (Playlist, Mixer, Piano roll) at `r`, shown and in front.
fn float(app: &mut App, w: Win, r: Rect) {
    let Some(i) = w.slot() else { return };
    let ws = &mut app.state.wins[i];
    ws.rect = r;
    ws.maxed = false;
    ws.mini = false;
    app.state.raise(w);
}

/// Splits `r` at `k` of its height (top, bottom) or width (left, right).
fn split(r: Rect, k: f32, across: bool) -> (Rect, Rect) {
    if across {
        let w = (r.w * k).round();
        (Rect::new(r.x, r.y, w, r.h), Rect::new(r.x + w, r.y, r.w - w, r.h))
    } else {
        let h = (r.h * k).round();
        (Rect::new(r.x, r.y, r.w, h), Rect::new(r.x, r.y + h, r.w, r.h - h))
    }
}

pub fn arrange(app: &mut App, l: Layout) {
    let work = app.state.work_rect();
    match l {
        Layout::Default => {
            // Where `App::new` puts them; the editors keep their floating places for when they are restored.
            let s = &mut app.state;
            for ws in &mut s.wins {
                ws.maxed = true;
                ws.mini = false;
            }
            s.rack_pos = (317.0, 195.0);
            s.rack_size = crate::views::rack::DEFAULT_SIZE;
            s.plugin_pos = (560.0, 260.0);
            s.raise(Win::Playlist);
        }
        Layout::PlaylistMixer => {
            let (top, bottom) = split(work, 0.55, false);
            app.state.hide(Win::PianoRoll);
            float(app, Win::Playlist, top);
            float(app, Win::Mixer, bottom);
        }
        Layout::PlaylistPiano => {
            let (top, bottom) = split(work, 0.45, false);
            let rack_w = app.state.rack_size.0.min(bottom.w * 0.45);
            let (rack, piano) = split(bottom, rack_w / bottom.w.max(1.0), true);
            app.state.hide(Win::Mixer);
            float(app, Win::Playlist, top);
            float(app, Win::PianoRoll, piano);
            app.state.rack_pos = (rack.x, rack.y);
            app.state.raise(Win::ChannelRack);
        }
        Layout::SideBySide => {
            let (left, right) = split(work, 0.5, true);
            app.state.hide(Win::Mixer);
            float(app, Win::Playlist, left);
            float(app, Win::PianoRoll, right);
        }
    }
    app.invalidate();
}

/// Cascade step between aligned windows.
const CASCADE: f32 = 26.0;

/// VIEW > Align all channel editors (Shift+F12): the open floating windows line up as a cascade from the workspace's
/// top-left, in their stacking order, each one fully on screen. Lemon Studio choice: Lemon draws one channel editor (the
/// channel settings window), so every floating window takes part: the channel rack, the channel settings, the editors
/// that are not maximized, the monitor and the tapper. Native plugin windows keep their places. Returns a message.
pub(crate) fn align(app: &mut App) -> Option<String> {
    let work = app.state.work_rect();
    let wins: Vec<Win> = app.state.z.iter().copied().filter(|&w| floating(app, w)).collect();
    if wins.is_empty() {
        return Some("No floating windows to align".into());
    }
    for (k, &w) in wins.iter().enumerate() {
        let (ww, wh) = size(app, w);
        let x = (work.x + 8.0 + CASCADE * k as f32).min(work.right() - ww).max(work.x);
        let y = (work.y + 8.0 + CASCADE * k as f32).min(work.bottom() - wh).max(work.y);
        let s = &mut app.state;
        match w {
            Win::ChannelRack => s.rack_pos = (x, y),
            Win::Plugin => s.plugin_pos = (x, y),
            Win::Perf => s.perf.pos = (x, y),
            Win::Tapper => {
                s.tapper.pos = (x, y);
                s.tapper.placed = true;
            }
            w => {
                if let Some(i) = w.slot() {
                    s.wins[i].rect.x = x;
                    s.wins[i].rect.y = y;
                }
            }
        }
    }
    app.invalidate();
    Some(format!("{} window{} aligned", wins.len(), if wins.len() == 1 { "" } else { "s" }))
}

/// Whether `w` floats (drawn by Lemon Studio, not maximized).
fn floating(app: &App, w: Win) -> bool {
    match w {
        Win::ChannelRack | Win::Plugin | Win::Perf | Win::Tapper => true,
        Win::Playlist | Win::Mixer | Win::PianoRoll => !app.state.is_max(w),
        Win::Browser | Win::Hosted(_) => false,
    }
}

fn size(app: &App, w: Win) -> (f32, f32) {
    let s = &app.state;
    let r = match w {
        Win::ChannelRack => crate::views::rack::rect(s),
        Win::Plugin => crate::views::channel_win::rect(s),
        Win::Perf => crate::views::perf::rect(s),
        Win::Tapper => crate::views::tapper::rect(s),
        w => s.win_rect(w),
    };
    (r.w, r.h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::menus::item_at;

    fn index(label: &str) -> usize {
        (0..60).find(|&i| item_at(VIEW, i).is_some_and(|it| it.label == label)).unwrap()
    }

    fn works(label: &str) -> bool {
        crate::views::menus::works(VIEW, item_at(VIEW, index(label)).unwrap())
    }

    /// The popup's item running `c`.
    fn pick(app: &mut App, c: VCmd) {
        let i = app.state.playlist.popup.as_ref().expect("a submenu").items.iter().position(|it| it.cmd == Cmd::View(c)).expect("the entry");
        app.act(Action::PlPopup(i));
    }

    /// VIEW > Toolbars: a tick per panel; unticking hides it (its controls go), Show all brings everything back.
    #[test]
    fn toolbars_show_and_hide_panels() {
        let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) else { return };
        let mut app = App::new(None);
        assert!(works("Toolbars"));
        app.menu_item(VIEW, index("Toolbars"));
        let pop = app.state.playlist.popup.as_ref().unwrap();
        assert_eq!(pop.items.iter().filter(|it| it.mark == Mark::Check(true)).count(), Panel::ALL.len());
        pick(&mut app, VCmd::Panel(Panel::Transport));
        assert!(!crate::views::toolbar::shown(Panel::Transport));
        app.paint(&mut c, None);
        assert!(!app.hits.iter().any(|h| h.action == Action::Play || h.action == Action::Tempo), "transport and tempo hidden");
        assert!(app.hits.iter().any(|h| h.action == Action::CpuPanel), "other panels stay");
        assert!(app.hits.iter().any(|h| h.action == Action::Menu(0)), "the menu bar stays");
        app.menu_item(VIEW, index("Toolbars"));
        let pop = app.state.playlist.popup.as_ref().unwrap();
        let tr = pop.items.iter().find(|it| it.cmd == Cmd::View(VCmd::Panel(Panel::Transport))).unwrap();
        assert_eq!(tr.mark, Mark::Check(false));
        pick(&mut app, VCmd::Panel(Panel::News));
        app.menu_item(VIEW, index("Toolbars"));
        pick(&mut app, VCmd::AllPanels);
        assert!(Panel::ALL.iter().all(|&p| crate::views::toolbar::shown(p)));
        app.paint(&mut c, None);
        assert!(app.hits.iter().any(|h| h.action == Action::Tempo));
    }

    #[test]
    fn toolbars_file_round_trips() {
        use crate::views::toolbar::{hidden_text, parse_hidden};
        assert_eq!(parse_hidden(""), 0);
        let h = parse_hidden("hidden=news, cpu,bogus\nother=x\n");
        assert_eq!(hidden_text(h), "hidden=cpu,news\n");
        assert_eq!(parse_hidden(&hidden_text(h)), h);
    }

    /// VIEW > Arrange windows: each layout puts the editors side by side or stacked inside the workspace.
    #[test]
    fn arrange_windows_layouts() {
        let mut app = App::new(None);
        assert!(works("Arrange windows"));
        app.menu_item(VIEW, index("Arrange windows"));
        pick(&mut app, VCmd::Arrange(Layout::PlaylistMixer));
        let s = &app.state;
        let (pl, mx) = (s.win_rect(Win::Playlist), s.win_rect(Win::Mixer));
        assert!(s.shown(Win::Playlist) && s.shown(Win::Mixer) && !s.is_max(Win::Playlist) && !s.is_max(Win::Mixer));
        assert!(pl.bottom() <= mx.y + 0.5, "{pl:?} over {mx:?}");
        let work = s.work_rect();
        assert!(mx.bottom() <= work.bottom() + 0.5 && pl.y >= work.y);
        arrange(&mut app, Layout::SideBySide);
        let s = &app.state;
        let (pl, pr) = (s.win_rect(Win::Playlist), s.win_rect(Win::PianoRoll));
        assert!(pl.right() <= pr.x + 0.5 && !s.shown(Win::Mixer));
        arrange(&mut app, Layout::PlaylistPiano);
        let s = &app.state;
        assert!(s.shown(Win::ChannelRack) && s.win_rect(Win::Playlist).bottom() <= s.win_rect(Win::PianoRoll).y + 0.5);
        arrange(&mut app, Layout::Default);
        assert!(app.state.is_max(Win::Playlist) && app.state.is_max(Win::Mixer));
        assert_eq!(app.state.message.as_deref(), Some("Windows arranged: Playlist over mixer"));
    }

    /// VIEW > Align all channel editors and Shift+F12 cascade the floating windows from the workspace's top-left.
    #[test]
    fn align_cascades_floating_windows() {
        use winit::keyboard::NamedKey;
        let mut app = App::new(None);
        assert!(works("Align all channel editors"));
        app.state.z = vec![Win::Mixer, Win::ChannelRack, Win::Plugin, Win::Tapper];
        app.state.rack_pos = (900.0, 700.0);
        app.state.plugin_pos = (1200.0, 300.0);
        app.shift = true;
        app.key(&Key::Named(NamedKey::F12), false, false);
        app.shift = false;
        let work = app.state.work_rect();
        let (x0, y0) = (work.x + 8.0, work.y + 8.0);
        assert_eq!(app.state.rack_pos, (x0, y0));
        assert_eq!(app.state.plugin_pos, (x0 + CASCADE, y0 + CASCADE));
        assert_eq!(app.state.tapper.pos, (x0 + 2.0 * CASCADE, y0 + 2.0 * CASCADE));
        assert_eq!(app.state.z, vec![Win::Mixer, Win::ChannelRack, Win::Plugin, Win::Tapper], "nothing closes");
        assert_eq!(app.state.message.as_deref(), Some("3 windows aligned"), "the maximized mixer stays");
        app.state.z.clear();
        app.menu_item(VIEW, index("Align all channel editors"));
        assert_eq!(app.state.message.as_deref(), Some("No floating windows to align"));
    }

    /// Ctrl+H runs TOOLS > Stop sound.
    #[test]
    fn ctrl_h_stops_sound() {
        let mut app = App::new(None);
        app.state.message = None;
        app.key(&Key::Character("h".into()), true, false);
        assert_eq!(app.state.message.as_deref(), Some("Sound stopped"));
        assert!(!key(&mut app, &Key::Character("h".into()), Mods::default()));
        // Alt+F7 (Touch controller) stays unbound.
        assert!(!key(&mut app, &Key::Named(NamedKey::F7), Mods { alt: true, ..Mods::default() }));
    }

    /// Touch controller, Script output and Background have no Lemon Studio meaning yet: still greyed out.
    #[test]
    fn unbuilt_entries_stay_greyed() {
        for label in ["Touch controller", "Script output", "Background"] {
            assert!(!works(label), "{label}");
        }
    }
}
