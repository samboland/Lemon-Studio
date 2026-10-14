//! Settings window (OPTIONS > MIDI / Audio / General / File / Theme settings, Project settings, Project info,
//! Debugging log; F10 / F11): ten tabs of grouped controls. Each tab is a file of this module (`midi.rs`, `audio.rs`,
//! ...) owning its rows (`page`), the labels of its controls that work today (`LIVE`) and what they do (`act`); this
//! file is the window, its tabs and the row drawing they share. Every control not in a tab's `LIVE` is drawn greyed
//! out and only says it is not available yet. The Plugins tab is the plugin manager (search folders, scanning, the
//! plugin table).
// spec: ui/settings.md, ui/plugin-manager.md

mod about;
mod audio;
mod debug;
mod file;
mod general;
mod info;
mod midi;
mod plugins;
mod project;
mod theme;

use std::collections::HashSet;

use os_ui::kit::lemon as lk;
use os_ui::kit::{self, Dir};
use os_ui::{Color, Font, Rect};

use crate::app::{Action, App, State, Ui};
use crate::plugin_db::{Col, Prefer};

/// What a click in the settings window does (`Action::Settings`).
#[derive(Debug, Clone, PartialEq)]
pub enum SAct {
    /// The window body (swallows clicks, wheel scrolls the page).
    Body,
    Close,
    Tab(usize),
    /// A working check (radio LED) by its label (`LIVE`).
    Check(&'static str),
    /// A collapsible group header by its title.
    Group(&'static str),
    /// A working button by its label (`LIVE`).
    Button(&'static str),
    /// Theme preset tile: 0 = default, n = `SettingsUi::themes[n - 1]`.
    Preset(usize),
    /// The plugin manager's controls (spec: ui/plugin-manager.md §2).
    Plug(Plug),
    /// A working combo (`LIVE_COMBOS`) by its label, with its box's bottom left (where its menu opens).
    Combo(&'static str, f32, f32),
}

/// Plugin manager controls. Rows count in `plugin_db::search_dirs` order (folders) or in `Db::rows` (table).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Plug {
    /// Search folder i: searched or not.
    Folder(usize),
    /// Remove added search folder i.
    Remove(usize),
    Prefer(Prefer),
    /// Column title: sort by it (again: reverse).
    Sort(Col),
    /// The table body (the wheel scrolls it).
    Table,
    /// Table row i (its hint: the path, why a file failed).
    Row(usize),
    Star(usize),
    Show(usize),
    /// The Find box: typing filters the table.
    Find,
}

/// Session state of the window.
#[derive(Debug, Default)]
pub struct SettingsUi {
    pub open: bool,
    pub tab: usize,
    /// Page scroll in pixels.
    pub scroll: f32,
    /// Groups whose open state differs from their default.
    pub folded: HashSet<&'static str>,
    /// Filled when the window opens: output devices, theme files (name, path), selected preset.
    pub devices: Vec<String>,
    pub device: String,
    /// Audio page: the audio settings, the rate streams open at, and the status lines (kept up to date while shown).
    pub audio: crate::audio_cfg::AudioCfg,
    pub rate: Option<u32>,
    pub status: [String; 2],
    /// Theme presets (name, `.flstheme` text): built-ins (spec ui/theme-presets.md), then the user's theme folder.
    pub themes: Vec<(String, String)>,
    pub preset: usize,
    /// The Plugins page shows the experimental native host toggles: `os_native::settings_shown()` at startup.
    pub native: bool,
}

/// Tabs: label, page title, group (0 system / 1 project / 2 info) (spec §2). Plugins takes the place of FL's Account
/// tab (spec: ui/plugin-manager.md §2); tabs are laid out from their text widths (`tab_rects`).
pub const TABS: [(&str, &str, u8); 10] = [
    ("MIDI", "MIDI input / output devices", 0),
    ("Audio", "Audio devices / mixer", 0),
    ("General", "System / miscellaneous", 0),
    ("File", "Files & folders", 0),
    ("Plugins", "Plugin manager", 0),
    ("Theme", "Look and feel", 0),
    ("Project", "Current project", 1),
    ("Info", "Project credits", 1),
    ("Debug", "Debugging log", 2),
    ("About", "About", 2),
];
pub const MIDI: usize = 0;
pub const AUDIO: usize = 1;
pub const GENERAL: usize = 2;
pub const FILE: usize = 3;
pub const PLUGINS: usize = 4;
pub const THEME: usize = 5;
pub const PROJECT: usize = 6;
pub const INFO: usize = 7;
pub const DEBUG: usize = 8;

use audio::LIVE_COMBOS;
pub use plugins::{COMBINE_CHECK, ERRORS_CHECK, STARTUP_CHECK};
#[cfg(test)]
pub use audio::{BUFFER_LENGTH, OUTPUT_DEVICE, SAMPLE_RATE};
#[cfg(test)]
pub use plugins::{NATIVE_FIRST_CHECK, NATIVE_HOST_CHECK};

/// Each tab's working checks and buttons (`LIVE` of its file). A label works on whichever page shows it.
const LIVE: [&[&str]; 9] = [midi::LIVE, audio::LIVE, general::LIVE, file::LIVE, plugins::LIVE, theme::LIVE, project::LIVE, info::LIVE, debug::LIVE];

fn live(key: &str) -> bool {
    LIVE.iter().any(|l| l.contains(&key))
}

/// Plugin table rows shown at once (spec: ui/plugin-manager.md §2).
pub const TABLE_ROWS: usize = 18;
const TABLE_ROW_H: f32 = 17.0;
/// Table columns: x from the table's left edge, width.
const COLS: [(f32, f32); 8] = [(0.0, 20.0), (20.0, 38.0), (58.0, 152.0), (210.0, 50.0), (260.0, 68.0), (328.0, 112.0), (440.0, 66.0), (506.0, 80.0)];

/// OPTIONS menu label -> tab (spec §1).
pub fn tab_for_menu(label: &str) -> Option<usize> {
    Some(match label {
        "MIDI settings" => MIDI,
        "Audio settings" => AUDIO,
        "General settings" => GENERAL,
        "File settings" => FILE,
        "Manage plugins" => PLUGINS,
        "Theme settings" => THEME,
        "Project general settings" => PROJECT,
        "Project info" => INFO,
        "Debugging log" => DEBUG,
        _ => return None,
    })
}

pub const W: f32 = 668.0;
const LABEL: Font = Font::new(13);
const TABS_B: f32 = 64.0;
const GX: f32 = 30.0;
const GW: f32 = 608.0;

/// The window rect: 668 x 900 at (626, 70) on a 1920x1080 screen, centred and shrunk to fit smaller screens.
pub fn rect(s: &State) -> Rect {
    let (sw, sh) = s.screen;
    let h = 900.0_f32.min(sh - 20.0).max(200.0);
    let y = 70.0_f32.min(sh - h - 10.0).max(0.0);
    Rect::new(((sw - W) / 2.0).max(0.0).round(), y, W, h)
}

/// One control row. Labels double as keys (unique per page).
pub enum Row {
    /// Check: label, default.
    Check(&'static str, bool),
    /// Two checks side by side (second at x 340).
    Check2(&'static str, bool, &'static str, bool),
    /// Disabled check (drawn dim).
    Off(&'static str, bool),
    /// Combo box: value, box width, label to its right.
    Combo(String, f32, &'static str),
    /// Red value box: value, label to its right.
    Value(String, &'static str),
    /// Label + red value box on the right side of the panel.
    ValueR(&'static str, String),
    /// Buttons in a row: (label, width).
    Buttons(Vec<(&'static str, f32)>),
    /// Slider: label, 0..1.
    Slider(&'static str, f32),
    /// List: column (title, x), rows of cells, visible rows.
    List(Vec<(&'static str, f32)>, Vec<Vec<String>>, usize),
    /// Plain text line (dim = small grey).
    Text(String, bool),
    /// Large text (About page).
    Big(String),
    /// Theme preset tiles (names; swatch colours).
    Presets(Vec<(String, [Color; 3])>, usize),
    /// Theme adjustment knobs and colour circles.
    ThemeKnobs,
    Gap(f32),
    /// Two controls on one line: left one, right one (right-hand column at x 304).
    Pair(Box<Row>, Box<Row>),
    /// A display that belongs to a feature not available yet (name for the hint): drawn greyed out.
    Dim(&'static str, Box<Row>),
    /// Plugin search folders: (path, tag, searched, added by the user).
    Folders(Vec<(String, &'static str, bool, bool)>),
    /// The preferred-format switch.
    Prefer(Prefer),
    /// The plugin table with its count and Find box (reads `State::plugdb`).
    Table,
}

pub struct Group {
    pub title: &'static str,
    pub note: &'static str,
    /// Has a fold chevron; `closed` = folded by default.
    pub fold: bool,
    pub closed: bool,
    pub rows: Vec<Row>,
}

fn pair(a: Row, b: Row) -> Row {
    Row::Pair(Box::new(a), Box::new(b))
}
fn g(title: &'static str, rows: Vec<Row>) -> Group {
    Group { title, note: "", fold: false, closed: false, rows }
}
fn gf(title: &'static str, closed: bool, rows: Vec<Row>) -> Group {
    Group { title, note: "", fold: true, closed, rows }
}
fn combo(v: &str, w: f32, label: &'static str) -> Row {
    Row::Combo(v.into(), w, label)
}
fn txt(v: impl Into<String>) -> Row {
    Row::Text(v.into(), false)
}
fn dim(name: &'static str, r: Row) -> Row {
    Row::Dim(name, Box::new(r))
}

/// The groups of page `tab` (spec §4), from the tab's own file. FL-only groups (cloud, remote app, licence) are left out.
pub fn page(tab: usize, s: &State) -> Vec<Group> {
    match tab {
        MIDI => midi::page(s),
        AUDIO => audio::page(s),
        GENERAL => general::page(s),
        FILE => file::page(s),
        PLUGINS => plugins::page(s),
        THEME => theme::page(s),
        PROJECT => project::page(s),
        INFO => info::page(s),
        DEBUG => debug::page(s),
        _ => about::page(s),
    }
}

/// What a click on a page's control does: the tab files' own handling, in tab order (`App::settings_act` handles the
/// window's body, close, tabs and group folds first). Returns false when no tab handles `a`.
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    midi::act(app, a)
        || audio::act(app, a)
        || general::act(app, a)
        || file::act(app, a)
        || plugins::act(app, a)
        || theme::act(app, a)
        || project::act(app, a)
        || info::act(app, a)
        || debug::act(app, a)
}

fn row_h(r: &Row) -> f32 {
    match r {
        Row::Check(..) | Row::Check2(..) | Row::Off(..) => 21.0,
        Row::Combo(..) | Row::Value(..) | Row::ValueR(..) => 27.0,
        Row::Buttons(..) => 36.0,
        Row::Slider(..) => 28.0,
        Row::List(c, _, n) => (if c.iter().any(|c| !c.0.is_empty()) { 18.0 } else { 0.0 }) + 17.0 * *n as f32 + 10.0,
        Row::Text(_, dim) => if *dim { 16.0 } else { 20.0 },
        Row::Big(_) => 28.0,
        Row::Presets(t, _) => ((t.len() + 2) / 3) as f32 * 64.0 + 6.0,
        Row::ThemeKnobs => 140.0,
        Row::Gap(h) => *h,
        Row::Pair(a, b) => row_h(a).max(row_h(b)),
        Row::Dim(_, r) => row_h(r),
        Row::Folders(f) => 21.0 * f.len().max(1) as f32 + 10.0,
        Row::Prefer(_) => 27.0,
        Row::Table => 4.0 + 18.0 + TABLE_ROW_H * TABLE_ROWS as f32 + 4.0 + 34.0,
    }
}

fn group_h(gr: &Group, open: bool) -> f32 {
    let head = if gr.title.is_empty() { 8.0 } else { 30.0 };
    if !open {
        return head;
    }
    head + gr.rows.iter().map(row_h).sum::<f32>() + 8.0
}

fn group_open(st: &SettingsUi, gr: &Group) -> bool {
    !gr.fold || (!gr.closed ^ st.folded.contains(gr.title))
}

/// Total content height of the current page (for scrolling).
pub fn content_h(s: &State) -> f32 {
    page(s.settings.tab, s).iter().map(|gr| group_h(gr, group_open(&s.settings, gr)) + 6.0).sum::<f32>() + 16.0
}

/// Visible page height.
pub fn page_h(s: &State) -> f32 {
    rect(s).h - TABS_B - 4.0
}

pub fn paint(ui: &mut Ui, s: &State) {
    let st = &s.settings;
    let r = rect(s);
    let t = ui.t;
    ui.hit(r, Action::Settings(SAct::Body), "");
    // Shadow, frame, title bar.
    let lemon = super::lemon(ui);
    if lemon {
        // Lemon (spec: ui/lemon-design.md §10): rounded window, pill tabs, rounded groups and square checks.
        ui.c.fill_rrect(r.outset(4.0), lk::r_panel(t) + 4.0, Color::rgb(0).with_alpha(50));
        lk::outlined(ui.c, r, lk::r_panel(t), t.s(0x1c1e21), t.s(0x2b2d30));
        ui.c.fill_rrect(Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, TABS_B - 1.0), lk::r_panel(t) - 1.0, t.s(0x25272a));
        ui.c.fill_rect(Rect::new(r.x + 1.0, r.y + TABS_B - lk::r_panel(t), r.w - 2.0, lk::r_panel(t)), t.s(0x25272a));
    } else {
        ui.c.fill_rrect(r.outset(4.0), 6.0, Color::rgb(0).with_alpha(50));
        ui.c.fill_rect(r, t.s(0x1c1e21));
        ui.c.fill_rect(r.inset(1.0), t.s(0x2b2d30));
        ui.c.fill_rect(Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, TABS_B - 1.0), t.s(0x25272a));
    }
    ui.c.icon(&os_ui::tabler::SETTINGS, Rect::new(r.x + 7.0, r.y + 7.0, 16.0, 16.0), t.text_dim);
    let (_, title, _) = TABS[st.tab.min(TABS.len() - 1)];
    ui.c.text(Font::new(13), r.x + 28.0, r.y + 7.0, t.text, &format!("Settings - {title}"));
    let x = Rect::new(r.right() - 26.0, r.y + 5.0, 20.0, 20.0);
    let hov = ui.hit(x, Action::Settings(SAct::Close), "Close");
    ui.c.icon(&os_ui::tabler::X, x.inset(3.0), if hov { t.glyph } else { t.text });
    // Tabs (spec §2): active text coloured by group.
    let font = Font::new(13);
    let rects = tab_rects(ui, r);
    for (i, &(label, _, grp)) in TABS.iter().enumerate() {
        let tr = rects[i];
        let hov = ui.hit(tr, Action::Settings(SAct::Tab(i)), TABS[i].1);
        let on = i == st.tab;
        let body = if on { t.s(0x1d1e1f) } else { t.s(0x222425) };
        if lemon {
            let pill = Rect::new(tr.x, tr.y + 2.0, tr.w, tr.h - 6.0);
            if on {
                lk::lit(ui.c, pill, lk::r_btn(t), hov);
            } else {
                ui.c.fill_rrect(pill, lk::r_btn(t), if hov { t.button_hi } else { body });
            }
        } else {
            ui.c.fill_rect(tr, body);
            for k in 0..7 {
                // slanted right edge
                let k = k as f32;
                ui.c.fill_rect(Rect::new(tr.right() + k, tr.y + 2.0 + k * 4.0, 1.0, tr.h - 2.0 - k * 4.0), body);
            }
        }
        let col = if on && lemon {
            lk::ink_on_lemon()
        } else if on {
            match grp {
                0 => t.a(0x2da2ff),
                1 => t.h(0xfdc13d),
                _ => t.a(0xbeff3c),
            }
        } else if hov {
            t.glyph
        } else {
            t.text
        };
        let lh = ui.c.line_height(font);
        let ty = if lemon { tr.cy() - 2.0 } else { tr.cy() };
        ui.c.text(font, tr.x + 10.0, (ty - lh / 2.0).round(), col, label);
    }
    ui.c.fill_rect(Rect::new(r.x + 1.0, r.y + TABS_B, r.w - 2.0, 1.0), t.s(0x1c1e21));
    // Page.
    let area = Rect::new(r.x + 1.0, r.y + TABS_B + 1.0, r.w - 2.0, r.h - TABS_B - 3.0);
    let total = content_h(s);
    let scroll = st.scroll.min((total - area.h).max(0.0)).max(0.0);
    ui.c.push_clip(area);
    let mut y = area.y + 7.0 - scroll;
    for gr in page(st.tab, s) {
        let open = group_open(st, &gr);
        let h = group_h(&gr, open);
        if y + h > area.y && y < area.bottom() {
            paint_group(ui, s, &gr, Rect::new(r.x + GX, y, GW, h), open, area);
        }
        y += h + 6.0;
    }
    ui.c.pop_clip();
    if total > area.h {
        let track = Rect::new(area.right() - 10.0, area.y + 4.0, 7.0, area.h - 8.0);
        kit::scrollbar(ui.c, t, track, true, scroll, area.h, total, kit::State::NORMAL);
    }
}

/// Tab rects in window `r`: text width + 10 px each side, 4 px apart, 15 px more between groups.
fn tab_rects(ui: &mut Ui, r: Rect) -> Vec<Rect> {
    let font = Font::new(13);
    let mut x = r.x + 3.0;
    let mut v = Vec::new();
    for (i, &(label, _, grp)) in TABS.iter().enumerate() {
        if i > 0 && TABS[i - 1].2 != grp {
            x += 15.0;
        }
        let w = ui.c.text_width(font, label) + 20.0;
        v.push(Rect::new(x.round(), r.y + 34.0, w.round(), TABS_B - 34.0));
        x += w + 11.0;
    }
    v
}

fn paint_group(ui: &mut Ui, s: &State, gr: &Group, r: Rect, open: bool, clip: Rect) {
    let t = ui.t;
    let lemon = super::lemon(ui);
    if lemon {
        lk::outlined(ui.c, r, lk::r_panel(t) - 2.0, t.s(0x1c1e21), t.s(0x25272a));
    } else {
        ui.c.fill_rect(r, t.s(0x1c1e21));
        ui.c.fill_rect(r.inset(1.0), t.s(0x25272a));
    }
    let mut y = r.y + 8.0;
    if !gr.title.is_empty() {
        let f = Font::new(13).bold();
        let tw = ui.c.text_width(f, gr.title);
        let hdr = Rect::new(r.x + 1.0, r.y + 1.0, tw + 14.0, 24.0);
        if !lemon {
            ui.c.fill_rect(hdr, t.s(0x222427));
            for k in 0..6 {
                let k = k as f32;
                ui.c.fill_rect(Rect::new(hdr.right() + k, hdr.y, 1.0, hdr.h - k * 4.0), t.s(0x222427));
            }
        }
        ui.c.text(f, r.x + if lemon { 8.0 } else { 4.0 }, r.y + 4.0, if lemon { t.accent } else { t.text }, gr.title);
        if !gr.note.is_empty() {
            ui.c.text(Font::new(13), r.x + if lemon { 8.0 } else { 4.0 } + tw, r.y + 4.0, t.text_dim, gr.note);
        }
        if gr.fold {
            let bar = Rect::new(r.x, r.y, r.w, 26.0);
            if clip.contains(bar.cx(), bar.cy()) {
                ui.hit(bar, Action::Settings(SAct::Group(gr.title)), if open { "Fold" } else { "Unfold" });
            }
            kit::chevron(ui.c, r.right() - 10.0, r.y + 13.0, 4.0, if open { Dir::Up } else { Dir::Down }, t.text_dim);
        }
        y = r.y + 30.0;
    }
    if !open {
        return;
    }
    for row in &gr.rows {
        let h = row_h(row);
        if y + h > clip.y && y < clip.bottom() {
            paint_row(ui, s, gr.title, row, r, y, h);
        }
        y += h;
    }
}

/// A check showing `on`; one that is not `enabled` (not in `LIVE`) is greyed out.
fn check(ui: &mut Ui, key: &'static str, on: bool, x: f32, cy: f32, enabled: bool) {
    let t = ui.t;
    let tw = ui.c.text_width(Font::new(13), key);
    let hr = Rect::new(x - 8.0, cy - 9.0, tw + 26.0, 18.0);
    let hov = enabled && ui.hit(hr, Action::Settings(SAct::Check(key)), "");
    if !enabled {
        ui.off(hr, key);
    }
    led(ui, x, cy, on, hov);
    let col = if hov { t.glyph } else { t.text };
    let lh = ui.c.line_height(Font::new(13));
    ui.c.text(Font::new(13), x + 11.0, (cy - lh / 2.0).round(), col, key);
    if !enabled {
        grey(ui, hr);
    }
}

/// The check mark itself at (x, cy).
fn led(ui: &mut Ui, x: f32, cy: f32, on: bool, hov: bool) {
    let t = ui.t;
    // Round LED (spec §3): orange when on. Lemon: a rounded square check, lit with a tick when on.
    let lemon = super::lemon(ui);
    if !lemon {
        ui.c.fill_circle(x, cy, 6.5, t.s(0x141516));
    }
    if lemon {
        let b = Rect::new(x - 6.5, cy - 6.5, 13.0, 13.0);
        if on {
            lk::lit(ui.c, b, lk::r_small(t), hov);
            ui.c.polyline(&[(x - 3.5, cy), (x - 1.0, cy + 2.5), (x + 3.5, cy - 2.5)], 1.6, lk::ink_on_lemon());
        } else {
            lk::outlined(ui.c, b, lk::r_small(t), t.s(0x141516), if hov { t.s(0x4a4c4e) } else { t.s(0x36393a) });
        }
    } else if on {
        ui.c.fill_circle(x, cy, 5.5, t.h(0x952b15));
        ui.c.fill_circle(x, cy, 4.2, t.h(0xf09672));
        ui.c.fill_circle(x - 1.0, cy - 1.2, 1.6, t.h(0xffd2bc));
    } else {
        ui.c.fill_circle(x, cy, 5.5, if hov { t.s(0x6a6c6e) } else { t.s(0x58595a) });
        ui.c.fill_circle(x, cy, 4.2, t.s(0x36393a));
    }
}

/// Greys out what was just drawn in `r` on a group body.
fn grey(ui: &mut Ui, r: Rect) {
    let bg = ui.t.s(0x25272a);
    ui.veil(r, bg);
}

/// The hint name of a control: its label, else its group's title.
fn name_of(label: &'static str, group: &'static str) -> &'static str {
    if !label.is_empty() {
        label
    } else if !group.is_empty() {
        group
    } else {
        "This option"
    }
}

fn paint_row(ui: &mut Ui, s: &State, group: &'static str, row: &Row, g: Rect, y: f32, h: f32) {
    paint_row_at(ui, s, group, row, g, y, h, 0.0)
}

#[allow(clippy::too_many_arguments)]
fn paint_row_at(ui: &mut Ui, s: &State, group: &'static str, row: &Row, g: Rect, y: f32, h: f32, dx: f32) {
    let t = ui.t;
    let f = LABEL;
    let x0 = g.x + 12.0 + dx;
    let cy = y + h / 2.0;
    let lh = ui.c.line_height(f);
    let ty = (cy - lh / 2.0).round();
    match row {
        // A check that is not available yet shows its default, greyed out.
        Row::Check(k, d) => check(ui, k, *d, x0 + 10.0, cy, live(k)),
        Row::Off(k, d) => check(ui, k, *d, x0 + 10.0, cy, false),
        Row::Check2(a, da, b, db) => {
            check(ui, a, *da, g.x + 22.0, cy, live(a));
            check(ui, b, *db, g.x + 324.0, cy, live(b));
        }
        // The Audio page's device, rate and buffer combos open their menus.
        Row::Combo(v, w, label) if LIVE_COMBOS.contains(label) => {
            let b = Rect::new(x0, y + 3.0, *w, 21.0);
            let hov = ui.hit(b, Action::Settings(SAct::Combo(label, b.x, b.bottom())), "");
            kit::combo(ui.c, t, b, v, f, kit::State::hover(hov));
            ui.c.text(f, b.right() + 8.0, ty, if hov { t.glyph } else { t.text }, label);
        }
        // Other combos and value boxes do not work yet. A combo keeps showing its value (the project's PPQ), greyed
        // out.
        Row::Combo(v, w, label) => {
            let b = Rect::new(x0, y + 3.0, *w, 21.0);
            ui.off(b, name_of(label, group));
            kit::combo(ui.c, t, b, v, f, kit::State::NORMAL);
            let lw = ui.c.text(f, b.right() + 8.0, ty, t.text, label);
            grey(ui, Rect::new(b.x, b.y, b.w + 8.0 + lw, b.h));
        }
        Row::Value(v, label) => {
            let b = Rect::new(x0, y + 3.0, 38.0, 21.0);
            ui.off(b, name_of(label, group));
            value_red(ui, b, v);
            let lw = ui.c.text(f, b.right() + 8.0, ty, t.text, label);
            grey(ui, Rect::new(b.x, b.y, b.w + 8.0 + lw, b.h));
        }
        Row::ValueR(label, v) => {
            let b = Rect::new(g.right() - 52.0, y + 3.0, 42.0, 21.0);
            ui.off(b, name_of(label, group));
            value_red(ui, b, v);
            ui.c.text_right(f, b.x - 8.0, ty, t.text, label);
            let lw = ui.c.text_width(f, label);
            grey(ui, Rect::span(b.x - 8.0 - lw, b.right(), b.y, b.bottom()));
        }
        Row::Buttons(bs) => {
            let mut x = x0;
            for &(label, w) in bs {
                let b = Rect::new(x, y + 4.0, w, 28.0);
                let on = live(label);
                let hov = on && ui.hit(b, Action::Settings(SAct::Button(label)), "");
                if !on {
                    ui.off(b, label);
                }
                if super::lemon(ui) {
                    lk::outlined(ui.c, b, lk::r_btn(t), t.s(0x161819), lk::face(t.s(0x2a2d2f), kit::State::hover(hov)));
                } else {
                    kit::bordered(ui.c, b, 2.0, t.s(0x161819), kit::face(t.s(0x212425), kit::State::hover(hov)));
                }
                ui.c.text_center(Font::new(13), b, if hov { t.glyph } else { t.text }, label);
                if !on {
                    grey(ui, b);
                }
                x += w + 8.0;
            }
        }
        Row::Slider(label, v) => {
            let tr = Rect::new(x0, cy - 2.0, 150.0, 4.0);
            if super::lemon(ui) {
                ui.c.fill_rrect(tr, 2.0, t.s(0x161819));
                ui.c.fill_rrect(Rect::new(tr.x, tr.y, tr.w * v, tr.h), 2.0, t.accent);
            } else {
                ui.c.fill_rect(tr, t.s(0x161819));
                ui.c.fill_rect(Rect::new(tr.x, tr.y, tr.w * v, tr.h), t.h(0xc8312e));
            }
            let hx = tr.x + tr.w * v;
            ui.c.fill_circle(hx, cy, 8.0, t.s(0x111213));
            ui.c.fill_circle(hx, cy, 7.0, t.s(0x4a4d50));
            ui.c.fill_rect(Rect::new(hx - 0.5, cy - 6.0, 1.0, 5.0), t.s(0x1a1b1c));
            let lw = ui.c.text(f, tr.right() + 12.0, ty, t.text, label);
            let r = Rect::span(tr.x - 9.0, tr.right() + 14.0 + lw, cy - 9.0, cy + 9.0);
            ui.off(r, name_of(label, group));
            grey(ui, r);
        }
        Row::List(cols, rows, n) => {
            let b = Rect::new(x0 - 4.0, y + 2.0, g.w - 16.0, h - 6.0);
            if super::lemon(ui) {
                lk::outlined(ui.c, b, lk::r_btn(t), t.s(0x161819), t.s(0x222527));
            } else {
                ui.c.fill_rect(b, t.s(0x161819));
                ui.c.fill_rect(b.inset(1.0), t.s(0x222527));
            }
            let mut ry = b.y + 2.0;
            if cols.iter().any(|c| !c.0.is_empty()) {
                for (c, cx) in cols {
                    ui.c.text(Font::new(13), b.x + 6.0 + cx, ry, t.text_dim, c);
                }
                ry += 18.0;
            }
            for (i, cells) in rows.iter().take(*n).enumerate() {
                let rr = Rect::new(b.x + 2.0, ry + i as f32 * 17.0, b.w - 4.0, 17.0);
                if i % 2 == 1 {
                    ui.c.fill_rect(rr, t.s(0x26292b));
                }
                for (c, (_, cx)) in cells.iter().zip(cols.iter()) {
                    ui.c.text_clip(Font::new(13), rr.x + 4.0 + cx, rr.y + 1.0, rr.w - cx - 8.0, t.text, c);
                }
            }
        }
        Row::Text(v, dim) => {
            ui.c.text(if *dim { Font::new(13) } else { f }, x0, ty, if *dim { t.text_dim } else { t.text }, v);
        }
        Row::Big(v) => {
            let b = Rect::new(x0 - 4.0, y + 2.0, g.w - 16.0, 24.0);
            ui.c.fill_rect(b, t.s(0x1d1f21));
            let lh = ui.c.line_height(Font::new(14));
            ui.c.text_clip(Font::new(14), b.x + 6.0, (b.cy() - lh / 2.0).round(), b.w - 12.0, t.text, v);
        }
        Row::Presets(tiles, sel) => {
            for (i, (name, sw)) in tiles.iter().enumerate() {
                let (col, rowi) = ((i % 3) as f32, (i / 3) as f32);
                let b = Rect::new(x0 + col * 196.0, y + 4.0 + rowi * 64.0, 186.0, 56.0);
                let hov = ui.hit(b, Action::Settings(SAct::Preset(i)), "Load this theme");
                if super::lemon(ui) {
                    // Lemon: a rounded tile, framed in the main colour when chosen.
                    use os_theme::design::lemon as brand;
                    let rad = lk::r_btn(t);
                    ui.c.fill_rrect(b, rad + 2.0, if i == *sel { brand::MAIN } else if hov { brand::DARK_HIGHLIGHT } else { t.s(0x161819) });
                    let inner = b.inset(2.0);
                    let name_bg = t.s(0x1d1e1f);
                    ui.c.fill_rrect(inner, rad, sw[0]);
                    let nr = Rect::new(inner.x, inner.y + 30.0, inner.w, inner.h - 30.0);
                    ui.c.fill_rrect(nr, rad, name_bg);
                    ui.c.fill_rect(Rect::new(nr.x, nr.y, nr.w, rad), name_bg);
                    ui.c.fill_rrect(Rect::new(inner.x + 8.0, inner.y + 10.0, 40.0, 10.0), 3.0, sw[1]);
                    ui.c.fill_rrect(Rect::new(inner.x + 54.0, inner.y + 10.0, 24.0, 10.0), 3.0, sw[2]);
                    ui.c.text_center(Font::new(13).bold(), nr, t.text, name);
                    continue;
                }
                ui.c.fill_rect(b, if i == *sel { t.a(0x2d7cd0) } else if hov { t.s(0x5a5c5e) } else { t.s(0x161819) });
                let inner = b.inset(2.0);
                ui.c.fill_rect(Rect::new(inner.x, inner.y, inner.w, 30.0), sw[0]);
                ui.c.fill_rect(Rect::new(inner.x + 8.0, inner.y + 10.0, 40.0, 10.0), sw[1]);
                ui.c.fill_rect(Rect::new(inner.x + 54.0, inner.y + 10.0, 24.0, 10.0), sw[2]);
                ui.c.fill_rect(Rect::new(inner.x, inner.y + 30.0, inner.w, inner.h - 30.0), t.s(0x1d1e1f));
                let nr = Rect::new(inner.x, inner.y + 30.0, inner.w, inner.h - 30.0);
                ui.c.text_center(Font::new(13).bold(), nr, t.text, name);
            }
        }
        Row::ThemeKnobs => {
            for (i, k) in ["Hue", "Saturation", "Brightness", "Contrast", "Text"].iter().enumerate() {
                let cx = x0 + 28.0 + i as f32 * 61.0;
                kit::knob(ui.c, t, cx, y + 24.0, 14.0, 0.5, true, kit::KnobLook::Ring, kit::State::NORMAL);
                ui.c.text_center(Font::new(13), Rect::new(cx - 30.0, y + 42.0, 60.0, 18.0), t.text, k);
            }
            let cols = [t.a(0xa6d84a), t.h(0xfb9244), t.led(0x7cd317), t.h(0xe8502a), t.s(0x3a3c3e), t.h(0xf0a040), t.a(0x8fa8c8), t.tx(0xcfd1d2)];
            for (i, k) in ["Selection", "Highlight", "Mute", "Option", "Steps", "Meters", "Waves", "Text"].iter().enumerate() {
                let cx = x0 + 28.0 + i as f32 * 61.0;
                ui.c.fill_circle(cx, y + 92.0, 16.0, t.s(0x111213));
                ui.c.fill_circle(cx, y + 92.0, 14.5, cols[i]);
                ui.c.text_center(Font::new(13), Rect::new(cx - 30.0, y + 110.0, 60.0, 18.0), t.text, k);
            }
            let r = Rect::new(x0, y, g.w - 24.0, 130.0);
            ui.off(r, "Theme adjustments");
            grey(ui, r);
        }
        Row::Gap(_) => {}
        Row::Pair(a, b) => {
            paint_row_at(ui, s, group, a, g, y, h, 0.0);
            paint_row_at(ui, s, group, b, g, y, h, 304.0);
        }
        Row::Folders(list) => folders(ui, list, Rect::new(x0 - 4.0, y + 2.0, g.w - 16.0, h - 6.0)),
        Row::Prefer(cur) => prefer(ui, *cur, x0, y, ty),
        Row::Table => table(ui, s, Rect::new(x0 - 4.0, y + 2.0, g.w - 16.0, h - 4.0)),
        Row::Dim(name, r) => {
            paint_row_at(ui, s, group, r, g, y, h, dx);
            let b = Rect::span(x0 - 4.0, g.right() - 8.0, y, y + h);
            ui.off(b, name);
            grey(ui, b);
        }
    }
}

/// A list frame (lists, the folder list, the plugin table).
fn list_frame(ui: &mut Ui, b: Rect) {
    let t = ui.t;
    if super::lemon(ui) {
        lk::outlined(ui.c, b, lk::r_btn(t), t.s(0x161819), t.s(0x222527));
    } else {
        ui.c.fill_rect(b, t.s(0x161819));
        ui.c.fill_rect(b.inset(1.0), t.s(0x222527));
    }
}

/// Search folders (spec: ui/plugin-manager.md §2): a check, the path, the tag; added folders end with a remove button.
fn folders(ui: &mut Ui, list: &[(String, &'static str, bool, bool)], b: Rect) {
    let t = ui.t;
    list_frame(ui, b);
    let f = Font::new(13);
    let lh = ui.c.line_height(f);
    if list.is_empty() {
        ui.c.text(f, b.x + 8.0, (b.y + 2.0 + 10.5 - lh / 2.0).round(), t.text_dim, "(no folders)");
    }
    for (i, (path, tag, on, added)) in list.iter().enumerate() {
        let rr = Rect::new(b.x + 2.0, b.y + 2.0 + i as f32 * 21.0, b.w - 4.0, 21.0);
        if i % 2 == 1 {
            ui.c.fill_rect(rr, t.s(0x26292b));
        }
        let ty = (rr.cy() - lh / 2.0).round();
        let right = if *added { rr.right() - 26.0 } else { rr.right() - 8.0 };
        let tag_w = ui.c.text_width(f, tag);
        let hr = Rect::span(rr.x, right - tag_w - 8.0, rr.y, rr.bottom());
        let hov = ui.hit(hr, Action::Settings(SAct::Plug(Plug::Folder(i))), if *on { "Searched: click to skip this folder" } else { "Skipped: click to search this folder" });
        led(ui, rr.x + 12.0, rr.cy(), *on, hov);
        let col = if !*on { t.text_dim } else if hov { t.glyph } else { t.text };
        ui.c.text_clip(f, rr.x + 26.0, ty, hr.right() - rr.x - 30.0, col, path);
        ui.c.text_right(f, right, ty, t.text_dim, tag);
        if *added {
            let xr = Rect::new(rr.right() - 21.0, rr.y + 2.0, 17.0, 17.0);
            let hov = ui.hit(xr, Action::Settings(SAct::Plug(Plug::Remove(i))), "Remove this folder");
            ui.c.icon(&os_ui::tabler::X, xr.inset(2.0), if hov { t.glyph } else { t.text_dim });
        }
    }
}

/// The preferred format switch (spec: ui/plugin-manager.md §3): VST3, CLAP, VST2 (greyed: not hosted yet).
fn prefer(ui: &mut Ui, cur: Prefer, x0: f32, y: f32, ty: f32) {
    let t = ui.t;
    let f = LABEL;
    let lw = ui.c.text(f, x0, ty, t.text, "Preferred format");
    let lemon = super::lemon(ui);
    let mut x = x0 + lw + 12.0;
    for p in [Prefer::Vst3, Prefer::Clap, Prefer::Vst2] {
        let b = Rect::new(x, y + 3.0, 54.0, 21.0);
        let enabled = p != Prefer::Vst2;
        let on = p == cur;
        let hov = enabled && ui.hit(b, Action::Settings(SAct::Plug(Plug::Prefer(p))), "Listed when a plugin is found in several formats");
        if !enabled {
            ui.off(b, "VST2 hosting");
        }
        let ink = if lemon {
            if on {
                lk::lit(ui.c, b, lk::r_btn(t), hov);
                lk::ink_on_lemon()
            } else {
                lk::outlined(ui.c, b, lk::r_btn(t), t.s(0x161819), lk::face(t.s(0x2a2d2f), kit::State::hover(hov)));
                if hov { t.glyph } else { t.text }
            }
        } else if on {
            kit::bordered(ui.c, b, 2.0, t.s(0x161819), t.h(0xf09672));
            t.s(0x1c1e21)
        } else {
            kit::bordered(ui.c, b, 2.0, t.s(0x161819), kit::face(t.s(0x212425), kit::State::hover(hov)));
            if hov { t.glyph } else { t.text }
        };
        ui.c.text_center(Font::new(13), b, ink, p.label());
        if !enabled {
            grey(ui, b);
        }
        x += 58.0;
    }
}

/// A filled star (favourite).
const STAR_ON: os_ui::tabler::TablerIcon = os_ui::tabler::TablerIcon { filled: true, d: os_ui::tabler::STAR.d };

/// The plugin table (spec: ui/plugin-manager.md §2): sortable column titles, ★ / Show toggles, the count and the Find box.
fn table(ui: &mut Ui, s: &State, area: Rect) {
    let t = ui.t;
    let d = &s.plugdb;
    let f = Font::new(13);
    let lh = ui.c.line_height(f);
    let b = Rect::new(area.x, area.y, area.w, 4.0 + 18.0 + TABLE_ROW_H * TABLE_ROWS as f32 + 2.0);
    list_frame(ui, b);
    ui.hit(b, Action::Settings(SAct::Plug(Plug::Table)), "");
    let x0 = b.x + 2.0;
    // Column titles.
    let hy = b.y + 2.0;
    for (col, &(cx, cw)) in Col::ALL.iter().zip(&COLS) {
        let hr = Rect::new(x0 + cx, hy, cw, 18.0);
        let hov = ui.hit(hr, Action::Settings(SAct::Plug(Plug::Sort(*col))), "Sort by this column");
        let tw = ui.c.text(f, hr.x + 4.0, (hr.cy() - lh / 2.0).round(), if hov { t.glyph } else { t.text_dim }, col.title());
        if d.sort.0 == *col {
            kit::chevron(ui.c, hr.x + 4.0 + tw + 6.0, hr.cy(), 3.0, if d.sort.1 { Dir::Up } else { Dir::Down }, t.text_dim);
        }
    }
    ui.c.fill_rect(Rect::new(b.x + 1.0, hy + 18.0, b.w - 2.0, 1.0), t.s(0x161819));
    // Rows.
    let ry0 = hy + 19.0;
    let n = d.rows.len();
    let scroll = n > TABLE_ROWS;
    let rw = b.w - 4.0 - if scroll { 10.0 } else { 0.0 };
    if n == 0 {
        let msg = if d.find.is_empty() { "(no plugins)" } else { "(no plugin matches)" };
        ui.c.text(f, x0 + 8.0, ry0 + 1.0, t.text_dim, msg);
    }
    for (vi, row) in d.rows.iter().enumerate().skip(d.top).take(TABLE_ROWS) {
        let rr = Rect::new(x0, ry0 + (vi - d.top) as f32 * TABLE_ROW_H, rw, TABLE_ROW_H);
        if (vi - d.top) % 2 == 1 {
            ui.c.fill_rect(rr, t.s(0x26292b));
        }
        let hint = match (row.path.is_empty(), row.note.is_empty()) {
            (false, false) => format!("{}: {}", row.path, row.note),
            (false, true) => row.path.clone(),
            (true, false) => row.note.clone(),
            (true, true) => String::new(),
        };
        ui.hit(rr, Action::Settings(SAct::Plug(Plug::Row(vi))), hint);
        let ty = (rr.cy() - lh / 2.0).round();
        if row.toggles {
            let sr = Rect::new(x0 + COLS[0].0, rr.y, COLS[0].1, rr.h);
            let hov = ui.hit(sr, Action::Settings(SAct::Plug(Plug::Star(vi))), if row.favorite { "Favourite: listed first in the plugin menus" } else { "Star: list it first in the plugin menus" });
            let ir = Rect::new(sr.cx() - 6.5, sr.cy() - 6.5, 13.0, 13.0);
            if row.favorite {
                ui.c.icon(&STAR_ON, ir, if super::lemon(ui) { os_theme::design::lemon::MAIN } else { t.h(0xfdc13d) });
            } else {
                ui.c.icon(&os_ui::tabler::STAR, ir, if hov { t.glyph } else { t.s(0x58595a) });
            }
            let shr = Rect::new(x0 + COLS[1].0, rr.y, COLS[1].1, rr.h);
            let hov = ui.hit(shr, Action::Settings(SAct::Plug(Plug::Show(vi))), if row.shown { "Listed in the plugin menus: click to hide" } else { "Hidden from the plugin menus: click to list" });
            led(ui, shr.cx(), shr.cy(), row.shown, hov);
        }
        let col = if row.dim || !row.shown { t.text_dim } else { t.text };
        let failed = !row.toggles && !row.status.is_empty() && row.status != "ok";
        let cells: [(&str, Color); 6] = [
            (&row.name, col),
            (row.format, col),
            (row.kind, col),
            (&row.vendor, col),
            (&row.status, if failed { t.h(0xe8502a) } else { col }),
            (&row.file, col),
        ];
        for (k, (text, c)) in cells.iter().enumerate() {
            let (cx, cw) = COLS[k + 2];
            let w = (cw - 8.0).min(rr.right() - (x0 + cx) - 6.0);
            ui.c.text_clip(f, x0 + cx + 4.0, ty, w, *c, text);
        }
    }
    if scroll {
        let track = Rect::new(b.right() - 10.0, ry0 + 2.0, 7.0, TABLE_ROW_H * TABLE_ROWS as f32 - 4.0);
        kit::scrollbar(ui.c, t, track, true, d.top as f32 * TABLE_ROW_H, TABLE_ROW_H * TABLE_ROWS as f32, n as f32 * TABLE_ROW_H, kit::State::NORMAL);
    }
    // Count and Find box.
    let fy = b.bottom() + 6.0;
    let all = d.all_rows.len();
    let count = if n == all { format!("{n} plugins") } else { format!("{n} of {all} plugins") };
    ui.c.text(f, b.x + 4.0, (fy + 11.0 - lh / 2.0).round(), t.text_dim, &count);
    let fr = Rect::new(b.right() - 220.0, fy, 220.0, 22.0);
    let hov = ui.hit(fr, Action::Settings(SAct::Plug(Plug::Find)), "Find: type to keep the plugins whose name, vendor or file contain the text (Esc clears)");
    let well = if d.finding { t.s(0x141516) } else if hov { t.s(0x1d1f21) } else { t.s(0x191b1c) };
    kit::sunken(ui.c, t, fr, well);
    ui.c.icon(&os_ui::tabler::SEARCH, Rect::new(fr.x + 5.0, fr.cy() - 6.5, 13.0, 13.0), t.text_dim);
    let tx = fr.x + 23.0;
    let ty = (fr.cy() - lh / 2.0).round();
    if d.find.is_empty() && !d.finding {
        ui.c.text(f, tx, ty, t.text_dim, "Find");
    } else {
        ui.c.text_clip(f, tx, ty, fr.w - 30.0, t.text, &d.find);
        if d.finding {
            let cx = (tx + ui.c.text_width(f, &d.find)).min(fr.right() - 6.0);
            ui.c.fill_rect(Rect::new(cx.round() + 1.0, fr.y + 4.0, 1.0, fr.h - 8.0), t.text);
        }
    }
}

fn value_red(ui: &mut Ui, b: Rect, v: &str) {
    let t = ui.t;
    if super::lemon(ui) {
        // Lemon: number fields are the coloured dark with lemon digits.
        use os_theme::design::lemon as brand;
        lk::outlined(ui.c, b, lk::r_small(t), brand::COLORED_DARK.shade(-0.25), brand::COLORED_DARK);
        ui.c.text_center(Font::new(15), b, brand::HIGHLIGHT, v);
        return;
    }
    kit::bordered(ui.c, b, 3.0, t.h(0x5a2229), t.h(0x863c44));
    ui.c.text_center(Font::new(15), b, t.tx(0xf2e4e6), v);
}
