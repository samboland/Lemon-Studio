//! The piano roll's own buttons and menus: the toolbar's options ▸, snap magnet, Note properties and Scale and chord
//! helpers, the title's channel menu ›, the note colour tab and its slide switch, the corner view-options button, the
//! time markers in the ruler (drag to move, double-click or right-click to rename, right-click to delete) and the Note
//! properties dialog's controls. Clicks arrive as `Action::PianoRoll(PrAct)`, menu entries as `Cmd::PianoRoll(PrCmd)`.
// spec: ui/piano-roll.md §1 (title), §1a (corner button), §2 (toolbar), §6 (note colours), §10a (time markers), §12

use crate::app::{App, Drag, State};
use crate::note_props::{self, Field, Target};
use crate::piano;
use crate::piano_mouse::PDrag;
use crate::piano_scale::{Chord, KeyScale, Scale, ROOTS};
use crate::piano_snap::{self, Snap};
use crate::piano_tools::NCmd;
use crate::playlist::{Cmd, Mark, PItem, Popup};

/// A click on one of the piano roll's buttons (`Action::PianoRoll`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrAct {
    /// Toolbar ▸: the options menu.
    Options,
    /// Toolbar magnet: the snap menu.
    Snap,
    /// Toolbar: the Note properties dialog (the selection's first note, else the new-note properties).
    Props,
    /// Toolbar: scale highlighting and the chord stamp.
    Helpers,
    /// The › after the title: which channel the piano roll edits.
    ChannelMenu,
    /// The note colour tab: the colour (MIDI channel) of new notes.
    ColorMenu,
    /// The tab's slide glyph: new notes are slide notes.
    Slide,
    /// The square button above the vertical scroll bar: view options.
    View,
    /// A time marker's tab in the ruler.
    Marker(usize),
    /// Note properties dialog: − (-1) / + (1) on a row, a click on its value, Close.
    Prop(Field, i8),
    PropValue(Field),
    PropsClose,
}

/// An entry of the piano roll's menus (`Cmd::PianoRoll`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrCmd {
    Snap(Snap),
    NoScale,
    Root(i32),
    Scale(Scale),
    Stamp(Option<Chord>),
    /// Edit channel row `n` (index into the project's channels).
    Channel(usize),
    /// New notes get colour `c` (0..=15 = MIDI channels 1..=16).
    Color(u8),
    ColorSelected,
    SelectColor,
    /// New notes are / are not slide notes.
    SlideNew,
    /// Toggle the slide flag of the selection (or every note of the channel).
    SlideSelected,
    Props,
    Helpers,
    Smooth,
    AllLabels,
    NoteNames,
    Center,
    Undo,
    SelectAll,
    Deselect,
    MarkerRename(usize),
    MarkerDelete(usize),
}

fn item(label: impl Into<std::borrow::Cow<'static, str>>, c: PrCmd) -> PItem {
    PItem::new(label, Cmd::PianoRoll(c))
}

fn snap_items(s: &State) -> Vec<PItem> {
    let mut v = Vec::new();
    for snap in Snap::ALL {
        if matches!(snap, Snap::SixthStep | Snap::SixthBeat | Snap::Bar) {
            v.push(PItem::sep());
        }
        v.push(item(snap.label(), PrCmd::Snap(snap)).mark(Mark::Radio(s.piano.view.snap == snap)));
    }
    v
}

/// The magnet's menu (§2): the snap unit; Alt held while clicking or dragging ignores it.
pub fn snap_menu(s: &State, x: f32, y: f32) -> Popup {
    let mut items = vec![PItem::head("Snap (hold Alt to bypass)")];
    items.extend(snap_items(s));
    Popup { x, y, items, sub: None }
}

/// Scale and chord helpers (§2): scale highlighting (root, scale) and the Draw tool's chord stamp.
pub fn helpers_menu(s: &State, x: f32, y: f32) -> Popup {
    let cur = s.piano.scale;
    let roots = (0..12).map(|r| item(ROOTS[r as usize], PrCmd::Root(r)).mark(Mark::Radio(cur.is_some_and(|k| k.root == r)))).collect();
    let scales = Scale::ALL.into_iter().map(|sc| item(sc.label(), PrCmd::Scale(sc)).mark(Mark::Radio(cur.is_some_and(|k| k.scale == sc)))).collect();
    let chords = Chord::ALL.into_iter().map(|c| item(c.label(), PrCmd::Stamp(Some(c))).mark(Mark::Radio(s.piano.stamp == Some(c)))).collect();
    let items = vec![
        PItem::head("Scale highlighting"),
        item("Off", PrCmd::NoScale).mark(Mark::Radio(cur.is_none())),
        PItem::sub("Root note", roots),
        PItem::sub("Scale", scales),
        PItem::head("Stamp (Draw tool)"),
        item("Off", PrCmd::Stamp(None)).mark(Mark::Radio(s.piano.stamp.is_none())),
        PItem::sub("Chord", chords),
    ];
    Popup { x, y, items, sub: None }
}

/// The toolbar's ▸ (§2): editing, helpers and view options in one menu (Lemon Studio choice of entries).
pub fn options_menu(s: &State, x: f32, y: f32) -> Popup {
    let v = s.piano.view;
    let items = vec![
        PItem::head("Edit"),
        item("Undo", PrCmd::Undo).key("Ctrl+Z"),
        item("Select all", PrCmd::SelectAll).key("Ctrl+A"),
        item("Deselect", PrCmd::Deselect).key("Ctrl+D"),
        PItem::head("Helpers"),
        PItem::sub("Snap", snap_items(s)),
        item("Note properties...", PrCmd::Props),
        item("Scale and chord helpers...", PrCmd::Helpers),
        item("Slide new notes", PrCmd::SlideNew).mark(Mark::Check(s.piano.template.slide())),
        item("Toggle slide on selected notes", PrCmd::SlideSelected),
        item("Smooth event lane", PrCmd::Smooth),
        PItem::head("View"),
        PItem::new("Ghost channels", Cmd::Piano(NCmd::Ghosts)).key("Alt+V").mark(Mark::Check(s.piano.ghosts)),
        item("Key labels on every key", PrCmd::AllLabels).mark(Mark::Check(v.all_labels)),
        item("Note names", PrCmd::NoteNames).mark(Mark::Check(v.note_names)),
        PItem::new("Add time marker", Cmd::Piano(NCmd::AddMarker)).key("Alt+T"),
        item("Center on notes", PrCmd::Center),
    ];
    Popup { x, y, items, sub: None }
}

/// The corner button above the vertical scroll bar (§1a): view options.
pub fn view_menu(s: &State, x: f32, y: f32) -> Popup {
    let v = s.piano.view;
    let items = vec![
        PItem::head("View"),
        PItem::new("Ghost channels", Cmd::Piano(NCmd::Ghosts)).key("Alt+V").mark(Mark::Check(s.piano.ghosts)),
        item("Key labels on every key", PrCmd::AllLabels).mark(Mark::Check(v.all_labels)),
        item("Note names", PrCmd::NoteNames).mark(Mark::Check(v.note_names)),
        item("Center on notes", PrCmd::Center),
    ];
    Popup { x, y, items, sub: None }
}

/// The title's › (§1): the channel the piano roll edits.
pub fn channel_menu(s: &State, x: f32, y: f32) -> Popup {
    let mut items = vec![PItem::head("Piano roll channel")];
    for (i, c) in s.project.channels.iter().enumerate() {
        let name = c.name.clone().unwrap_or_else(|| format!("Channel {}", i + 1));
        items.push(item(name, PrCmd::Channel(i)).mark(Mark::Radio(i == s.selected_channel)));
    }
    Popup { x, y, items, sub: None }
}

/// The note colour tab (§6): the colour (MIDI channel) new notes get, and colouring / selecting by colour.
pub fn color_menu(s: &State, x: f32, y: f32) -> Popup {
    let cur = piano::color_of(&s.piano.template);
    let mut items = vec![PItem::head("Note color / MIDI channel")];
    for c in 0..16u8 {
        items.push(item(format!("Color {} (MIDI channel {})", c + 1, c + 1), PrCmd::Color(c)).mark(Mark::Radio(c == cur)));
    }
    items.push(PItem::sep());
    items.push(item("Color selected notes", PrCmd::ColorSelected));
    items.push(item("Select notes of this color", PrCmd::SelectColor));
    items.push(item("Slide new notes", PrCmd::SlideNew).mark(Mark::Check(s.piano.template.slide())));
    Popup { x, y, items, sub: None }
}

/// A time marker's right-click menu (§10a).
pub fn marker_menu(i: usize, x: f32, y: f32) -> Popup {
    Popup { x, y, items: vec![item("Rename...", PrCmd::MarkerRename(i)), item("Delete", PrCmd::MarkerDelete(i))], sub: None }
}

/// A piano-roll time marker's name, for the rename box (`playlist::EditTarget::PianoMarker`).
pub fn marker_name(s: &State, i: usize) -> String {
    s.project.pattern(s.pattern).and_then(|p| p.markers.get(i)).map(|m| m.name.clone()).unwrap_or_default()
}

/// Rename box accepted: an empty name keeps the old one.
pub fn rename_marker(s: &mut State, i: usize, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some(m) = piano::pattern_mut(s).markers.get_mut(i) {
        m.name = text;
    }
}

pub fn delete_marker(s: &mut State, i: usize) -> Option<String> {
    let markers = &mut piano::pattern_mut(s).markers;
    (i < markers.len()).then(|| format!("Piano roll: deleted \"{}\"", markers.remove(i).name))
}

/// Move marker `i` to `tick`; the list is put back in position order when the drag ends (`sort_markers`).
pub fn move_marker(s: &mut State, i: usize, tick: u32) {
    if let Some(m) = piano::pattern_mut(s).markers.get_mut(i) {
        m.position = tick;
    }
}

pub fn sort_markers(s: &mut State) {
    piano::pattern_mut(s).markers.sort_by_key(|m| m.position);
}

/// Toggle the slide flag of the selection (or every note of the channel); returns how many notes changed.
fn slide_selected(s: &mut State) -> usize {
    let idx = piano::targets(s);
    if idx.is_empty() {
        return 0;
    }
    let on = !idx.iter().all(|&i| piano::notes(s).get(i).is_some_and(|n| n.slide()));
    piano::snapshot(s);
    let p = piano::pattern_mut(s);
    for &i in &idx {
        if let Some(n) = p.notes.get_mut(i) {
            piano::set_slide(n, on);
        }
    }
    idx.len()
}

fn set_view(s: &mut State, f: impl FnOnce(&mut piano_snap::View)) {
    f(&mut s.piano.view);
    piano_snap::save(s.piano.view);
}

/// Open the Note properties dialog: on the selection's first note, else on the new-note properties.
pub fn open_props(s: &mut State) {
    let sel = s.piano.selection(s.pattern, piano::channel(s));
    let target = sel.iter().min().map_or(Target::NewNotes, |&i| Target::Note(i));
    note_props::open(s, target);
}

/// Run a menu entry; returns a message for the hint bar.
pub fn run(app: &mut App, c: PrCmd) -> Option<String> {
    app.invalidate();
    let s = &mut app.state;
    match c {
        PrCmd::Snap(snap) => {
            set_view(s, |v| v.snap = snap);
            return Some(format!("Piano roll snap: {}", snap.label()));
        }
        PrCmd::NoScale => s.piano.scale = None,
        PrCmd::Root(root) => {
            let scale = s.piano.scale.map_or(Scale::Major, |k| k.scale);
            s.piano.scale = Some(KeyScale { root, scale });
        }
        PrCmd::Scale(scale) => {
            let root = s.piano.scale.map_or(0, |k| k.root);
            s.piano.scale = Some(KeyScale { root, scale });
        }
        PrCmd::Stamp(chord) => {
            s.piano.stamp = chord;
            return Some(chord.map_or("Stamp off".into(), |c| format!("Stamp: {} (click with the Draw tool)", c.label())));
        }
        PrCmd::Channel(i) => {
            if i < s.project.channels.len() {
                app.focus_channel(i);
                app.state.rack.select_only(i);
            }
            return None;
        }
        PrCmd::Color(c) => piano::set_color(&mut s.piano.template, c),
        PrCmd::ColorSelected => {
            let c = piano::color_of(&s.piano.template);
            let idx = s.piano.selection(s.pattern, piano::channel(s));
            if idx.is_empty() {
                return Some("Piano roll: select the notes to color first".into());
            }
            piano::snapshot(s);
            let p = piano::pattern_mut(s);
            for &i in &idx {
                if let Some(n) = p.notes.get_mut(i) {
                    piano::set_color(n, c);
                }
            }
        }
        PrCmd::SelectColor => {
            let c = piano::color_of(&s.piano.template);
            let Some(ch) = piano::channel(s) else { return None };
            let sel = piano::notes(s).iter().enumerate().filter(|(_, n)| n.channel == ch && piano::color_of(n) == c).map(|(i, _)| i).collect();
            s.piano.select(s.pattern, ch, sel);
        }
        PrCmd::SlideNew => {
            let on = !s.piano.template.slide();
            piano::set_slide(&mut s.piano.template, on);
            return Some(if on { "New notes are slide notes" } else { "New notes are normal notes" }.into());
        }
        PrCmd::SlideSelected => {
            if slide_selected(s) == 0 {
                return Some("Piano roll: no notes".into());
            }
        }
        PrCmd::Props => open_props(s),
        PrCmd::Helpers => {
            let (x, y) = app.mouse;
            app.state.playlist.popup = Some(helpers_menu(&app.state, x, y));
        }
        PrCmd::Smooth => {
            if !piano::smooth_lane(s) {
                return Some("Smooth: needs a note lane and three notes or more".into());
            }
        }
        PrCmd::AllLabels => set_view(s, |v| v.all_labels = !v.all_labels),
        PrCmd::NoteNames => set_view(s, |v| v.note_names = !v.note_names),
        PrCmd::Center => {
            let h = app.piano_grid.map_or(700.0, |g| g.h);
            piano::center_keys(&mut app.state, h);
        }
        PrCmd::Undo => {
            if !piano::undo(s) {
                return Some("Nothing to undo".into());
            }
        }
        PrCmd::SelectAll => {
            if let Some(ch) = piano::channel(s) {
                let all = piano::notes(s).iter().enumerate().filter(|(_, n)| n.channel == ch).map(|(i, _)| i).collect();
                s.piano.select(s.pattern, ch, all);
            }
        }
        PrCmd::Deselect => s.piano.clear_selection(),
        PrCmd::MarkerRename(i) => {
            let at = marker_at(app, i);
            crate::playlist::start_edit(&mut app.state, crate::playlist::EditTarget::PianoMarker(i), at);
        }
        PrCmd::MarkerDelete(i) => return delete_marker(s, i),
    }
    app.state.piano.dirty = true;
    None
}

/// Where a marker's rename box goes: over its tab.
fn marker_at(app: &App, i: usize) -> (f32, f32) {
    let a = crate::app::Action::PianoRoll(PrAct::Marker(i));
    app.hits.iter().rev().find(|h| h.action == a).map_or(app.mouse, |h| (h.rect.x, h.rect.y))
}

impl App {
    /// A left click on one of the piano roll's buttons.
    pub(crate) fn piano_roll_act(&mut self, a: PrAct) {
        let (x, y) = self.mouse;
        self.invalidate();
        let s = &mut self.state;
        match a {
            PrAct::Options => s.playlist.popup = Some(options_menu(s, x - 6.0, y + 10.0)),
            PrAct::Snap => s.playlist.popup = Some(snap_menu(s, x - 6.0, y + 10.0)),
            PrAct::Helpers => s.playlist.popup = Some(helpers_menu(s, x - 6.0, y + 10.0)),
            PrAct::ChannelMenu => s.playlist.popup = Some(channel_menu(s, x - 6.0, y + 10.0)),
            PrAct::ColorMenu => s.playlist.popup = Some(color_menu(s, x, y)),
            PrAct::View => s.playlist.popup = Some(view_menu(s, x - 150.0, y + 10.0)),
            PrAct::Props => {
                if s.piano.props.is_some() {
                    s.piano.props = None;
                } else {
                    open_props(s);
                }
            }
            PrAct::Slide => {
                if let Some(m) = run(self, PrCmd::SlideNew) {
                    self.state.message = Some(m);
                }
            }
            PrAct::Marker(i) => {
                let now = std::time::Instant::now();
                let double = s.piano.marker_click.is_some_and(|(j, t)| j == i && now.duration_since(t).as_millis() < 400);
                s.piano.marker_click = if double { None } else { Some((i, now)) };
                if double {
                    run(self, PrCmd::MarkerRename(i));
                } else {
                    self.drag = Some(Drag::Piano(PDrag::Marker { i }));
                }
            }
            PrAct::Prop(f, dir) => {
                let k = if self.shift { 10 } else { 1 };
                note_props::step(s, f, i64::from(dir) * k);
            }
            PrAct::PropValue(f) => note_props::click(s, f),
            PrAct::PropsClose => s.piano.props = None,
        }
    }

    /// A right click on one of them; returns whether it was taken.
    pub(crate) fn piano_roll_right(&mut self, a: PrAct) -> bool {
        let (x, y) = self.mouse;
        match a {
            PrAct::Marker(i) => self.state.playlist.popup = Some(marker_menu(i, x, y)),
            PrAct::Prop(f, _) | PrAct::PropValue(f) => note_props::reset(&mut self.state, f),
            _ => return false,
        }
        self.invalidate();
        true
    }
}

/// The piano roll's shortcuts (`menu_registry::KEYS`): Esc closes the Note properties dialog.
pub fn key(app: &mut App, key: &winit::keyboard::Key, mods: crate::menu_registry::Mods) -> bool {
    use winit::keyboard::{Key, NamedKey};
    if matches!(key, Key::Named(NamedKey::Escape)) && !mods.ctrl && app.state.piano.props.is_some() {
        app.state.piano.props = None;
        app.invalidate();
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Action, Win};
    use crate::piano::Tool;
    use os_ui::{Canvas, Rect};

    fn app() -> App {
        let mut a = App::new(None);
        a.state.selected_channel = 4;
        a.state.z = vec![Win::PianoRoll];
        a
    }

    fn frame(a: &mut App, c: &mut Canvas) {
        a.paint(c, None);
    }

    fn rect(a: &App, act: &Action) -> Rect {
        a.hits.iter().rev().find(|h| &h.action == act).map(|h| h.rect).unwrap_or_else(|| panic!("no hit for {act:?}"))
    }

    fn press(a: &mut App, c: &mut Canvas, (x, y): (f32, f32)) {
        a.mouse_move(x, y);
        a.mouse_down();
        a.mouse_up();
        frame(a, c);
    }

    fn click(a: &mut App, c: &mut Canvas, act: Action) {
        let r = rect(a, &act);
        press(a, c, (r.cx(), r.cy()));
    }

    /// Choose the popup entry labelled `label` (in a submenu when `sub` names its parent).
    fn choose(a: &mut App, c: &mut Canvas, sub: Option<&str>, label: &str) {
        let items = a.state.playlist.popup.as_ref().expect("a popup is open").items.clone();
        match sub {
            Some(parent) => {
                let i = items.iter().position(|it| it.label == parent).unwrap_or_else(|| panic!("no submenu {parent}"));
                a.act(Action::PlPopup(i));
                let j = items[i].children.iter().position(|it| it.label == label).unwrap_or_else(|| panic!("no entry {label}"));
                a.act(Action::PlPopupSub(j));
            }
            None => {
                let i = items.iter().position(|it| it.label == label).unwrap_or_else(|| panic!("no entry {label}"));
                a.act(Action::PlPopup(i));
            }
        }
        frame(a, c);
    }

    /// Centre of the grid cell of `key` at `tick`.
    fn at(a: &App, tick: f32, key: i32) -> (f32, f32) {
        let g = a.piano_geom().expect("piano roll shown");
        (g.x_of(tick) + 3.0, g.y_of(key) + g.row / 2.0)
    }

    /// Every toolbar entry, the title's channel menu, the colour tab and the corner button work: no placeholder is left
    /// in the piano roll.
    #[test]
    fn nothing_in_the_piano_roll_is_a_placeholder() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        frame(&mut a, &mut c);
        let left: Vec<_> = a.hits.iter().filter(|h| matches!(h.action, Action::Placeholder(_)) && h.win == Some(Win::PianoRoll)).map(|h| h.action.clone()).collect();
        assert!(left.is_empty(), "still greyed: {left:?}");
        for act in [PrAct::Options, PrAct::Snap, PrAct::Props, PrAct::Helpers, PrAct::ChannelMenu, PrAct::ColorMenu, PrAct::Slide, PrAct::View] {
            rect(&a, &Action::PianoRoll(act));
        }
        rect(&a, &Action::PianoTool(Tool::PaintAdd));
    }

    /// The magnet's menu sets the snap (kept per user, `pianoview.txt`); drags follow it and Alt bypasses it.
    #[test]
    fn snap_menu_and_alt_bypass() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        frame(&mut a, &mut c);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Snap));
        choose(&mut a, &mut c, None, "1/2 beat");
        assert_eq!(a.state.piano.view.snap, Snap::HalfBeat);
        assert_eq!(piano_snap::parse(&piano_snap::text(a.state.piano.view)).snap, Snap::HalfBeat, "what is saved reads back");
        assert_eq!(a.piano_geom().unwrap().step, 48);
        // A click at tick 70 draws at 48; dragging 30 ticks right stays (rounds to 0), 30 more moves one half beat.
        let (x, y) = at(&a, 70.0, 60);
        a.mouse_move(x, y);
        a.mouse_down();
        let ppt = a.piano_geom().unwrap().ppt;
        a.mouse_move(x + 20.0 * ppt, y);
        assert_eq!(piano::notes(&a.state)[0].position, 48);
        a.mouse_move(x + 30.0 * ppt, y);
        assert_eq!(piano::notes(&a.state)[0].position, 96);
        a.mouse_up();
        frame(&mut a, &mut c);
        // Alt: no snapping while held.
        a.alt = true;
        let (x, y) = at(&a, 300.0, 64);
        press(&mut a, &mut c, (x, y));
        let n = piano::notes(&a.state).iter().find(|n| n.key == 64).copied().unwrap();
        assert!(n.position > 288 && n.position < 312 && n.position % 48 != 0, "unsnapped: {}", n.position);
        a.alt = false;
        // Snap off: one tick.
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Snap));
        choose(&mut a, &mut c, None, "(none)");
        assert_eq!(a.piano_geom().unwrap().step, 1);
    }

    /// Double-click a note: its properties; − / + change it and the change is heard; the toolbar button without a
    /// selection edits what new notes get; Esc closes.
    #[test]
    fn note_properties_dialog() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        let i = piano::add_note(&mut a.state, 4, 0, 60);
        frame(&mut a, &mut c);
        let r = rect(&a, &Action::PianoNote(i));
        press(&mut a, &mut c, (r.x + 4.0, r.cy()));
        assert!(a.state.piano.props.is_none());
        press(&mut a, &mut c, (r.x + 4.0, r.cy()));
        assert_eq!(a.state.piano.props.map(|p| p.target), Some(Target::Note(i)), "double click opens the dialog");
        for _ in 0..2 {
            click(&mut a, &mut c, Action::PianoRoll(PrAct::Prop(Field::Key, 1)));
        }
        a.shift = true;
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Prop(Field::Velocity, -1)));
        a.shift = false;
        let n = piano::notes(&a.state)[0];
        assert_eq!((n.key, n.velocity), (62, 90));
        // Right-click on a value: the default.
        let r = rect(&a, &Action::PianoRoll(PrAct::PropValue(Field::Velocity)));
        a.mouse_move(r.cx(), r.cy());
        a.mouse_down_right();
        a.mouse_up();
        assert_eq!(piano::notes(&a.state)[0].velocity, 100);
        // The engine plays the edited key.
        let mut lib = os_engine::SampleLibrary::new(None);
        let mut e = os_engine::Engine::new(&a.state.project, &mut lib, 48000, os_engine::PlayMode::Pattern(1));
        let mut buf = vec![0.0f32; 2 * 512];
        e.process(&mut buf);
        assert_eq!(e.sounding().collect::<Vec<_>>(), vec![(4, 62)]);
        use winit::keyboard::{Key, NamedKey};
        a.key(&Key::Named(NamedKey::Escape), false, false);
        assert!(a.state.piano.props.is_none(), "Esc closes");
        frame(&mut a, &mut c);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Props));
        assert_eq!(a.state.piano.props.map(|p| p.target), Some(Target::NewNotes));
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Prop(Field::Release, 1)));
        assert_eq!(a.state.piano.template.release, 65);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::PropsClose));
        assert!(a.state.piano.props.is_none());
    }

    /// The colour tab: new notes get the chosen colour (MIDI channel); colour and select by colour; the slide glyph
    /// makes new notes slide notes, which the engine plays as slides.
    #[test]
    fn note_colors_and_slide() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        frame(&mut a, &mut c);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::ColorMenu));
        choose(&mut a, &mut c, None, "Color 5 (MIDI channel 5)");
        assert_eq!(piano::color_of(&a.state.piano.template), 4);
        let (x, y) = at(&a, 0.0, 60);
        press(&mut a, &mut c, (x, y));
        assert_eq!(piano::color_of(&piano::notes(&a.state)[0]), 4);
        let j = piano::add_note(&mut a.state, 4, 192, 67);
        piano::set_color(&mut piano::pattern_mut(&mut a.state).notes[j], 0);
        frame(&mut a, &mut c);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::ColorMenu));
        choose(&mut a, &mut c, None, "Select notes of this color");
        assert_eq!(a.state.piano.selection(a.state.pattern, Some(4)), vec![0]);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::ColorMenu));
        choose(&mut a, &mut c, None, "Color 2 (MIDI channel 2)");
        click(&mut a, &mut c, Action::PianoRoll(PrAct::ColorMenu));
        choose(&mut a, &mut c, None, "Color selected notes");
        assert_eq!(piano::color_of(&piano::notes(&a.state)[0]), 1);
        assert_eq!(piano::color_of(&piano::notes(&a.state)[1]), 0, "unselected notes keep theirs");
        // Slide.
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Slide));
        assert!(a.state.piano.template.slide());
        let (x, y) = at(&a, 96.0, 72);
        press(&mut a, &mut c, (x, y));
        let slide = piano::notes(&a.state).iter().find(|n| n.key == 72).copied().unwrap();
        assert!(slide.slide(), "drawn as a slide note");
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Slide));
        assert!(!a.state.piano.template.slide());
        // A slide note makes no voice of its own: only the two normal notes are heard.
        let mut lib = os_engine::SampleLibrary::new(None);
        let mut e = os_engine::Engine::new(&a.state.project, &mut lib, 48000, os_engine::PlayMode::Pattern(1));
        let mut buf = vec![0.0f32; 2 * 256];
        let mut keys = Vec::new();
        for _ in 0..400 {
            e.process(&mut buf);
            keys.extend(e.sounding().map(|(_, k)| k));
        }
        assert!(keys.contains(&60) && keys.contains(&67), "{keys:?}");
        assert!(!keys.contains(&72), "the slide note glides, it does not sound on its own");
        // Toggle slide on the selection from the options menu.
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Options));
        choose(&mut a, &mut c, None, "Toggle slide on selected notes");
        assert!(piano::notes(&a.state)[0].slide());
    }

    /// Scale highlighting (root, scale) and the chord stamp: a Draw click places the chord, which plays.
    #[test]
    fn scale_and_chord_helpers() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        frame(&mut a, &mut c);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Helpers));
        choose(&mut a, &mut c, Some("Root note"), "D");
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Helpers));
        choose(&mut a, &mut c, Some("Scale"), "Dorian");
        assert_eq!(a.state.piano.scale, Some(KeyScale { root: 2, scale: Scale::Dorian }));
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Helpers));
        choose(&mut a, &mut c, Some("Chord"), "Minor 7th");
        assert_eq!(a.state.piano.stamp, Some(Chord::Minor7));
        let (x, y) = at(&a, 0.0, 62);
        press(&mut a, &mut c, (x, y));
        let mut keys: Vec<u16> = piano::notes(&a.state).iter().map(|n| n.key).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec![62, 65, 69, 72]);
        let mut lib = os_engine::SampleLibrary::new(None);
        let mut e = os_engine::Engine::new(&a.state.project, &mut lib, 48000, os_engine::PlayMode::Pattern(1));
        let mut buf = vec![0.0f32; 2 * 512];
        e.process(&mut buf);
        let mut heard: Vec<u16> = e.sounding().map(|(_, k)| k).collect();
        heard.sort_unstable();
        assert_eq!(heard, vec![62, 65, 69, 72], "the stamped chord plays");
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Helpers));
        choose(&mut a, &mut c, None, "Off");
        assert_eq!(a.state.piano.scale, None);
    }

    /// The title's › switches the edited channel; the corner button and the options menu toggle view options.
    #[test]
    fn channel_menu_and_view_options() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        frame(&mut a, &mut c);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::ChannelMenu));
        let pop = a.state.playlist.popup.clone().expect("channel menu");
        assert_eq!(pop.items.len(), a.state.project.channels.len() + 1);
        let i = pop.items.iter().position(|it| it.cmd == Cmd::PianoRoll(PrCmd::Channel(1))).unwrap();
        a.act(Action::PlPopup(i));
        assert_eq!(a.state.selected_channel, 1);
        frame(&mut a, &mut c);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::View));
        choose(&mut a, &mut c, None, "Key labels on every key");
        assert!(a.state.piano.view.all_labels);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Options));
        choose(&mut a, &mut c, None, "Note names");
        assert!(!a.state.piano.view.note_names);
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Options));
        choose(&mut a, &mut c, Some("Snap"), "Bar");
        assert_eq!(a.state.piano.view.snap, Snap::Bar);
    }

    /// Paint and add: a drag across rows adds a note in every slot it enters.
    #[test]
    fn paint_and_add() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        frame(&mut a, &mut c);
        click(&mut a, &mut c, Action::PianoTool(Tool::PaintAdd));
        assert_eq!(a.state.piano.tool, Tool::PaintAdd);
        let (x, y) = at(&a, 0.0, 60);
        a.mouse_move(x, y);
        a.mouse_down();
        for (tick, key) in [(10.0, 60), (50.0, 61), (100.0, 63), (110.0, 63), (150.0, 64)] {
            let (x, y) = at(&a, tick, key);
            a.mouse_move(x, y);
        }
        a.mouse_up();
        let got: Vec<(u32, u16)> = piano::notes(&a.state).iter().map(|n| (n.position, n.key)).collect();
        assert_eq!(got, vec![(0, 60), (48, 61), (96, 63), (144, 64)]);
        assert!(piano::undo(&mut a.state) && piano::notes(&a.state).is_empty(), "one undo step");
    }

    /// Event panel: right-drag puts the default back, Alt+drag draws a straight line, smoothing evens the values.
    #[test]
    fn event_lane_aids() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        for k in 0..4 {
            let i = piano::add_note(&mut a.state, 4, k * 96, 60);
            piano::pattern_mut(&mut a.state).notes[i].velocity = [10, 120, 10, 120][k as usize];
        }
        frame(&mut a, &mut c);
        let panel = rect(&a, &Action::PianoVelocity);
        let g = a.piano_geom().unwrap();
        // Smooth: 1-2-1.
        click(&mut a, &mut c, Action::PianoRoll(PrAct::Options));
        choose(&mut a, &mut c, None, "Smooth event lane");
        let v: Vec<u8> = piano::notes(&a.state).iter().map(|n| n.velocity).collect();
        assert_eq!(v, vec![38, 65, 65, 93]);
        // Right-drag over the first two: default velocity.
        a.mouse_move(g.x_of(0.0), panel.cy());
        a.mouse_down_right();
        a.mouse_move(g.x_of(96.0), panel.cy());
        a.mouse_up();
        let v: Vec<u8> = piano::notes(&a.state).iter().map(|n| n.velocity).collect();
        assert_eq!(v, vec![100, 100, 65, 93]);
        // Alt+drag from the bottom at the first note to the top at the last: a rising line.
        frame(&mut a, &mut c);
        a.alt = true;
        a.mouse_move(g.x_of(0.0) + 1.0, panel.bottom() - 1.0);
        a.mouse_down();
        a.mouse_move(g.x_of(288.0), panel.y + 30.0);
        a.mouse_move(g.x_of(288.0) + 1.0, panel.y + 1.0);
        a.mouse_up();
        a.alt = false;
        let v: Vec<u8> = piano::notes(&a.state).iter().map(|n| n.velocity).collect();
        assert!(v[0] <= 2 && v[3] >= 126 && v.windows(2).all(|w| w[0] < w[1]), "{v:?}");
    }

    /// Time markers: drag to move, right-click menu to rename or delete, double-click to rename.
    #[test]
    fn time_markers_move_rename_delete() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        crate::piano_tools::add_marker(&mut a.state, 0);
        crate::piano_tools::add_marker(&mut a.state, 384);
        frame(&mut a, &mut c);
        let r = rect(&a, &Action::PianoRoll(PrAct::Marker(0)));
        let g = a.piano_geom().unwrap();
        a.mouse_move(r.x + 2.0, r.cy());
        a.mouse_down();
        a.mouse_move(g.x_of(480.0) + 2.0, r.cy());
        a.mouse_up();
        let ms: Vec<(u32, String)> = a.state.project.pattern(a.state.pattern).unwrap().markers.iter().map(|m| (m.position, m.name.clone())).collect();
        assert_eq!(ms, vec![(384, "Marker 2".to_string()), (480, "Marker 1".to_string())], "moved and kept in order");
        frame(&mut a, &mut c);
        let r = rect(&a, &Action::PianoRoll(PrAct::Marker(1)));
        a.mouse_move(r.cx(), r.cy());
        a.mouse_down_right();
        a.mouse_up();
        choose(&mut a, &mut c, None, "Rename...");
        let e = a.state.playlist.edit.as_mut().expect("rename box");
        assert_eq!(e.text, "Marker 1");
        e.text = "Chorus".into();
        crate::playlist::commit_edit(&mut a.state);
        assert_eq!(marker_name(&a.state, 1), "Chorus");
        // Double-click: the rename box.
        frame(&mut a, &mut c);
        let r = rect(&a, &Action::PianoRoll(PrAct::Marker(0)));
        press(&mut a, &mut c, (r.cx(), r.cy()));
        press(&mut a, &mut c, (r.cx(), r.cy()));
        assert_eq!(a.state.playlist.edit.as_ref().map(|e| e.text.as_str()), Some("Marker 2"));
        a.state.playlist.edit = None;
        frame(&mut a, &mut c);
        let r = rect(&a, &Action::PianoRoll(PrAct::Marker(0)));
        a.mouse_move(r.cx(), r.cy());
        a.mouse_down_right();
        a.mouse_up();
        choose(&mut a, &mut c, None, "Delete");
        assert_eq!(a.state.project.pattern(a.state.pattern).unwrap().markers.len(), 1);
    }

    /// Double-click a ghost note: its channel becomes the piano roll's, and the first click's note is taken back.
    #[test]
    fn ghost_double_click_switches_channel() {
        let Ok(mut c) = Canvas::new(1920, 1080, 1.0) else { return };
        let mut a = app();
        let other = a.state.project.channels[2].index;
        let i = piano::add_note(&mut a.state, other, 96, 60);
        piano::pattern_mut(&mut a.state).notes[i].length = 96;
        frame(&mut a, &mut c);
        let p = at(&a, 120.0, 60);
        press(&mut a, &mut c, p);
        assert_eq!(piano::notes(&a.state).len(), 2, "the first click draws");
        press(&mut a, &mut c, p);
        assert_eq!(a.state.selected_channel, 2);
        assert_eq!(piano::notes(&a.state).len(), 1, "the drawn note is taken back");
    }
}
