//! Mixer behaviour: track selection, mute / solo, routing and send levels, the track / options / layout menus,
//! adding / moving / deleting tracks, the effect slots that host plugins, the track equaliser, docking, groups,
//! track states and rendering one track to a wave file.
//!
// spec: ui/mixer.md (every interaction), flp/events/mixer.md (what is stored), audio/gain-laws.md (fader law),
// audio/mixer.md ("Equaliser")

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, VecDeque};

use os_model::{MixerTrack, Rgb};

use crate::app::{Action, App, Drag, State, Win, HINT_RECT};
use crate::playlist::{Cmd, EditTarget, Mark, PItem, Popup};
use crate::views;

/// Mixer controls (the view registers them as `Action::Mix`). Controls that are not available yet (multi-touch, input
/// pin, latency clock, disk recording, input / output combos) are greyed out with `Ui::off` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MHit {
    /// Anywhere on a strip that is not a control.
    Strip(u16),
    Led(u16),
    Polarity(u16),
    Swap(u16),
    /// The ▲ "route to this track" button.
    Route(u16),
    /// The small green "^" above a send knob (removes that route).
    Unroute(u16),
    /// "+" after the last strip; title-bar "+" (after the selected track).
    Add,
    AddAfter,
    Options,
    Layout,
    /// Effect slot `n` of the current track: name, ▸ arrow, LED.
    SlotName(u8),
    SlotArrow(u8),
    SlotLed(u8),
    /// The level scale column.
    Scale,
    /// Title-bar view buttons: [`VIEW_WAVES`], [`VIEW_EXTRA`], [`VIEW_INSPECTOR`].
    View(u8),
}

/// Where a track is docked (track menu > Layout > Dock to; spec: ui/mixer.md §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dock {
    Left,
    Middle,
    Right,
}

/// Menu commands (`Cmd::Mixer`).
#[derive(Debug, Clone, PartialEq)]
pub enum MCmd {
    Rename(u16),
    Color(u16, Option<Rgb>),
    RandomColor(u16),
    /// The selected tracks shade from the first one's colour to the last one's.
    Gradient(u16),
    Reset(u16),
    ResetRouting(u16),
    UnrouteTo(u16),
    /// The selected channels (channel rack) go to this track.
    RouteChannels(u16),
    /// Track routing submenu: route `.0` to `.1` on / off.
    ToggleRoute(u16, u16),
    /// Select the tracks this one sends to.
    SelectRouted(u16),
    Solo(u16),
    /// Solo with the tracks the selected ones send to and receive from.
    AltSolo(u16),
    /// Group: separator before the track, a group of the selected tracks, one colour for a group.
    Separator(u16),
    Group(u16),
    AutoColor(u16),
    Dock(u16, Dock),
    MoveLeft(u16),
    MoveRight(u16),
    Delete(u16),
    InsertOne(u16),
    InsertMany(u16, u16),
    /// Track states (`mixer_states`): keep the track's settings under its name; insert a track from one.
    SaveState(u16),
    InsertState(u16, String),
    /// File > Render to wave file: what the track plays, offline.
    Render(u16),
    /// Route the selected tracks to `dst`: only = drop their other routes, side = sidechain (level 0).
    Route { dst: u16, only: bool, side: bool },
    /// Layout menu: one of [`LAYOUTS`].
    Layout(u8),
    /// Options > View and the title-bar view buttons.
    View(u8),
    /// Effect slots of the current track.
    AddEffect(u8, usize),
    SlotDelete(u8),
    SlotMove(u8, i8),
    SlotRename(u8),
    SlotView(u8),
    /// Spare state: keep the plugin's state aside; swap it with the kept one.
    SlotStoreSpare(u8),
    SlotFlipSpare(u8),
}

/// What the plugin host does next tick for a mixer slot (`App::mixer_tick`).
#[derive(Debug, Clone, PartialEq)]
pub enum FxReq {
    /// Host effect `index` in the slot, from `state` when given (a track state).
    Create { track: u16, slot: u8, index: usize, state: Option<Vec<u8>> },
    Toggle { track: u16, slot: u8 },
    Show { track: u16, slot: u8 },
    Delete { track: u16, slot: u8 },
    /// Slots moved inside a track or tracks moved: `map[(t, s)] = (t2, s2)`; unlisted instances are dropped.
    Remap(Vec<((u16, u8), (u16, u8))>),
}

pub const LAYOUTS: [&str; 8] = ["Compact", "Compact 2", "Compact alt", "Wide", "Wide 2", "Wide 3", "Wide alt", "Extra large"];
/// Index of "Wide" (the default layout).
pub const WIDE: u8 = 3;

/// Strip pitch of each layout (Lemon Studio choice: the layouts differ in strip width; the "alt" ones put the slot
/// panel on the left).
const LAYOUT_W: [f32; 8] = [34.0, 30.0, 34.0, 44.0, 50.0, 56.0, 44.0, 64.0];

/// Strip pitch of layout `l`.
pub fn strip_w(l: u8) -> f32 {
    LAYOUT_W.get(usize::from(l)).copied().unwrap_or(views::mixer::STRIP_W)
}

/// Whether layout `l` puts the slot panel on the left.
pub fn panel_left(l: u8) -> bool {
    l == 2 || l == 6
}

/// View options (Options > View, title-bar buttons).
pub const VIEW_WAVES: u8 = 0;
pub const VIEW_EXTRA: u8 = 1;
pub const VIEW_INSPECTOR: u8 = 2;
/// Peak history kept per track for the waveform view (frames).
pub const WAVE_LEN: usize = 96;

/// The mixer's view options, kept in the user folder (`mixer.txt`) with the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    /// Each strip shows its recent peaks as a waveform behind its name.
    pub waveforms: bool,
    /// The polarity, swap and stereo separation controls.
    pub extra: bool,
    /// The selected track's properties beside the slot panel.
    pub inspector: bool,
}

impl Default for View {
    fn default() -> Self {
        View { waveforms: false, extra: true, inspector: false }
    }
}

impl View {
    pub fn get(&self, k: u8) -> bool {
        match k {
            VIEW_WAVES => self.waveforms,
            VIEW_EXTRA => self.extra,
            _ => self.inspector,
        }
    }

    fn toggle(&mut self, k: u8) {
        match k {
            VIEW_WAVES => self.waveforms = !self.waveforms,
            VIEW_EXTRA => self.extra = !self.extra,
            _ => self.inspector = !self.inspector,
        }
    }
}

/// Mixer state that is not project data.
pub struct MixerUi {
    /// Selected tracks besides the current one (Ctrl+click, §2).
    pub sel: Vec<u16>,
    pub layout: u8,
    pub view: View,
    /// Slot names typed in the rename box (session only).
    pub slot_names: HashMap<(u16, u8), String>,
    /// Docked tracks (absent = middle) and tracks with a separator before them (session only: the project model has
    /// no place for them yet).
    pub dock: HashMap<u16, Dock>,
    pub seps: BTreeSet<u16>,
    /// Spare plugin states per slot (session only).
    pub spare: HashMap<(u16, u8), Vec<u8>>,
    pub pending: Vec<FxReq>,
    /// Meter state per track (`meters::Meter`), fed by `feed_meters`.
    pub meters: Vec<crate::meters::Meter>,
    /// Recent peaks per track (newest first) for the waveform view.
    pub waves: Vec<VecDeque<f32>>,
    /// Meter ballistics and look.
    pub meter_cfg: crate::meters::Settings,
    /// When the meters last advanced.
    pub meter_at: Option<std::time::Instant>,
}

impl Default for MixerUi {
    fn default() -> Self {
        let (layout, view) = load_prefs();
        MixerUi { sel: Vec::new(), layout, view, slot_names: HashMap::new(), dock: HashMap::new(), seps: BTreeSet::new(),
            spare: HashMap::new(), pending: Vec::new(), meters: Vec::new(), waves: Vec::new(), meter_cfg: Default::default(),
            meter_at: None }
    }
}

/// `key=value` lines of `mixer.txt`: `layout` (0..7), `waveforms`, `extra`, `inspector` (0 / 1); the rest is ignored.
pub fn parse_prefs(text: &str) -> (u8, View) {
    let (mut layout, mut view) = (WIDE, View::default());
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "layout" => layout = v.parse::<u8>().ok().filter(|&l| usize::from(l) < LAYOUTS.len()).unwrap_or(layout),
            "waveforms" => view.waveforms = v == "1",
            "extra" => view.extra = v == "1",
            "inspector" => view.inspector = v == "1",
            _ => {}
        }
    }
    (layout, view)
}

pub fn prefs_text(layout: u8, v: &View) -> String {
    format!("layout={layout}\nwaveforms={}\nextra={}\ninspector={}\n", u8::from(v.waveforms), u8::from(v.extra), u8::from(v.inspector))
}

/// The layout and view kept in the user folder (tests: the defaults).
fn load_prefs() -> (u8, View) {
    if cfg!(test) {
        return (WIDE, View::default());
    }
    os_paths::user_file("mixer.txt").and_then(|f| std::fs::read_to_string(f).ok()).map(|t| parse_prefs(&t)).unwrap_or((WIDE, View::default()))
}

fn save_prefs(ui: &MixerUi) {
    if cfg!(test) {
        return;
    }
    if let Some(dir) = os_paths::user_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("mixer.txt"), prefs_text(ui.layout, &ui.view));
    }
}

/// Highest insert a channel can target (at least 1).
pub fn max_insert(p: &os_model::Project) -> u16 {
    max_insert_of(&p.mixer)
}

pub fn max_insert_of(mixer: &[MixerTrack]) -> u16 {
    (mixer.len().saturating_sub(2) as u16).max(1)
}

/// Inserts shown: stored tracks minus the master and the last (hidden) one (§1).
pub fn inserts(s: &State) -> u16 {
    s.project.mixer.len().saturating_sub(2) as u16
}

pub fn track(s: &State, n: u16) -> Option<&MixerTrack> {
    s.project.mixer.iter().find(|m| m.index == n)
}

fn track_mut(s: &mut State, n: u16) -> Option<&mut MixerTrack> {
    s.project.mixer.iter_mut().find(|m| m.index == n)
}

/// "Master", "Insert 3", or the track's own name.
pub fn name(s: &State, n: u16) -> String {
    track(s, n).and_then(|m| m.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| if n == 0 { "Master".into() } else { format!("Insert {n}") })
}

/// The current track first, then the other selected ones.
pub fn selected(s: &State) -> Vec<u16> {
    let mut v = vec![s.selected_mixer];
    v.extend(s.mixer.sel.iter().copied().filter(|&t| t != s.selected_mixer));
    v
}

pub fn is_selected(s: &State, n: u16) -> bool {
    s.selected_mixer == n || s.mixer.sel.contains(&n)
}

/// Whether track `src` sends to `dst` (route present), and at what level (fader scale, 0.8 = unity).
pub fn send(s: &State, src: u16, dst: u16) -> Option<f64> {
    let t = track(s, src)?;
    t.routes.contains(&dst).then(|| t.send_levels.iter().find(|(d, _)| *d == dst).map_or(0.8, |&(_, l)| l))
}

/// Whether any track routes into `n`.
pub fn receives(s: &State, n: u16) -> bool {
    s.project.mixer.iter().any(|m| m.index != n && m.routes.contains(&n))
}

/// The slot's shown name (renamed, else the plugin's name).
pub fn slot_name(s: &State, t: u16, slot: u8) -> Option<String> {
    let p = track(s, t)?.slots.get(usize::from(slot))?.as_ref()?;
    Some(s.mixer.slot_names.get(&(t, slot)).cloned().unwrap_or_else(|| crate::project_plugins::slot_title(p)))
}

/// "Volume: 100%  0.0dB  1.00" (§4); `label` = "Volume" or "To Insert 3".
pub fn level_text(label: &str, v: f64) -> String {
    let g = os_engine::laws::fader_gain(v);
    let db = if g > 0.0 { format!("{:.1}dB", 20.0 * g.log10()) } else { "-INFdB".into() };
    format!("{label}: {:.0}%  {db}  {g:.2}", v / 0.8 * 100.0)
}

/// "Stereo separation: Original" / "40% separated" / "40% merged" (§4). A positive stored value merges towards mono
/// (spec: audio/mixer.md "Fader matrix"); which way FL's knob turns for it is not checked.
pub fn sep_text(sep: f64) -> String {
    let pct = (sep.abs() * 100.0).round();
    if pct == 0.0 {
        "Stereo separation: Original".into()
    } else if sep > 0.0 {
        format!("Stereo separation: {pct:.0}% merged")
    } else {
        format!("Stereo separation: {pct:.0}% separated")
    }
}

/// Would a route `src` -> `dst` make a loop (dst already reaches src)?
fn makes_loop(s: &State, src: u16, dst: u16) -> bool {
    let mut seen = vec![dst];
    let mut i = 0;
    while i < seen.len() {
        let t = seen[i];
        if t == src {
            return true;
        }
        if let Some(m) = track(s, t) {
            for &r in &m.routes {
                if !seen.contains(&r) {
                    seen.push(r);
                }
            }
        }
        i += 1;
    }
    false
}

fn changed(s: &mut State) {
    s.piano.dirty = true;
}

/// Selects `n` as a click with these modifiers does (§2): plain = only it, Ctrl = add it.
pub fn select(s: &mut State, n: u16, ctrl: bool) {
    if ctrl {
        let cur = s.selected_mixer;
        if !s.mixer.sel.contains(&cur) {
            s.mixer.sel.push(cur);
        }
        if !s.mixer.sel.contains(&n) {
            s.mixer.sel.push(n);
        }
    } else {
        s.mixer.sel.clear();
    }
    s.selected_mixer = n;
}

/// Left click (or press) on a mixer control.
pub fn hit(app: &mut App, h: MHit) -> Option<String> {
    let (ctrl, shift) = (app.ctrl, app.shift);
    let at = app.mouse;
    match h {
        // A filled slot: the press starts a drag that reorders the slots; a release that did not move opens or closes
        // the plugin (`drag_end`).
        MHit::SlotName(i) if slot_name(&app.state, app.state.selected_mixer, i).is_some() => {
            app.drag = Some(Drag::MixSlot { track: app.state.selected_mixer, slot: i, y0: at.1, moved: false });
            return None;
        }
        MHit::SlotArrow(i) if slot_name(&app.state, app.state.selected_mixer, i).is_some() => {
            app.state.playlist.popup = Some(slot_popup(app, i, at.0, at.1));
            return None;
        }
        MHit::View(k) => return run(app, MCmd::View(k)),
        _ => {}
    }
    let s = &mut app.state;
    match h {
        MHit::Strip(n) => {
            select(s, n, ctrl);
            if shift {
                start_rename(s, n, at);
            }
            None
        }
        MHit::Led(n) => {
            select(s, n, ctrl);
            let m = track_mut(s, n)?;
            m.enabled = !m.enabled;
            changed(s);
            None
        }
        MHit::Polarity(n) => {
            select(s, n, ctrl);
            let m = track_mut(s, n)?;
            m.polarity = !m.polarity;
            changed(s);
            None
        }
        MHit::Swap(n) => {
            select(s, n, ctrl);
            let m = track_mut(s, n)?;
            m.swap = !m.swap;
            changed(s);
            None
        }
        MHit::Route(dst) => route(s, dst, ctrl, shift),
        MHit::Unroute(dst) => {
            let cur = s.selected_mixer;
            let m = track_mut(s, cur)?;
            m.routes.retain(|&r| r != dst);
            m.send_levels.retain(|&(d, _)| d != dst);
            changed(s);
            Some(format!("{} no longer sends to {}", name(s, cur), name(s, dst)))
        }
        MHit::Add => {
            let n = add_track(s, None);
            Some(format!("Added {}", name(s, n)))
        }
        MHit::AddAfter => {
            let n = add_track(s, Some(s.selected_mixer + 1));
            Some(format!("Added {}", name(s, n)))
        }
        MHit::Options => {
            s.playlist.popup = Some(options_menu(s, at.0, at.1 + 10.0));
            None
        }
        MHit::Layout => {
            s.playlist.popup = Some(layout_menu(s, at.0, at.1 + 10.0));
            None
        }
        // Empty slots (filled ones are handled above): the effect list.
        MHit::SlotName(i) | MHit::SlotArrow(i) => {
            s.playlist.popup = Some(effect_menu(s, i, at.0, at.1));
            None
        }
        MHit::SlotLed(i) => {
            let t = s.selected_mixer;
            let m = track_mut(s, t)?;
            let k = usize::from(i);
            m.slot_on[k] = !m.slot_on[k];
            changed(s);
            None
        }
        MHit::Scale | MHit::View(_) => None,
    }
}

/// Right click on a mixer control. Returns false when it is not handled here (knob menus).
pub fn right(app: &mut App, h: MHit) -> bool {
    let at = app.mouse;
    if let MHit::SlotArrow(i) = h {
        // The slot's plugin menu, like a left click (spec: ui/plugin-menu.md §1).
        let t = app.state.selected_mixer;
        let p = if slot_name(&app.state, t, i).is_some() { slot_popup(app, i, at.0, at.1) } else { effect_menu(&app.state, i, at.0, at.1) };
        app.state.playlist.popup = Some(p);
        return true;
    }
    let s = &mut app.state;
    match h {
        MHit::Strip(n) => {
            if !is_selected(s, n) {
                select(s, n, false);
            }
            s.playlist.popup = Some(track_menu(s, n, at.0, at.1));
        }
        MHit::Add => s.playlist.popup = Some(track_menu(s, s.selected_mixer, at.0, at.1)),
        MHit::Led(n) => solo(s, n),
        MHit::Route(dst) => {
            let items = vec![
                PItem::new("Route to this track", Cmd::Mixer(MCmd::Route { dst, only: false, side: false })),
                PItem::new("Route to this track only", Cmd::Mixer(MCmd::Route { dst, only: true, side: false })),
                PItem::sep(),
                PItem::new("Sidechain to this track", Cmd::Mixer(MCmd::Route { dst, only: false, side: true })),
                PItem::new("Sidechain to this track only", Cmd::Mixer(MCmd::Route { dst, only: true, side: true })),
            ];
            s.playlist.popup = Some(Popup { x: at.0, y: at.1, items, sub: None });
        }
        MHit::SlotName(i) => {
            let t = s.selected_mixer;
            if slot_name(s, t, i).is_some() {
                s.mixer.pending.push(FxReq::Show { track: t, slot: i });
            } else {
                s.playlist.popup = Some(effect_menu(s, i, at.0, at.1));
            }
        }
        MHit::SlotLed(i) => {
            // Solo the slot: the track's other slots are bypassed (§7).
            let t = s.selected_mixer;
            if let Some(m) = track_mut(s, t) {
                let only = (0..10).all(|k| m.slot_on[k] == (k == usize::from(i)));
                for k in 0..10 {
                    m.slot_on[k] = only || k == usize::from(i);
                }
                changed(s);
            }
        }
        MHit::Polarity(n) | MHit::Swap(n) => select(s, n, false),
        _ => return false,
    }
    true
}

/// Middle click: rename a filled slot (§7).
pub fn middle(app: &mut App, h: MHit) -> bool {
    if let MHit::SlotName(i) = h {
        let t = app.state.selected_mixer;
        if slot_name(&app.state, t, i).is_some() {
            let at = app.mouse;
            crate::playlist::start_edit(&mut app.state, EditTarget::SlotName(t, i), at);
            return true;
        }
    }
    false
}

fn start_rename(s: &mut State, n: u16, at: (f32, f32)) {
    crate::playlist::start_edit(s, EditTarget::MixerName(n), at);
}

/// Right click on a LED (§3): solo, or back to all on when it already is the only one audible.
pub fn solo(s: &mut State, n: u16) {
    let soloed = s.project.mixer.iter().all(|m| m.index == 0 || (m.index == n) == m.enabled);
    for m in s.project.mixer.iter_mut() {
        m.enabled = soloed || m.index == 0 || m.index == n;
    }
    changed(s);
}

/// The tracks reached from `from` (not counting them) by following routes downstream (what they send to, down to the
/// master) or upstream (what sends into them), directly or through other tracks.
pub fn reach(s: &State, from: &[u16], down: bool) -> BTreeSet<u16> {
    let mut out = BTreeSet::new();
    let mut todo: Vec<u16> = from.to_vec();
    while let Some(t) = todo.pop() {
        let next: Vec<u16> = if down {
            track(s, t).map(|m| m.routes.clone()).unwrap_or_default()
        } else {
            s.project.mixer.iter().filter(|m| m.routes.contains(&t)).map(|m| m.index).collect()
        };
        for r in next {
            if !from.contains(&r) && out.insert(r) {
                todo.push(r);
            }
        }
    }
    out
}

/// The tracks on the signal paths of `from`: themselves and every track [`reach`]ed both ways.
pub fn chain(s: &State, from: &[u16]) -> BTreeSet<u16> {
    let mut out: BTreeSet<u16> = from.iter().copied().collect();
    out.extend(reach(s, from, true));
    out.extend(reach(s, from, false));
    out
}

/// Alt solo (Lemon Studio choice): like solo, for the selected tracks together with everything on their signal paths
/// ([`chain`]), so a track that plays through a bus is heard; again when it already is the case: all back on.
pub fn alt_solo(s: &mut State, n: u16) {
    let keep = chain(s, &targets(s, n));
    let soloed = s.project.mixer.iter().all(|m| m.index == 0 || keep.contains(&m.index) == m.enabled);
    for m in s.project.mixer.iter_mut() {
        m.enabled = soloed || m.index == 0 || keep.contains(&m.index);
    }
    changed(s);
}

/// Gradient color (Lemon Studio choice, no dialog): the selected tracks, in track order, shade from the first one's
/// colour to the last one's (red and blue for tracks without a colour; a darker shade when both are the same).
fn gradient(s: &mut State, n: u16) -> Option<String> {
    let mut tg = targets(s, n);
    tg.sort_unstable();
    let (Some(&a), Some(&b)) = (tg.first(), tg.last()) else { return None };
    if tg.len() < 2 {
        return Some("Gradient color: select two or more tracks first (Ctrl+click)".into());
    }
    let hex = |c: u32| Rgb::new((c >> 16) as u8, (c >> 8) as u8, c as u8);
    let c0 = track(s, a).and_then(|m| m.color).unwrap_or(hex(COLORS[0].1));
    let mut c1 = track(s, b).and_then(|m| m.color).unwrap_or(hex(COLORS[5].1));
    if c1 == c0 {
        c1 = Rgb::new(c0.r / 3, c0.g / 3, c0.b / 3);
    }
    let last = (tg.len() - 1) as f32;
    for (i, &t) in tg.iter().enumerate() {
        let k = i as f32 / last;
        let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * k).round() as u8;
        if let Some(m) = track_mut(s, t) {
            m.color = Some(Rgb::new(ch(c0.r, c1.r), ch(c0.g, c1.g), ch(c0.b, c1.b)));
        }
    }
    None
}

/// The group a track is in (Lemon Studio choice): the tracks from the nearest separator at or before it up to the
/// next separator (or the last insert).
pub fn group_of(s: &State, n: u16) -> Vec<u16> {
    if n == 0 {
        return vec![0];
    }
    let first = s.mixer.seps.range(1..=n).next_back().copied().unwrap_or(1);
    let end = s.mixer.seps.range(n + 1..).next().copied().unwrap_or(inserts(s) + 1);
    (first..end).collect()
}

/// Create group (Lemon Studio choice, no dialog): the selected tracks become a group (a separator before the first and
/// after the last) in one colour: the first one's, else a random one.
fn make_group(s: &mut State, n: u16) -> Option<String> {
    let mut tg: Vec<u16> = targets(s, n).into_iter().filter(|&t| t != 0).collect();
    tg.sort_unstable();
    let (&a, &b) = (tg.first()?, tg.last()?);
    s.mixer.seps.insert(a);
    if b < inserts(s) {
        s.mixer.seps.insert(b + 1);
    }
    let c = track(s, a).and_then(|m| m.color).unwrap_or_else(|| crate::patterns::random_color(u64::from(rand_seed()) ^ u64::from(a)));
    for t in a..=b {
        if let Some(m) = track_mut(s, t) {
            m.color = Some(c);
        }
    }
    Some(format!("Grouped {} to {}", name(s, a), name(s, b)))
}

/// Routes the selected tracks to `dst` (§5): a normal route at unity, a sidechain at level 0, `only` drops their
/// other routes.
pub fn route(s: &mut State, dst: u16, only: bool, side: bool) -> Option<String> {
    let mut msg = None;
    for src in selected(s) {
        if src == dst || src == 0 {
            continue;
        }
        if makes_loop(s, src, dst) {
            msg = Some(format!("Can't route {} to {}: it would make a loop", name(s, src), name(s, dst)));
            continue;
        }
        let m = track_mut(s, src)?;
        if only {
            m.routes.clear();
            m.send_levels.clear();
        }
        if !m.routes.contains(&dst) {
            m.routes.push(dst);
            m.routes.sort_unstable();
        }
        m.send_levels.retain(|&(d, _)| d != dst);
        if side {
            m.send_levels.push((dst, 0.0));
        }
    }
    changed(s);
    msg
}

/// Appends a track (`at` = None) or inserts one before `at`, shifting the later ones; selects it. Returns its number.
pub fn add_track(s: &mut State, at: Option<u16>) -> u16 {
    let last = inserts(s) + 1;
    let n = at.unwrap_or(last).clamp(1, last);
    // The new track takes number n; n.. move one up (the hidden last track stays last).
    let len = s.project.mixer.len() as u16;
    s.project.mixer.push(MixerTrack::new(len));
    // The hidden last track moves too, keeping what it had (spec: flp/events/mixer.md "Adding tracks").
    shift_tracks(s, n, len, 1);
    let fresh = MixerTrack::new(n);
    if let Some(m) = track_mut(s, n) {
        *m = fresh;
    }
    s.mixer.sel.clear();
    s.selected_mixer = n;
    changed(s);
    n
}

/// Moves tracks `from..to` (exclusive) by `d` places (+1 = right): contents, routes into them, channel targets and
/// hosted slot instances. The track at the destination end is overwritten; callers make room first.
fn shift_tracks(s: &mut State, from: u16, to: u16, d: i32) {
    let m = |t: u16| if t >= from && t < to { (i32::from(t) + d) as u16 } else { t };
    renumber(s, &m, from, to, d);
}

/// Renumbers tracks with `m` (a permutation on the touched range).
fn renumber(s: &mut State, m: &dyn Fn(u16) -> u16, from: u16, to: u16, d: i32) {
    let old: Vec<MixerTrack> = s.project.mixer.clone();
    let _ = d;
    for t in &old {
        if t.index >= from && t.index < to {
            let n = m(t.index);
            if let Some(dst) = s.project.mixer.iter_mut().find(|x| x.index == n) {
                *dst = MixerTrack { index: n, ..t.clone() };
            }
        }
    }
    for t in s.project.mixer.iter_mut() {
        for r in t.routes.iter_mut() {
            *r = m(*r);
        }
        for (d, _) in t.send_levels.iter_mut() {
            *d = m(*d);
        }
        t.routes.retain(|&r| r != t.index);
    }
    for c in s.project.channels.iter_mut() {
        c.mixer_track = m(c.mixer_track);
    }
    s.selected_mixer = m(s.selected_mixer);
    s.mixer.sel = s.mixer.sel.iter().map(|&t| m(t)).collect();
    remap_ui(&mut s.mixer, m, None);
    let mut map = Vec::new();
    for t in &old {
        for k in 0..10u8 {
            map.push(((t.index, k), (m(t.index), k)));
        }
    }
    s.mixer.pending.push(FxReq::Remap(map));
    changed(s);
}

/// Session-only per-track state (slot names, spare states, docking, separators) follows renumbered tracks (`m`);
/// track `gone` (deleted) loses its own first.
fn remap_ui(ui: &mut MixerUi, m: &dyn Fn(u16) -> u16, gone: Option<u16>) {
    if let Some(g) = gone {
        ui.slot_names.retain(|(t, _), _| *t != g);
        ui.spare.retain(|(t, _), _| *t != g);
        ui.dock.remove(&g);
        ui.seps.remove(&g);
    }
    ui.slot_names = std::mem::take(&mut ui.slot_names).into_iter().map(|((t, k), v)| ((m(t), k), v)).collect();
    ui.spare = std::mem::take(&mut ui.spare).into_iter().map(|((t, k), v)| ((m(t), k), v)).collect();
    ui.dock = std::mem::take(&mut ui.dock).into_iter().map(|(t, d)| (m(t), d)).collect();
    ui.seps = std::mem::take(&mut ui.seps).into_iter().map(m).collect();
}

/// Swaps track `n` with its neighbour (`d` = -1 left, +1 right), §6.1 Move left / right.
fn move_track(s: &mut State, n: u16, d: i32) -> Option<String> {
    let o = i32::from(n) + d;
    if n == 0 || o < 1 || o > i32::from(inserts(s)) {
        return None;
    }
    let o = o as u16;
    let m = move |t: u16| if t == n { o } else if t == o { n } else { t };
    let (a, b) = (n.min(o), n.max(o) + 1);
    renumber(s, &m, a, b, 0);
    None
}

/// Deletes track `n`: later tracks move one left, a default track is added at the end (§6.1 Delete).
fn delete_track(s: &mut State, n: u16) -> Option<String> {
    if n == 0 || n > inserts(s) {
        return Some("The master track can't be deleted".into());
    }
    let label = name(s, n);
    let len = s.project.mixer.len() as u16;
    // Drop the track's slots and routes into it, then close the gap.
    for t in s.project.mixer.iter_mut() {
        t.routes.retain(|&r| r != n);
        t.send_levels.retain(|&(d, _)| d != n);
    }
    for c in s.project.channels.iter_mut().filter(|c| c.mixer_track == n) {
        c.mixer_track = 0;
    }
    for k in 0..10 {
        s.mixer.pending.push(FxReq::Delete { track: n, slot: k });
    }
    // The hidden last track stays last; the default track comes before it (spec: flp/events/mixer.md "The last track").
    let hidden = len - 1;
    let m = move |t: u16| if t > n && t < hidden { t - 1 } else { t };
    let old = s.project.mixer.clone();
    for t in &old {
        if t.index > n && t.index < hidden {
            if let Some(dst) = s.project.mixer.iter_mut().find(|x| x.index == t.index - 1) {
                *dst = MixerTrack { index: t.index - 1, ..t.clone() };
            }
        }
    }
    if let Some(last) = s.project.mixer.iter_mut().find(|x| x.index == hidden - 1) {
        *last = MixerTrack::new(hidden - 1);
    }
    for t in s.project.mixer.iter_mut() {
        for r in t.routes.iter_mut() {
            *r = m(*r);
        }
        for (d, _) in t.send_levels.iter_mut() {
            *d = m(*d);
        }
    }
    for c in s.project.channels.iter_mut() {
        c.mixer_track = m(c.mixer_track);
    }
    let mut map = Vec::new();
    for t in n + 1..hidden {
        for k in 0..10u8 {
            map.push(((t, k), (t - 1, k)));
        }
    }
    s.mixer.pending.push(FxReq::Remap(map));
    remap_ui(&mut s.mixer, &m, Some(n));
    s.mixer.sel.clear();
    s.selected_mixer = n.min(inserts(s)).max(1);
    changed(s);
    Some(format!("Deleted {label}"))
}

/// Back to a fresh track (name, colour, levels, slots kept out), routed to the master (§6.1 Reset).
fn reset_track(s: &mut State, n: u16) {
    if let Some(m) = track_mut(s, n) {
        let slots = m.slots.clone();
        *m = MixerTrack { slots, ..MixerTrack::new(n) };
    }
    changed(s);
}

// ---------------------------------------------------------------------------------------------------------
// Menus

const COLORS: [(&str, u32); 8] = [
    ("Red", 0xc8504a),
    ("Orange", 0xd0803c),
    ("Yellow", 0xc8b44a),
    ("Green", 0x5a9c4a),
    ("Teal", 0x3c8c88),
    ("Blue", 0x4a78b4),
    ("Purple", 0x8a5aa0),
    ("Grey", 0x7a8288),
];

/// Track menu (right click on a strip), §6.1.
pub fn track_menu(s: &State, n: u16, x: f32, y: f32) -> Popup {
    let mut items = vec![PItem::head(Box::leak(format!("Mixer track ({})", name(s, n)).into_boxed_str()))];
    items.extend(track_items(s, n));
    Popup { x, y, items, sub: None }
}

/// Counts offered by "Insert multiple..." (Lemon Studio choice: a submenu instead of a number dialog).
const INSERT_COUNTS: [u16; 7] = [2, 3, 4, 5, 8, 10, 16];

fn track_items(s: &State, n: u16) -> Vec<PItem> {
    let mx = |c: MCmd| Cmd::Mixer(c);
    let mut colors: Vec<PItem> = vec![PItem::new("Default", mx(MCmd::Color(n, None)))];
    colors.extend(COLORS.iter().map(|&(l, c)| PItem::new(l, mx(MCmd::Color(n, Some(Rgb::new((c >> 16) as u8, (c >> 8) as u8, c as u8)))))));
    // Track routing: every other track, checked where this one sends (Lemon Studio choice).
    let routes: Vec<PItem> = (0..=inserts(s))
        .filter(|&d| d != n && n != 0)
        .map(|d| PItem::new(name(s, d), mx(MCmd::ToggleRoute(n, d))).mark(Mark::Check(send(s, n, d).is_some())))
        .collect();
    let routes = if routes.is_empty() { vec![PItem::new("(the master sends nowhere)", Cmd::Nothing).off()] } else { routes };
    let dock = s.mixer.dock.get(&n).copied().unwrap_or(Dock::Middle);
    let docks = [("Left", Dock::Left), ("Middle", Dock::Middle), ("Right", Dock::Right)]
        .map(|(l, d)| PItem::new(l, mx(MCmd::Dock(n, d))).mark(Mark::Radio(dock == d)))
        .to_vec();
    let many = INSERT_COUNTS.iter().map(|&k| PItem::new(format!("{k} tracks"), mx(MCmd::InsertMany(n, k)))).collect();
    let mut states: Vec<PItem> = crate::mixer_states::names().into_iter().map(|nm| PItem::new(nm.clone(), mx(MCmd::InsertState(n, nm)))).collect();
    if states.is_empty() {
        states.push(PItem::new("(no saved states)", Cmd::Nothing).off());
    }
    vec![
        PItem::new("Rename, color and icon...", mx(MCmd::Rename(n))).key("F2"),
        PItem::sub("Change color...", colors),
        PItem::new("Random color", mx(MCmd::RandomColor(n))),
        PItem::new("Gradient color...", mx(MCmd::Gradient(n))),
        // Track icons need a place in the project model.
        PItem::not_yet("Change icon..."),
        PItem::not_yet("Assign to new Audio track(s)"),
        PItem::not_yet("Unassign from Audio track(s)"),
        PItem::sep(),
        PItem::sub("File", vec![
            PItem::new("Render to wave file...", mx(MCmd::Render(n))),
            PItem::new("Save mixer track state as...", mx(MCmd::SaveState(n))),
        ]),
        PItem::not_yet("Open audio editor").key("Ctrl+E"),
        PItem::not_yet("Open audio editor and record").key("Shift+E"),
        PItem::sep(),
        PItem::new("Reset selected track(s) to default", mx(MCmd::Reset(n))),
        PItem::head("Routing"),
        PItem::sub("Channel routing", vec![PItem::new("Route selected channels to this track", mx(MCmd::RouteChannels(n)))]),
        PItem::sub("Track routing", routes),
        PItem::sub("Select", vec![PItem::new("Select tracks routed from this one", mx(MCmd::SelectRouted(n)))]),
        PItem::new("Reset selected track(s) routing", mx(MCmd::ResetRouting(n))),
        PItem::new("Unroute tracks routed to the selected track(s)", mx(MCmd::UnrouteTo(n))),
        PItem::sep(),
        PItem::new("Solo selected track(s)", mx(MCmd::Solo(n))).key("S"),
        PItem::new("Alt solo selected track(s)", mx(MCmd::AltSolo(n))).key("Alt+S"),
        // Disk recording is not available yet.
        PItem::not_yet("Arm selected track(s)"),
        PItem::not_yet("Disarm selected track(s)"),
        PItem::sep(),
        PItem::not_yet("Compensate for master latency").mark(Mark::Check(true)),
        PItem::head("Group"),
        PItem::new("Separator", mx(MCmd::Separator(n))).mark(Mark::Check(s.mixer.seps.contains(&n))),
        PItem::new("Create group...", mx(MCmd::Group(n))),
        PItem::new("Auto color group", mx(MCmd::AutoColor(n))),
        PItem::head("Layout"),
        PItem::sub("Dock to", docks),
        PItem::new("Move left", mx(MCmd::MoveLeft(n))).key("Alt+Left"),
        PItem::new("Move right", mx(MCmd::MoveRight(n))).key("Alt+Right"),
        PItem::sep(),
        PItem::new("Delete", mx(MCmd::Delete(n))).key("Del"),
        PItem::new("Insert one", mx(MCmd::InsertOne(n))),
        PItem::sub("Insert multiple...", many),
        PItem::sub("Insert from state", states),
        PItem::head("CPU"),
        PItem::not_yet("Allow threaded processing").mark(Mark::Check(true)),
    ]
}

/// Mixer options (title-bar ▸), §6.2: its own items, then the track menu's.
pub fn options_menu(s: &State, x: f32, y: f32) -> Popup {
    let v = |k: u8, l: &'static str| PItem::new(l, Cmd::Mixer(MCmd::View(k))).mark(Mark::Check(s.mixer.view.get(k)));
    let mut items = vec![
        PItem::head("Options"),
        PItem::sub("Disk recording", vec![PItem::not_yet("Auto-create audio clips")]),
        PItem::sub("Plugin delay compensation", vec![PItem::not_yet("Automatic")]),
        PItem::not_yet("Link all parameters..."),
        PItem::sub("View", vec![v(VIEW_WAVES, "Waveforms"), v(VIEW_EXTRA, "Extra properties"), v(VIEW_INSPECTOR, "Track inspector")]),
        PItem::not_yet("Detached").mark(Mark::Check(false)),
        PItem::sep(),
        PItem::not_yet("Trim unused tracks"),
        PItem::not_yet("Switch smart disable for all plugins"),
        PItem::not_yet("Remove all (audio editor) instances"),
        PItem::head(Box::leak(format!("Mixer track ({})", name(s, s.selected_mixer)).into_boxed_str())),
    ];
    items.extend(track_items(s, s.selected_mixer));
    Popup { x, y, items, sub: None }
}

/// Layout combo menu (§6.4).
pub fn layout_menu(s: &State, x: f32, y: f32) -> Popup {
    let mut items = Vec::new();
    for (i, &l) in LAYOUTS.iter().enumerate() {
        if i == 7 {
            items.push(PItem::sep());
        }
        items.push(PItem::new(l, Cmd::Mixer(MCmd::Layout(i as u8))).mark(Mark::Radio(s.mixer.layout == i as u8)));
    }
    Popup { x, y, items, sub: None }
}

/// Empty slot: the effect list (§7.1), the plugin menu for this slot (spec: ui/plugin-menu.md).
pub fn effect_menu(s: &State, slot: u8, x: f32, y: f32) -> Popup {
    let mut items = vec![PItem::not_yet("Plugin database").key("Shift+F8"), PItem::sep()];
    items.extend(crate::plugin_menu::items(s, crate::plugin_menu::Target::Fx(Some(slot)), false));
    Popup { x, y, items, sub: None }
}

/// Filled slot ▸ (§7.2).
pub fn slot_menu(s: &State, slot: u8, x: f32, y: f32) -> Popup {
    let mx = |c: MCmd| Cmd::Mixer(c);
    let replace = crate::plugin_menu::items(s, crate::plugin_menu::Target::Fx(Some(slot)), true);
    let items = vec![
        PItem::sub("Replace", replace),
        PItem::new("Delete...", mx(MCmd::SlotDelete(slot))),
        PItem::sep(),
        PItem::not_yet("Patcherize"),
        PItem::not_yet("Patcherize all"),
        PItem::head("Presets"),
        PItem::sub("Presets", vec![PItem::not_yet("(none)")]),
        PItem::not_yet("Browse presets"),
        PItem::not_yet("Save preset as..."),
        PItem::not_yet("Add to plugin database (flag as favorite)"),
        PItem::not_yet("Display preset name in preset selector").mark(Mark::Check(true)),
        PItem::head("Spare state"),
        PItem::new("Store in spare state", mx(MCmd::SlotStoreSpare(slot))),
        PItem::new("Flip with spare state", mx(MCmd::SlotFlipSpare(slot))),
        PItem::head("Parameters"),
        PItem::not_yet("Link all parameters..."),
        PItem::new("Browse parameters", Cmd::Param(crate::params::PCmd::Browse(crate::plugins::fx_key(s.selected_mixer, slot)))),
        PItem::head("CPU"),
        PItem::not_yet("Allow threaded processing").mark(Mark::Check(true)),
        PItem::not_yet("Smart disable"),
        PItem::sep(),
        PItem::new("Move up", mx(MCmd::SlotMove(slot, -1))),
        PItem::new("Move down", mx(MCmd::SlotMove(slot, 1))),
        PItem::sep(),
        PItem::new("View editor", mx(MCmd::SlotView(slot))).mark(Mark::Check(false)),
        PItem::new("Rename, color and icon...", mx(MCmd::SlotRename(slot))).key("F2"),
        PItem::not_yet("Change color..."),
        PItem::not_yet("Random color"),
        PItem::not_yet("Change icon..."),
        PItem::sep(),
        PItem::sub("Scaling", vec![PItem::not_yet("100%")]),
        PItem::not_yet("Help"),
    ];
    Popup { x, y, items, sub: None }
}

/// A filled slot's ▸ menu with its plugin's presets (`presets`, as the plugin window has them): the preset list and
/// "Save preset as..." work once the slot's plugin is hosted.
pub fn slot_popup(app: &App, slot: u8, x: f32, y: f32) -> Popup {
    let s = &app.state;
    let mut p = slot_menu(s, slot, x, y);
    let key = crate::plugins::fx_key(s.selected_mixer, slot);
    if app.plugins.hosted.contains_key(&key) {
        for it in p.items.iter_mut() {
            match &*it.label {
                "Presets" if !it.head => {
                    *it = PItem::sub("Presets", crate::presets::items(app, key, false));
                }
                "Save preset as..." => *it = PItem::new("Save preset as...", Cmd::Preset(crate::presets::PCmd::SaveAs(key))),
                _ => {}
            }
        }
    }
    p
}

// ---------------------------------------------------------------------------------------------------------
// Commands

pub fn run(app: &mut App, c: MCmd) -> Option<String> {
    let at = app.mouse;
    let s = &mut app.state;
    let msg = match c {
        MCmd::Rename(n) => {
            start_rename(s, n, at);
            None
        }
        MCmd::Color(n, col) => {
            for t in targets(s, n) {
                if let Some(m) = track_mut(s, t) {
                    m.color = col;
                }
            }
            None
        }
        MCmd::RandomColor(n) => {
            for t in targets(s, n) {
                let c = crate::patterns::random_color(u64::from(t).wrapping_mul(0x9e37_79b9) ^ u64::from(rand_seed()));
                if let Some(m) = track_mut(s, t) {
                    m.color = Some(c);
                }
            }
            None
        }
        MCmd::Gradient(n) => gradient(s, n),
        MCmd::Reset(n) => {
            for t in targets(s, n) {
                reset_track(s, t);
            }
            None
        }
        MCmd::RouteChannels(n) => {
            let mut rows: Vec<usize> = s.rack.sel.iter().copied().filter(|&r| r < s.project.channels.len()).collect();
            if rows.is_empty() && s.selected_channel < s.project.channels.len() {
                rows.push(s.selected_channel);
            }
            for &r in &rows {
                s.project.channels[r].mixer_track = n;
            }
            changed(s);
            Some(format!("{} channel(s) routed to {}", rows.len(), name(s, n)))
        }
        MCmd::ToggleRoute(n, d) => {
            if send(s, n, d).is_some() {
                let m = track_mut(s, n)?;
                m.routes.retain(|&r| r != d);
                m.send_levels.retain(|&(x, _)| x != d);
                changed(s);
                Some(format!("{} no longer sends to {}", name(s, n), name(s, d)))
            } else if makes_loop(s, n, d) {
                Some(format!("Can't route {} to {}: it would make a loop", name(s, n), name(s, d)))
            } else {
                let m = track_mut(s, n)?;
                m.routes.push(d);
                m.routes.sort_unstable();
                changed(s);
                None
            }
        }
        MCmd::SelectRouted(n) => {
            let dst = track(s, n).map(|m| m.routes.clone()).unwrap_or_default();
            match dst.split_first() {
                Some((&first, rest)) => {
                    s.selected_mixer = first;
                    s.mixer.sel = std::iter::once(first).chain(rest.iter().copied()).collect();
                    None
                }
                None => Some(format!("{} sends to no track", name(s, n))),
            }
        }
        MCmd::AltSolo(n) => {
            alt_solo(s, n);
            None
        }
        MCmd::Separator(n) => {
            let on = !s.mixer.seps.contains(&n);
            for t in targets(s, n).into_iter().filter(|&t| t != 0) {
                if on {
                    s.mixer.seps.insert(t);
                } else {
                    s.mixer.seps.remove(&t);
                }
            }
            None
        }
        MCmd::Group(n) => make_group(s, n),
        MCmd::AutoColor(n) => {
            let g = group_of(s, n);
            let c = crate::patterns::random_color(u64::from(rand_seed()) ^ u64::from(n));
            for &t in &g {
                if let Some(m) = track_mut(s, t) {
                    m.color = Some(c);
                }
            }
            None
        }
        MCmd::Dock(n, d) => {
            for t in targets(s, n).into_iter().filter(|&t| t != 0) {
                if d == Dock::Middle {
                    s.mixer.dock.remove(&t);
                } else {
                    s.mixer.dock.insert(t, d);
                }
            }
            None
        }
        MCmd::InsertMany(n, k) => {
            let at = n.max(1);
            for _ in 0..k {
                add_track(s, Some(at));
            }
            s.selected_mixer = at;
            Some(format!("Inserted {k} tracks"))
        }
        MCmd::SaveState(n) => crate::mixer_states::save_track(app, n),
        MCmd::InsertState(n, nm) => crate::mixer_states::insert(app, n, &nm),
        MCmd::Render(n) => Some(render(app, n)),
        MCmd::ResetRouting(n) => {
            for t in targets(s, n).into_iter().filter(|&t| t != 0) {
                if let Some(m) = track_mut(s, t) {
                    m.routes = vec![0];
                    m.send_levels.clear();
                }
            }
            changed(s);
            None
        }
        MCmd::UnrouteTo(n) => {
            let tg = targets(s, n);
            for m in s.project.mixer.iter_mut() {
                m.routes.retain(|r| !tg.contains(r));
                m.send_levels.retain(|(d, _)| !tg.contains(d));
            }
            changed(s);
            None
        }
        MCmd::Solo(n) => {
            solo(s, n);
            None
        }
        MCmd::MoveLeft(n) => move_track(s, n, -1),
        MCmd::MoveRight(n) => move_track(s, n, 1),
        MCmd::Delete(n) => delete_track(s, n),
        MCmd::InsertOne(n) => {
            let n = add_track(s, Some(n.max(1)));
            Some(format!("Inserted {}", name(s, n)))
        }
        MCmd::Route { dst, only, side } => route(s, dst, only, side),
        MCmd::Layout(l) => {
            s.mixer.layout = l.min(LAYOUTS.len() as u8 - 1);
            save_prefs(&s.mixer);
            None
        }
        MCmd::View(k) => {
            s.mixer.view.toggle(k);
            save_prefs(&s.mixer);
            None
        }
        MCmd::AddEffect(slot, index) => {
            let t = s.selected_mixer;
            let Some(nm) = app.plugins.effects.get(index).map(|a| a.desc.name.clone()) else { return None };
            if let Some(m) = track_mut(s, t) {
                m.slots[usize::from(slot)] = Some(crate::plugins::new_slot(&nm));
                m.slot_on[usize::from(slot)] = true;
                m.slot_mix[usize::from(slot)] = 1.0;
            }
            s.mixer.slot_names.remove(&(t, slot));
            s.mixer.spare.remove(&(t, slot));
            s.mixer.pending.push(FxReq::Create { track: t, slot, index, state: None });
            changed(s);
            None
        }
        MCmd::SlotDelete(slot) => {
            let t = s.selected_mixer;
            if let Some(m) = track_mut(s, t) {
                m.slots[usize::from(slot)] = None;
                m.slot_on[usize::from(slot)] = true;
                m.slot_mix[usize::from(slot)] = 1.0;
            }
            s.mixer.slot_names.remove(&(t, slot));
            s.mixer.spare.remove(&(t, slot));
            s.mixer.pending.push(FxReq::Delete { track: t, slot });
            changed(s);
            None
        }
        MCmd::SlotMove(slot, d) => {
            let t = s.selected_mixer;
            let o = i32::from(slot) + i32::from(d);
            if !(0..10).contains(&o) {
                return None;
            }
            let (a, b) = (usize::from(slot), o as usize);
            if let Some(m) = track_mut(s, t) {
                m.slots.swap(a, b);
                m.slot_on.swap(a, b);
                m.slot_mix.swap(a, b);
            }
            let (na, nb) = (s.mixer.slot_names.remove(&(t, a as u8)), s.mixer.slot_names.remove(&(t, b as u8)));
            if let Some(v) = na {
                s.mixer.slot_names.insert((t, b as u8), v);
            }
            if let Some(v) = nb {
                s.mixer.slot_names.insert((t, a as u8), v);
            }
            let (sa, sb) = (s.mixer.spare.remove(&(t, a as u8)), s.mixer.spare.remove(&(t, b as u8)));
            if let Some(v) = sa {
                s.mixer.spare.insert((t, b as u8), v);
            }
            if let Some(v) = sb {
                s.mixer.spare.insert((t, a as u8), v);
            }
            s.mixer.pending.push(FxReq::Remap(vec![((t, a as u8), (t, b as u8)), ((t, b as u8), (t, a as u8))]));
            changed(s);
            None
        }
        MCmd::SlotRename(slot) => {
            let t = s.selected_mixer;
            crate::playlist::start_edit(s, EditTarget::SlotName(t, slot), at);
            None
        }
        MCmd::SlotView(slot) => {
            let t = s.selected_mixer;
            s.mixer.pending.push(FxReq::Toggle { track: t, slot });
            None
        }
        MCmd::SlotStoreSpare(slot) => {
            let t = s.selected_mixer;
            match slot_state(app, t, slot) {
                Some(st) => {
                    app.state.mixer.spare.insert((t, slot), st);
                    Some("Stored in the spare state".into())
                }
                None => Some("This slot's plugin has no state to store".into()),
            }
        }
        MCmd::SlotFlipSpare(slot) => flip_spare(app, slot),
    };
    app.invalidate();
    msg
}

/// The current state of the plugin in slot `slot` of track `t`: the hosted instance's, else what the project holds.
fn slot_state(app: &App, t: u16, slot: u8) -> Option<Vec<u8>> {
    let key = crate::plugins::fx_key(t, slot);
    if app.plugins.hosted.contains_key(&key) {
        return app.plugins.state(key);
    }
    track(&app.state, t)?.slots.get(usize::from(slot))?.as_ref().map(|p| p.state.clone()).filter(|st| !st.is_empty())
}

/// Flip with spare state: the slot's plugin takes the spare state and the spare keeps the one it had.
fn flip_spare(app: &mut App, slot: u8) -> Option<String> {
    let t = app.state.selected_mixer;
    let Some(spare) = app.state.mixer.spare.get(&(t, slot)).cloned() else { return Some("No spare state stored for this slot yet".into()) };
    let now = slot_state(app, t, slot)?;
    let key = crate::plugins::fx_key(t, slot);
    if app.plugins.hosted.contains_key(&key) {
        if !app.plugins.set_state(key, &spare) {
            return Some("The plugin did not take the spare state".into());
        }
    } else if let Some(p) = track_mut(&mut app.state, t).and_then(|m| m.slots[usize::from(slot)].as_mut()) {
        p.state = spare;
    }
    app.state.mixer.spare.insert((t, slot), now);
    changed(&mut app.state);
    Some("Flipped with the spare state".into())
}

/// File > Render to wave file (Lemon Studio choice of what is rendered): the song (or the current pattern outside
/// song mode) as track `n` plays it, offline, into "Rendered/<track> n.wav" beside the project. Tracks off its signal
/// path are muted, it sends straight to a neutral master, and the tracks sending into it send nowhere else. Hosted
/// plugins are not part of an offline render yet (as for Consolidate).
pub fn render(app: &mut App, n: u16) -> String {
    let s = &app.state;
    let mut p = crate::rack::play_project(s);
    let up = reach(s, &[n], false);
    for m in p.mixer.iter_mut() {
        if m.index == 0 {
            *m = MixerTrack { routes: Vec::new(), ..MixerTrack::new(0) };
        } else if m.index == n {
            m.routes = if n == 0 { Vec::new() } else { vec![0] };
            m.send_levels.clear();
        } else if up.contains(&m.index) {
            m.routes.retain(|r| up.contains(r) || *r == n);
            m.send_levels.retain(|(d, _)| up.contains(d) || *d == n);
        } else {
            m.enabled = false;
        }
    }
    if n == 0 {
        // The master itself: the project as it is.
        p.mixer.clone_from(&s.project.mixer);
    }
    p.main_volume = None;
    let rate = 44100;
    let mode = if s.song_mode { os_engine::PlayMode::Song } else { os_engine::PlayMode::Pattern(s.pattern) };
    let mut lib = os_engine::SampleLibrary::new(os_engine::SampleLibrary::default_factory_dir());
    let mut e = os_engine::Engine::new(&p, &mut lib, rate, mode);
    let mut audio = e.render_all(512);
    audio.resize(audio.len().max(2), 0.0);
    let dir = render_dir(s);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return format!("Render: {e}");
    }
    let label: String = name(s, n).chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    let file = (1..).map(|k| dir.join(format!("{label} {k}.wav"))).find(|f| !f.exists()).unwrap_or_else(|| dir.join(format!("{label}.wav")));
    let written = std::fs::File::create(&file).and_then(|f| {
        let mut w = std::io::BufWriter::new(f);
        os_engine::wav::write_f32_stereo(&mut w, rate, &audio)
    });
    match written {
        Ok(()) => format!("Rendered {} to {}", name(s, n), file.display()),
        Err(e) => format!("Render: {e}"),
    }
}

/// Where renders go: "Rendered" beside the project, else in the temporary folder (as Consolidate does).
fn render_dir(s: &State) -> std::path::PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("Lemon Studio test {}", std::process::id())).join("Rendered");
    }
    s.path.as_ref().and_then(|p| p.parent()).map(|d| d.join("Rendered")).unwrap_or_else(|| std::env::temp_dir().join("Lemon Studio").join("Rendered"))
}

/// The automation target, name and value of a mixer knob that `App::automation_target` leaves to the mixer: stereo
/// separation, sends, slot mix levels and the equaliser (the engine plays all of them; spec: ui/playlist.md §8,
/// audio/mixer.md "Changes while the song plays").
pub fn automation_target(s: &State, a: &Action) -> Option<(u32, String, f64)> {
    use crate::knobs::KnobId;
    use os_model::target;
    let Action::Knob(id) = *a else { return None };
    let v = f64::from(s.knob(id));
    Some(match id {
        KnobId::MixSep(t) => (target::mixer(t, target::MIXER_SEPARATION), format!("{} - Stereo separation", name(s, t)), v),
        KnobId::Send(t, d) => (target::send(t, d), format!("{} - Send to {}", name(s, t), name(s, d)), v),
        KnobId::SlotMix(t, k) => (target::slot(t, k, target::SLOT_MIX), format!("{} - Slot {} mix level", name(s, t), k + 1), v),
        KnobId::MixEq(t, b, f) => {
            let id = [target::EQ_GAIN, target::EQ_FREQ, target::EQ_WIDTH][usize::from(f.min(2))] + b.min(2);
            (target::mixer(t, id), format!("{} - EQ band {} {}", name(s, t), b + 1, EQ_FIELDS[usize::from(f.min(2))]), v)
        }
        _ => return None,
    })
}

/// Tracks a "selected track(s)" command acts on: the selection when `n` is in it, else `n` alone.
fn targets(s: &State, n: u16) -> Vec<u16> {
    if is_selected(s, n) { selected(s) } else { vec![n] }
}

fn rand_seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.subsec_nanos())
}

/// Keys while the mixer is in front (§2): F2 rename, Del delete, Alt+Left / Right move, S solo, Alt+S alt solo.
pub fn key(app: &mut App, key: &winit::keyboard::Key, alt: bool) -> bool {
    use winit::keyboard::{Key, NamedKey};
    let n = app.state.selected_mixer;
    let msg = match key {
        Key::Named(NamedKey::F2) => run(app, MCmd::Rename(n)),
        Key::Named(NamedKey::Delete) => run(app, MCmd::Delete(n)),
        Key::Named(NamedKey::ArrowLeft) if alt => run(app, MCmd::MoveLeft(n)),
        Key::Named(NamedKey::ArrowRight) if alt => run(app, MCmd::MoveRight(n)),
        Key::Named(NamedKey::ArrowLeft) => {
            if n > 0 {
                select(&mut app.state, n - 1, false);
            }
            None
        }
        Key::Named(NamedKey::ArrowRight) => {
            if n < inserts(&app.state) {
                select(&mut app.state, n + 1, false);
            }
            None
        }
        Key::Character(c) if c.eq_ignore_ascii_case("s") && !alt => run(app, MCmd::Solo(n)),
        Key::Character(c) if c.eq_ignore_ascii_case("s") => run(app, MCmd::AltSolo(n)),
        _ => return false,
    };
    if msg.is_some() {
        app.state.message = msg;
    }
    app.invalidate();
    true
}

/// New peaks from the audio thread: meters rise, fall and hold by elapsed time (`meters::step`). True while any meter
/// is still moving.
pub fn feed_meters(s: &mut State, peaks: &[[f32; 2]]) -> bool {
    let now = std::time::Instant::now();
    // Long gaps (the app was idle) count as one frame, so a meter does not vanish in a single step.
    let dt = s.mixer.meter_at.map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32().min(0.1));
    s.mixer.meter_at = Some(now);
    let m = &mut s.mixer.meters;
    if m.len() < peaks.len() {
        m.resize(peaks.len(), Default::default());
    }
    let mut moving = false;
    for (i, v) in m.iter_mut().enumerate() {
        let p = peaks.get(i).copied().unwrap_or([0.0; 2]);
        moving |= crate::meters::step(v, p, dt, &s.mixer.meter_cfg);
    }
    // The waveform view: each strip's recent peaks, newest first.
    if s.mixer.view.waveforms {
        let w = &mut s.mixer.waves;
        if w.len() < peaks.len() {
            w.resize(peaks.len(), VecDeque::new());
        }
        for (i, h) in w.iter_mut().enumerate() {
            let p = peaks.get(i).copied().unwrap_or([0.0; 2]);
            h.push_front(p[0].max(p[1]).abs());
            h.truncate(WAVE_LEN);
            moving |= h.iter().any(|&x| x > 0.0);
        }
    }
    if !moving {
        s.mixer.meter_at = None;
    }
    moving
}

impl App {
    /// Mixer slot work on the UI thread (`plugin_tick`): create / open / close / drop hosted effect instances.
    pub(crate) fn mixer_tick(&mut self) {
        let reqs = std::mem::take(&mut self.state.mixer.pending);
        if reqs.is_empty() {
            return;
        }
        let mut relink = false;
        // Creates waiting for a native plugin's first check (`Plugins::effect_ready`); a newer request for the same
        // slot replaces a waiting one.
        let mut waiting: Vec<FxReq> = Vec::new();
        let mut done = false;
        for r in reqs {
            if let FxReq::Create { track, slot, .. } | FxReq::Delete { track, slot } = r {
                waiting.retain(|w| !matches!(*w, FxReq::Create { track: t, slot: s, .. } if (t, s) == (track, slot)));
            }
            if let FxReq::Create { index, .. } = r {
                if !self.plugins.effect_ready(index) {
                    let name = self.plugins.effects.get(index).map(|a| a.desc.name.clone()).unwrap_or_default();
                    let msg = format!("Trying {name} before its first load...");
                    if self.state.message.as_deref() != Some(msg.as_str()) {
                        self.state.message = Some(msg);
                        done = true;
                    }
                    waiting.push(r);
                    continue;
                }
            }
            done = true;
            match r {
                FxReq::Create { track, slot, index, state } => match self.plugins.create_effect(track, slot, index, state.as_deref()) {
                    Ok(_) => {
                        relink = true;
                        self.open_fx(track, slot);
                    }
                    Err(e) => {
                        if let Some(m) = track_mut(&mut self.state, track) {
                            m.slots[usize::from(slot)] = None;
                        }
                        self.state.message = Some(format!("Effect: {e}"));
                    }
                },
                FxReq::Toggle { track, slot } => {
                    let key = crate::plugins::fx_key(track, slot);
                    if !self.plugins.close_if_open(key) {
                        self.open_fx(track, slot);
                    }
                }
                FxReq::Show { track, slot } => self.open_fx(track, slot),
                FxReq::Delete { track, slot } => {
                    let key = crate::plugins::fx_key(track, slot);
                    if self.plugins.hosted.contains_key(&key) {
                        self.plugins.drop_key(key);
                        relink = true;
                    }
                }
                FxReq::Remap(map) => {
                    self.plugins.remap_fx(&|t, k| Some(map.iter().find(|(a, _)| *a == (t, k)).map_or((t, k), |&(_, b)| b)));
                    relink = true;
                }
            }
        }
        waiting.append(&mut self.state.mixer.pending);
        self.state.mixer.pending = waiting;
        if relink {
            self.transport.set_effects(self.plugins.effects());
            self.state.piano.dirty = true;
        }
        if done {
            self.invalidate();
        }
    }

    pub(crate) fn open_fx(&mut self, track: u16, slot: u8) {
        let key = crate::plugins::fx_key(track, slot);
        if !self.plugins.hosted.contains_key(&key) {
            self.state.message = Some("This slot's plugin is not hosted (no Lemon Studio version of it yet)".into());
            return;
        }
        #[cfg(not(plugin_windows))]
        {
            self.state.message = Some(crate::app::off_text(crate::app::PLUGIN_WINDOWS));
        }
        #[cfg(plugin_windows)]
        {
            let info = fx_info(&self.state.project, &self.state.mixer, key);
            match info.map(|i| self.plugins.open_window(key, i)) {
                Some(Ok(())) => self.state.raise(crate::app::Win::Hosted(key)),
                Some(Err(e)) => self.state.message = Some(format!("Plugin: {e}")),
                None => {}
            }
        }
    }
}

/// The plugin frame's title for a mixer slot: "Name (Insert N)" / "(Master)" (§7).
#[cfg(plugin_windows)]
pub fn fx_info(p: &os_model::Project, ui: &MixerUi, key: usize) -> Option<crate::plugin_window::ChannelInfo> {
    let (t, k) = crate::plugins::fx_of(key)?;
    let m = p.mixer.iter().find(|m| m.index == t)?;
    let name = ui.slot_names.get(&(t, k)).cloned().or_else(|| m.slots.get(usize::from(k))?.as_ref().map(crate::project_plugins::slot_title))?;
    Some(crate::plugin_window::ChannelInfo {
        misc: None,
        name,
        target: m.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| if t == 0 { "Master".into() } else { format!("Insert {t}") }),
        enabled: m.slot_on[usize::from(k)],
        volume: 0.8,
        pan: 0.0,
        pitch_cents: 0,
        range: 2,
        track: t,
    })
}

/// Press on a fader (spec: ui/mixer.md): selects its track and starts dragging it.
pub(crate) fn press_fader(app: &mut App, t: u16) {
    let s = &mut app.state;
    select(s, t, app.ctrl);
    let v0 = s.project.mixer.iter().find(|m| m.index == t).and_then(|m| m.volume).unwrap_or(0.8);
    app.drag = Some(Drag::Fader { track: t, y0: app.mouse.1, v0 });
}

/// A mixer drag moved to (`x`, `y`) (`App::mouse_move`).
pub(crate) fn drag_move(app: &mut App, d: Drag, _x: f32, y: f32) {
    match d {
        Drag::Fader { track, y0, v0 } => {
            let per_px = views::mixer::FADER_PER_PX / f64::from(views::mixer::scale(app.state.win_rect(Win::Mixer)));
            let v = (v0 - f64::from(y - y0) * per_px).clamp(0.0, 1.0);
            if let Some(t) = app.state.project.mixer.iter_mut().find(|t| t.index == track) {
                t.volume = Some(v);
                app.state.piano.dirty = true;
            }
            app.hint = Cow::Owned(crate::mixer::level_text("Volume", v));
            let col = app.hits.iter().rev().find(|h| h.action == Action::Fader(track)).map(|h| h.rect);
            match col {
                Some(r) => app.damage_rect(r.outset(10.0)),
                None => app.invalidate(),
            }
            app.damage_rect(HINT_RECT);
        }
        Drag::MixSlot { track, slot, y0, .. } => {
            // Whole slot rows dragged over move the slot one row at a time (as Move up / Move down do).
            let pitch = views::mixer::slot_pitch(app.state.win_rect(Win::Mixer));
            let target = (i32::from(slot) + ((y - y0) / pitch).round() as i32).clamp(0, 9);
            let moved = (y - y0).abs() > 3.0;
            let (mut cur, mut y0) = (slot, y0);
            if track == app.state.selected_mixer {
                while i32::from(cur) != target {
                    let d: i8 = if i32::from(cur) < target { 1 } else { -1 };
                    run(app, MCmd::SlotMove(cur, d));
                    cur = (i32::from(cur) + i32::from(d)) as u8;
                    y0 += pitch * f32::from(d);
                }
            }
            if let Some(Drag::MixSlot { slot, y0: y, moved: m, .. }) = app.drag.as_mut() {
                (*slot, *y, *m) = (cur, y0, *m || moved);
            }
            app.hint = Cow::Owned(format!("Move to slot {}", cur + 1));
            app.damage_rect(HINT_RECT);
        }
        _ => {}
    }
}

/// The end of a mixer drag (`App::mouse_up`): a slot pressed and released without moving opens or closes its plugin.
pub(crate) fn drag_end(app: &mut App, d: Drag) {
    if let Drag::MixSlot { track, slot, moved: false, .. } = d {
        app.state.mixer.pending.push(FxReq::Toggle { track, slot });
    }
}

// ---------------------------------------------------------------------------------------------------------
// Track equaliser (spec: audio/mixer.md "Equaliser"): the model keeps the stored values; the knobs show them 0..1 as
// automation clips send them (gain (2 v - 1) 1800, frequency and width 65536 v).

/// Band names and setting names, for hints and automation clips.
pub const EQ_BANDS: [&str; 3] = ["low shelf", "peaking", "high shelf"];
pub const EQ_FIELDS: [&str; 3] = ["gain", "frequency", "width"];

/// Knob value (0..1) of stored setting `v` of field `f` (0 gain, 1 frequency, 2 width).
pub fn eq_norm(f: u8, v: i32) -> f32 {
    if f == 0 { (0.5 + v as f32 / 3600.0).clamp(0.0, 1.0) } else { (v as f32 / 65536.0).clamp(0.0, 1.0) }
}

/// The stored setting of field `f` at knob value `v`.
pub fn eq_stored(f: u8, v: f32) -> i32 {
    let v = f64::from(v.clamp(0.0, 1.0));
    if f == 0 { ((2.0 * v - 1.0) * 1800.0).round() as i32 } else { (65536.0 * v).round() as i32 }
}

/// Knob value of band `b`'s field `f` on track `t`.
pub fn eq_value(s: &State, t: u16, b: u8, f: u8) -> f32 {
    let (b, f) = (b.min(2), f.min(2));
    let v = track(s, t).map_or(os_model::EQ_DEFAULT[usize::from(b)][usize::from(f)], |m| m.eq[usize::from(b)][usize::from(f)]);
    eq_norm(f, v)
}

pub fn set_eq_value(s: &mut State, t: u16, b: u8, f: u8, v: f32) {
    let (b, f) = (b.min(2), f.min(2));
    if let Some(m) = track_mut(s, t) {
        m.eq[usize::from(b)][usize::from(f)] = eq_stored(f, v);
        changed(s);
    }
}

/// The value a middle click restores: the new-track default.
pub fn eq_default_value(b: u8, f: u8) -> f32 {
    eq_norm(f.min(2), os_model::EQ_DEFAULT[usize::from(b.min(2))][usize::from(f.min(2))])
}

/// A band's centre (shelf corner) frequency in Hz at stored frequency `v` (10 Hz .. 16 kHz).
pub fn eq_hz(v: i32) -> f64 {
    let x = f64::from(v as f32 / 65536.0);
    ((x * 101f64.ln()).exp() - 1.0) * 159.9 + 10.0
}

/// "1.25 kHz", "440 Hz".
pub fn hz_text(hz: f64) -> String {
    if hz >= 1000.0 { format!("{:.2} kHz", hz / 1000.0) } else { format!("{hz:.0} Hz") }
}

/// The hint of an equaliser knob: "EQ band 2 (peaking) gain: +3.0 dB".
pub fn eq_text(s: &State, t: u16, b: u8, f: u8) -> String {
    let (b, f) = (b.min(2), f.min(2));
    let v = track(s, t).map_or(os_model::EQ_DEFAULT[usize::from(b)][usize::from(f)], |m| m.eq[usize::from(b)][usize::from(f)]);
    let shown = match f {
        0 => format!("{:+.1} dB", f64::from(v) / 100.0),
        1 => hz_text(eq_hz(v)),
        _ => format!("{:.0}%", f64::from(v) / 65536.0 * 100.0),
    };
    format!("EQ band {} ({}) {}: {shown}", b + 1, EQ_BANDS[usize::from(b)], EQ_FIELDS[usize::from(f)])
}

/// Biquad coefficients `[b0, b1, b2, a1, a2]` of band `k` at `rate`, worked out as the engine's equaliser does
/// (os-dsp `TrackEq`), for the response graph.
fn eq_coef(k: usize, [gain, freq, width]: [i32; 3], rate: f64) -> [f64; 5] {
    let f = eq_hz(freq).min(rate * 0.4999);
    let q = f64::from(width as f32 / 65536.0);
    let w = f * 2.0 * std::f64::consts::PI / rate;
    let (sn, cs) = (w.sin(), w.cos());
    let a = 10f64.powf(f64::from(gain) * 0.01 / 40.0);
    if k == 1 {
        let qq = ((q * 1001f64.ln()).exp() - 1.0) * 0.0999 + 0.1;
        let alpha = sn / (2.0 * qq);
        let n = 1.0 / (alpha / a + 1.0);
        return [n * (1.0 + alpha * a), -2.0 * n * cs, n * (1.0 - alpha * a), -2.0 * n * cs, n * (1.0 - alpha / a)];
    }
    let sl = q * 1.9 + 0.1;
    let beta = ((a * a + 1.0) / sl - (a - 1.0) * (a - 1.0)).sqrt() * sn;
    let d = (a - 1.0) * cs;
    if k == 0 {
        let n = 1.0 / (a + 1.0 + d + beta);
        [n * a * ((a + 1.0) - d + beta), 2.0 * n * a * ((a - 1.0) - (a + 1.0) * cs), n * a * ((a + 1.0) - d - beta), -2.0 * n * ((a - 1.0) + (a + 1.0) * cs), n * ((a + 1.0 + d) - beta)]
    } else {
        let n = 1.0 / ((a + 1.0) - d + beta);
        [n * a * (a + 1.0 + d + beta), -2.0 * n * a * ((a - 1.0) + (a + 1.0) * cs), n * a * ((a + 1.0 + d) - beta), 2.0 * n * ((a - 1.0) - (a + 1.0) * cs), n * (((a + 1.0) - d) - beta)]
    }
}

/// The equaliser's gain in dB at `hz` (bands at gain 0 do nothing), at 44.1 kHz.
pub fn eq_response(eq: &[[i32; 3]; 3], hz: f64) -> f64 {
    let rate = 44100.0;
    let w = 2.0 * std::f64::consts::PI * hz.min(rate * 0.4999) / rate;
    let (c1, s1, c2, s2) = (w.cos(), -w.sin(), (2.0 * w).cos(), -(2.0 * w).sin());
    let mut db = 0.0;
    for (k, band) in eq.iter().enumerate().filter(|(_, b)| b[0] != 0) {
        let [b0, b1, b2, a1, a2] = eq_coef(k, *band, rate);
        let num = (b0 + b1 * c1 + b2 * c2).hypot(b1 * s1 + b2 * s2);
        let den = (1.0 + a1 * c1 + a2 * c2).hypot(a1 * s1 + a2 * s2);
        db += 20.0 * (num / den).log10();
    }
    db
}

/// The docked order of the inserts (track menu > Dock to): the left-docked ones, then the middle ones (both
/// scrolling), and the right-docked ones, which stay at the right edge of the strip area.
pub fn strip_order(s: &State) -> (Vec<u16>, Vec<u16>) {
    let dock = |t: u16| s.mixer.dock.get(&t).copied().unwrap_or(Dock::Middle);
    let all = 1..=inserts(s);
    let mut scroll: Vec<u16> = all.clone().filter(|&t| dock(t) == Dock::Left).collect();
    scroll.extend(all.clone().filter(|&t| dock(t) == Dock::Middle));
    (scroll, all.filter(|&t| dock(t) == Dock::Right).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new(None);
        app.state.project = crate::app::test_project();
        app
    }

    #[test]
    fn select_mute_solo() {
        let mut a = app();
        let n = inserts(&a.state);
        assert_eq!(n, 16, "default mixer shows Insert 1-16");
        hit(&mut a, MHit::Strip(3));
        assert_eq!(selected(&a.state), vec![3]);
        a.ctrl = true;
        hit(&mut a, MHit::Strip(5));
        a.ctrl = false;
        assert_eq!(selected(&a.state), vec![5, 3]);
        hit(&mut a, MHit::Strip(2));
        assert_eq!(selected(&a.state), vec![2]);
        hit(&mut a, MHit::Led(4));
        assert!(!track(&a.state, 4).unwrap().enabled);
        hit(&mut a, MHit::Led(4));
        assert!(track(&a.state, 4).unwrap().enabled);
        right(&mut a, MHit::Led(6));
        assert!(a.state.project.mixer.iter().all(|m| m.enabled == (m.index == 0 || m.index == 6)), "solo");
        right(&mut a, MHit::Led(6));
        assert!(a.state.project.mixer.iter().all(|m| m.enabled), "unsolo");
        a.shift = true;
        hit(&mut a, MHit::Strip(7));
        a.shift = false;
        assert_eq!(a.state.playlist.edit.as_ref().map(|e| e.target), Some(EditTarget::MixerName(7)));
        a.state.playlist.edit.as_mut().unwrap().text = "Drums".into();
        crate::playlist::commit_edit(&mut a.state);
        assert_eq!(name(&a.state, 7), "Drums");
    }

    #[test]
    fn routing() {
        let mut a = app();
        select(&mut a.state, 1, false);
        hit(&mut a, MHit::Route(5));
        assert_eq!(track(&a.state, 1).unwrap().routes, vec![0, 5]);
        assert_eq!(send(&a.state, 1, 5), Some(0.8));
        a.ctrl = true;
        hit(&mut a, MHit::Route(6));
        a.ctrl = false;
        assert_eq!(track(&a.state, 1).unwrap().routes, vec![6], "route only");
        a.shift = true;
        hit(&mut a, MHit::Route(7));
        a.shift = false;
        assert_eq!(send(&a.state, 1, 7), Some(0.0), "sidechain");
        hit(&mut a, MHit::Unroute(7));
        assert_eq!(send(&a.state, 1, 7), None);
        assert!(receives(&a.state, 6));
        // 6 -> 1 would loop.
        select(&mut a.state, 6, false);
        assert!(hit(&mut a, MHit::Route(1)).is_some_and(|m| m.contains("loop")));
        assert!(!track(&a.state, 6).unwrap().routes.contains(&1));
        assert_eq!(level_text("Volume", 0.8), "Volume: 100%  0.0dB  1.00");
        assert_eq!(level_text("To Insert 11", 0.0), "To Insert 11: 0%  -INFdB  0.00");
    }

    #[test]
    fn add_move_delete_tracks() {
        let mut a = app();
        let len = a.state.project.mixer.len();
        hit(&mut a, MHit::Add);
        assert_eq!(a.state.project.mixer.len(), len + 1);
        assert_eq!(a.state.selected_mixer, 17);
        track_mut(&mut a.state, 2).unwrap().name = Some("Two".into());
        a.state.project.channels[0].mixer_track = 2;
        run(&mut a, MCmd::MoveRight(2));
        assert_eq!(name(&a.state, 3), "Two");
        assert_eq!(a.state.project.channels[0].mixer_track, 3, "channels follow their track");
        run(&mut a, MCmd::Delete(1));
        assert_eq!(name(&a.state, 2), "Two", "later tracks move left");
        assert_eq!(a.state.project.channels[0].mixer_track, 2);
        assert_eq!(a.state.project.mixer.len(), len + 1, "a fresh track fills the end");
        run(&mut a, MCmd::InsertOne(2));
        assert_eq!(name(&a.state, 3), "Two");
        assert!(a.state.project.mixer.iter().enumerate().all(|(i, m)| usize::from(m.index) == i), "indices stay in order");
    }

    #[test]
    fn slot_commands_without_plugins() {
        let mut a = app();
        a.plugins.effects = vec![crate::plugin_db::fake("Test FX", os_host::Kind::Effect)];
        select(&mut a.state, 2, false);
        run(&mut a, MCmd::AddEffect(0, 0));
        assert_eq!(slot_name(&a.state, 2, 0).as_deref(), Some("Test FX"));
        run(&mut a, MCmd::SlotMove(0, 1));
        assert_eq!(slot_name(&a.state, 2, 1).as_deref(), Some("Test FX"));
        hit(&mut a, MHit::SlotLed(1));
        assert!(!track(&a.state, 2).unwrap().slot_on[1]);
        run(&mut a, MCmd::SlotDelete(1));
        assert_eq!(slot_name(&a.state, 2, 1), None);
        assert!(a.state.mixer.pending.iter().any(|r| matches!(r, FxReq::Create { track: 2, slot: 0, index: 0, .. })));
    }

    /// The app painted with the mixer in front (None without a canvas).
    fn painted(a: &mut App) -> Option<()> {
        let mut c = os_ui::Canvas::new(1920, 1080, 1.0).ok()?;
        a.state.raise(Win::Mixer);
        a.paint(&mut c, None);
        Some(())
    }

    fn hit_of(a: &App, act: &Action) -> Option<os_ui::Rect> {
        a.hits.iter().rev().find(|h| &h.action == act).map(|h| h.rect)
    }

    fn placeholder(a: &App, name: &str) -> bool {
        a.hits.iter().any(|h| h.action == Action::Placeholder(Box::leak(name.to_string().into_boxed_str())))
    }

    fn item<'a>(items: &'a [PItem], label: &str) -> &'a PItem {
        items.iter().find(|i| i.label == label).unwrap_or_else(|| panic!("no {label}"))
    }

    /// The track equaliser: knobs of the model's `eq` (the stored values the engine plays), with hints, defaults and
    /// automation targets the engine follows; the editor is no longer greyed out.
    #[test]
    fn equaliser_knobs_edit_the_model_and_are_automatable() {
        use crate::knobs::KnobId;
        use os_model::target;
        let mut a = app();
        let id = KnobId::MixEq(3, 1, 0);
        assert_eq!(a.state.knob(id), 0.5, "flat");
        a.state.set_knob(id, 0.75);
        assert_eq!(track(&a.state, 3).unwrap().eq[1][0], 900);
        assert_eq!(a.state.knob_text(id), "EQ band 2 (peaking) gain: +9.0 dB");
        assert_eq!(crate::rack::play_project(&a.state).mixer[3].eq[1][0], 900, "playback gets it");
        a.state.set_knob(KnobId::MixEq(3, 0, 1), 0.5);
        assert_eq!(track(&a.state, 3).unwrap().eq[0][1], 32768);
        assert!(a.state.knob_text(KnobId::MixEq(3, 0, 1)).starts_with("EQ band 1 (low shelf) frequency: "));
        assert_eq!(a.state.knob_text(KnobId::MixEq(3, 2, 2)), "EQ band 3 (high shelf) width: 27%");
        assert_eq!(KnobId::MixEq(3, 2, 1).default_value(), eq_norm(1, os_model::EQ_DEFAULT[2][1]));
        for v in [-1800, -1, 0, 37, 1800] {
            assert_eq!(eq_stored(0, eq_norm(0, v)), v, "gain round trip");
        }
        // Automation: target ids the engine decodes as the same setting, at the knob's value.
        let (tgt, name, v) = a.automation_target(&Action::Knob(id)).unwrap();
        assert_eq!(target::decode(tgt), target::Target::MixerEq(3, 1, 0));
        assert_eq!((name.as_str(), v), ("Insert 3 - EQ band 2 gain", 0.75));
        let (tgt, ..) = a.automation_target(&Action::Knob(KnobId::MixEq(0, 2, 2))).unwrap();
        assert_eq!(target::decode(tgt), target::Target::MixerEq(0, 2, 2));
        let (tgt, ..) = a.automation_target(&Action::Knob(KnobId::MixSep(4))).unwrap();
        assert_eq!(target::decode(tgt), target::Target::MixerSeparation(4));
        let (tgt, ..) = a.automation_target(&Action::Knob(KnobId::SlotMix(2, 5))).unwrap();
        assert_eq!(target::decode(tgt), target::Target::SlotMix(2, 5));
        // The response graph: flat is 0 dB; a +12 dB peaking band gives about +12 dB at its frequency.
        let mut e = os_model::EQ_DEFAULT;
        assert_eq!(eq_response(&e, 1000.0), 0.0);
        e[1][0] = 1200;
        assert!((eq_response(&e, eq_hz(e[1][1])) - 12.0).abs() < 0.1);
        e[0][0] = -600;
        assert!((eq_response(&e, 20.0) + 6.0).abs() < 0.5, "{}", eq_response(&e, 20.0));

        // On screen: the band faders and knobs are knobs, with "Create automation clip" on a right click.
        select(&mut a.state, 3, false);
        if painted(&mut a).is_none() {
            return;
        }
        assert!(!placeholder(&a, "Track equalizer") && !placeholder(&a, "EQ band 2 (peaking) level"));
        for f in 0..3 {
            assert!(hit_of(&a, &Action::Knob(KnobId::MixEq(3, 1, f))).is_some(), "field {f}");
        }
        let r = hit_of(&a, &Action::Knob(KnobId::MixEq(3, 0, 0))).unwrap();
        // Dragging the gain fader up raises the band.
        a.mouse_move(r.cx(), r.cy());
        a.mouse_down();
        a.mouse_move(r.cx(), r.cy() - 30.0);
        a.mouse_up();
        assert!(track(&a.state, 3).unwrap().eq[0][0] > 0);
        a.mouse_move(r.cx(), r.cy());
        a.mouse_down_right();
        let menu = a.state.playlist.popup.take().expect("knob menu");
        let it = item(&menu.items, "Create automation clip");
        assert!(it.enabled);
        let before = a.state.project.channels.len();
        crate::playlist::run(&mut a, it.cmd.clone());
        assert_eq!(a.state.project.channels.len(), before + 1, "an automation clip channel");
    }

    /// The layout combo (every layout works), the view toggles and the track inspector.
    #[test]
    fn layouts_and_views() {
        let mut a = app();
        let menu = layout_menu(&a.state, 0.0, 0.0);
        assert!(menu.items.iter().filter(|i| !i.sep).all(|i| i.enabled));
        run(&mut a, MCmd::Layout(0));
        assert_eq!(a.state.mixer.layout, 0);
        assert_eq!(strip_w(0), 34.0);
        let view = options_menu(&a.state, 0.0, 0.0);
        let v = item(&view.items, "View").children.clone();
        assert!(v.iter().all(|i| i.enabled) && v.len() == 3);
        assert_eq!(item(&v, "Extra properties").mark, Mark::Check(true));
        hit(&mut a, MHit::View(VIEW_WAVES));
        hit(&mut a, MHit::View(VIEW_INSPECTOR));
        run(&mut a, MCmd::View(VIEW_EXTRA));
        assert_eq!(a.state.mixer.view, View { waveforms: true, extra: false, inspector: true });
        assert_eq!(parse_prefs(&prefs_text(2, &a.state.mixer.view)), (2, a.state.mixer.view));
        assert_eq!(parse_prefs("layout=9\njunk"), (WIDE, View::default()));
        // Peaks feed the waveform history.
        feed_meters(&mut a.state, &[[0.5, 0.25]; 4]);
        assert_eq!(a.state.mixer.waves[2].front(), Some(&0.5));
        if painted(&mut a).is_none() {
            return;
        }
        for name in ["View waveforms", "Extra volume / stereo properties", "Track inspector"] {
            assert!(!placeholder(&a, name), "{name}");
        }
        assert!(placeholder(&a, "Multi-touch control"));
        assert!(a.hits.iter().any(|h| h.action == Action::Swallow && h.hint == "Track inspector"));
        assert!(hit_of(&a, &Action::Mix(MHit::Polarity(2))).is_none(), "extra properties hidden");
        let (r1, r2) = (hit_of(&a, &Action::Mix(MHit::Strip(1))).unwrap(), hit_of(&a, &Action::Mix(MHit::Strip(2))).unwrap());
        assert_eq!(r2.x - r1.x, 34.0, "compact strips");
        // An "alt" layout puts the slot panel on the left.
        run(&mut a, MCmd::Layout(6));
        painted(&mut a);
        let slot = hit_of(&a, &Action::Mix(MHit::SlotName(0))).unwrap();
        assert!(slot.x < hit_of(&a, &Action::Mix(MHit::Strip(0))).unwrap().x);
    }

    /// Dock to left / right, separators and groups, gradient colours.
    #[test]
    fn docking_groups_and_colours() {
        let mut a = app();
        run(&mut a, MCmd::Dock(5, Dock::Right));
        run(&mut a, MCmd::Dock(9, Dock::Left));
        let (scroll, right) = strip_order(&a.state);
        assert_eq!((scroll[0], scroll[1], right.clone()), (9, 1, vec![5]));
        let dock = item(&track_items(&a.state, 5), "Dock to").children.clone();
        assert_eq!(item(&dock, "Right").mark, Mark::Radio(true));
        // Docking follows a moved track.
        run(&mut a, MCmd::MoveLeft(5));
        assert_eq!(a.state.mixer.dock.get(&4), Some(&Dock::Right));
        run(&mut a, MCmd::Dock(4, Dock::Middle));
        assert!(a.state.mixer.dock.get(&4).is_none());

        run(&mut a, MCmd::Separator(3));
        assert!(a.state.mixer.seps.contains(&3));
        assert_eq!(item(&track_items(&a.state, 3), "Separator").mark, Mark::Check(true));
        select(&mut a.state, 6, false);
        a.ctrl = true;
        hit(&mut a, MHit::Strip(8));
        a.ctrl = false;
        assert!(run(&mut a, MCmd::Group(6)).is_some_and(|m| m.contains("Insert 6")));
        assert!(a.state.mixer.seps.contains(&6) && a.state.mixer.seps.contains(&9));
        assert_eq!(group_of(&a.state, 7), vec![6, 7, 8]);
        assert_eq!(group_of(&a.state, 4), vec![3, 4, 5]);
        let c = track(&a.state, 6).unwrap().color;
        assert!(c.is_some() && (6..=8).all(|t| track(&a.state, t).unwrap().color == c));
        run(&mut a, MCmd::AutoColor(4));
        let c = track(&a.state, 3).unwrap().color;
        assert!(c.is_some() && track(&a.state, 5).unwrap().color == c && track(&a.state, 6).unwrap().color != c);

        // Gradient: first to last selected.
        select(&mut a.state, 1, false);
        assert!(run(&mut a, MCmd::Gradient(1)).is_some(), "needs two tracks");
        a.ctrl = true;
        hit(&mut a, MHit::Strip(3));
        hit(&mut a, MHit::Strip(2));
        a.ctrl = false;
        track_mut(&mut a.state, 1).unwrap().color = Some(Rgb::new(0, 0, 0));
        track_mut(&mut a.state, 3).unwrap().color = Some(Rgb::new(200, 100, 0));
        run(&mut a, MCmd::Gradient(3));
        assert_eq!(track(&a.state, 2).unwrap().color, Some(Rgb::new(100, 50, 0)));
        for l in ["Gradient color...", "Separator", "Create group...", "Auto color group", "Alt solo selected track(s)"] {
            assert!(item(&track_items(&a.state, 2), l).enabled, "{l}");
        }
    }

    /// Insert multiple, alt solo (menu and Alt+S), channel and track routing, select routed tracks.
    #[test]
    fn insert_many_alt_solo_and_routing_items() {
        let mut a = app();
        let len = a.state.project.mixer.len();
        let many = item(&track_items(&a.state, 4), "Insert multiple...").children.clone();
        crate::playlist::run(&mut a, item(&many, "3 tracks").cmd.clone());
        assert_eq!(a.state.project.mixer.len(), len + 3);
        assert_eq!(a.state.selected_mixer, 4);

        // 2 -> 5 -> master, 7 -> 5: alt solo of 2 keeps 5 (where it plays through), 7 is muted.
        select(&mut a.state, 2, false);
        run(&mut a, MCmd::Route { dst: 5, only: true, side: false });
        select(&mut a.state, 7, false);
        run(&mut a, MCmd::Route { dst: 5, only: true, side: false });
        select(&mut a.state, 2, false);
        assert!(crate::mixer::key(&mut a, &winit::keyboard::Key::Character("s".into()), true));
        let on = |a: &App, t: u16| track(&a.state, t).unwrap().enabled;
        assert!(on(&a, 2) && on(&a, 5) && on(&a, 0) && !on(&a, 7) && !on(&a, 3));
        run(&mut a, MCmd::AltSolo(2));
        assert!(a.state.project.mixer.iter().all(|m| m.enabled), "again: all back on");

        // Route selected channels to this track.
        a.state.rack.sel = [0usize].into_iter().collect();
        run(&mut a, MCmd::RouteChannels(9));
        assert_eq!(a.state.project.channels[0].mixer_track, 9);
        // Track routing: a check per destination.
        let routes = item(&track_items(&a.state, 2), "Track routing").children.clone();
        assert_eq!(item(&routes, "Insert 5").mark, Mark::Check(true));
        assert_eq!(item(&routes, "Master").mark, Mark::Check(false));
        run(&mut a, MCmd::ToggleRoute(2, 0));
        assert_eq!(track(&a.state, 2).unwrap().routes, vec![0, 5]);
        run(&mut a, MCmd::ToggleRoute(2, 5));
        assert_eq!(track(&a.state, 2).unwrap().routes, vec![0]);
        assert!(run(&mut a, MCmd::ToggleRoute(5, 7)).is_some_and(|m| m.contains("loop")), "7 sends into 5");
        // Select tracks routed from this one.
        run(&mut a, MCmd::ToggleRoute(2, 8));
        run(&mut a, MCmd::SelectRouted(2));
        assert_eq!(selected(&a.state), vec![0, 8]);
    }

    /// Render to wave file writes what the track plays.
    #[test]
    fn render_track_to_wave_file() {
        let mut a = app();
        let msg = run(&mut a, MCmd::Render(1)).unwrap();
        assert!(msg.starts_with("Rendered Insert 1 to "), "{msg}");
        let path = std::path::PathBuf::from(msg.trim_start_matches("Rendered Insert 1 to "));
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        let _ = std::fs::remove_file(path);
    }

    /// Spare state (store / flip) and the slot drag that reorders slots; a click without a drag opens the plugin.
    #[test]
    fn spare_state_and_slot_drag() {
        let mut a = app();
        select(&mut a.state, 2, false);
        track_mut(&mut a.state, 2).unwrap().slots[0] = Some(os_model::SlotPlugin { name: "Test FX".into(), state: vec![1, 2] });
        let menu = slot_menu(&a.state, 0, 0.0, 0.0);
        assert!(item(&menu.items, "Store in spare state").enabled && item(&menu.items, "Flip with spare state").enabled);
        assert!(run(&mut a, MCmd::SlotFlipSpare(0)).is_some_and(|m| m.contains("No spare")));
        run(&mut a, MCmd::SlotStoreSpare(0));
        track_mut(&mut a.state, 2).unwrap().slots[0].as_mut().unwrap().state = vec![3];
        run(&mut a, MCmd::SlotFlipSpare(0));
        assert_eq!(track(&a.state, 2).unwrap().slots[0].as_ref().unwrap().state, vec![1, 2]);
        assert_eq!(a.state.mixer.spare.get(&(2, 0)), Some(&vec![3]));

        // Press, drag down two rows, release: the slot moves to slot 3 (its spare state with it).
        let pitch = views::mixer::slot_pitch(a.state.win_rect(Win::Mixer));
        a.mouse = (1800.0, 300.0);
        hit(&mut a, MHit::SlotName(0));
        assert!(matches!(a.drag, Some(Drag::MixSlot { slot: 0, .. })));
        a.mouse_move(1800.0, 300.0 + 2.0 * pitch + 2.0);
        a.mouse_up();
        assert!(a.drag.is_none());
        assert_eq!(slot_name(&a.state, 2, 2).as_deref(), Some("Test FX"));
        assert_eq!(slot_name(&a.state, 2, 0), None);
        assert_eq!(a.state.mixer.spare.get(&(2, 2)), Some(&vec![3]));
        assert!(!a.state.mixer.pending.iter().any(|r| matches!(r, FxReq::Toggle { .. })), "a drag does not open it");
        // A click opens / closes it.
        hit(&mut a, MHit::SlotName(2));
        a.mouse_up();
        assert!(a.state.mixer.pending.iter().any(|r| matches!(r, FxReq::Toggle { track: 2, slot: 2 })));
    }
}
