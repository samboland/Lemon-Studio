//! Channel rack behaviour: the selection set, channel commands (fill, rotate, copy / paste, clone, delete, move,
//! sort, filter groups, colours, mute / solo / lock, zip, free mixer tracks), the rack's popup menus, its keyboard,
//! the graph editor values and the per-channel loop lengths.
// spec: ui/channel-rack.md

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use os_model::{target, ChannelKind, ClipSource, Note, Project, Rgb};

use crate::app::{Action, App, Drag, State, Win, HINT_RECT};
use crate::knobs::KnobId;
use crate::playlist::{Cmd, Mark, PItem, Popup};
use crate::transport::State as TransportState;
use crate::views;

/// Name button width at 100 % (§17): wide enough for a new project's "Lemon Sampler" at every UI scale with every text
/// engine; Linux's Barlow is the widest (Lemon Studio).
pub const NAME_W: f32 = 94.0;
/// FL's name button width at 100 % (§17): the measured columns right of the name button assume it.
pub const FL_NAME_W: f32 = 82.0;
/// Width of the mixer-box column (§17: Alt+M moves everything right of it ≈ 41 px).
pub const MIX_W: f32 = 41.0;
/// Extra width of loop mode (§15).
pub const LOOP_W: f32 = 40.0;
/// Row pitch of a zipped channel (§17).
pub const ZIP_H: f32 = 12.0;
/// Pixels of vertical drag per step on the pattern length box and the loop boxes (§7, §15).
pub const LEN_DRAG: f32 = 4.0;
/// Pixels of vertical drag per insert on the mixer box (§12).
pub const MIXER_DRAG: f32 = 10.0;
/// Longest pattern the length box sets (32 bars, §7).
pub const MAX_STEPS: u32 = 512;
/// Keys the graph editor's Note tab spans at first (two octaves around C5, a Lemon Studio choice).
pub const KEY_LO: u16 = 48;
pub const KEY_HI: u16 = 72;
/// Keys the Note tab spans (two octaves); its octave buttons move the span over the whole keyboard (Lemon Studio
/// choice: the graph editor's Note tab is the rack's keyboard editor).
pub const KEY_SPAN: u16 = KEY_HI - KEY_LO;
/// Highest key a note holds (C10 + 11, as the piano roll).
pub const KEY_MAX: u16 = 131;

/// Options menu view options that work (§17): ticked = on, kept in `RackUi::opts`.
pub const OPT_COMPLETE: &str = "Show complete piano roll preview";
pub const OPT_PREVIEW_MUTE: &str = "Show a mute button on the piano roll preview";
pub const OPT_FOCUS: &str = "Focus selected channel on playback";
pub const OPT_MUTE_REMOVED: &str = "Mute removed steps";
pub const OPT_AUTO_FILTER: &str = "Auto switch display filter";

/// Display filter (§21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    Group(usize),
    Unsorted,
}

/// Options > Auto resize (§17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoResize {
    Off,
    #[default]
    Min,
    Max,
}

/// Graph editor tabs (§14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    Note,
    #[default]
    Vel,
    Rel,
    Fine,
    Pan,
    X,
    Y,
    Shift,
    Rep,
}

impl Tab {
    pub const ALL: [Tab; 9] = [Tab::Note, Tab::Vel, Tab::Rel, Tab::Fine, Tab::Pan, Tab::X, Tab::Y, Tab::Shift, Tab::Rep];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Note => "Note",
            Tab::Vel => "Vel",
            Tab::Rel => "Rel",
            Tab::Fine => "Fine",
            Tab::Pan => "Pan",
            Tab::X => "X",
            Tab::Y => "Y",
            Tab::Shift => "Shift",
            Tab::Rep => "Rep",
        }
    }

    /// Bar / label colour (§14, approximate).
    pub fn color(self) -> u32 {
        match self {
            Tab::Note => 0x5fd07a,
            Tab::Vel => 0x40c0e0,
            Tab::Rel => 0x30c070,
            Tab::Fine => 0xe05050,
            Tab::Pan => 0xb070e0,
            Tab::X => 0x70d060,
            Tab::Y => 0xe0d050,
            Tab::Shift => 0xf06040,
            Tab::Rep => 0xf0a040,
        }
    }

    /// Drawn as a line from the centre instead of a bar from the bottom.
    pub fn centred(self) -> bool {
        matches!(self, Tab::Fine | Tab::Pan)
    }

    /// The note's value as 0..1 (the Note tab over its first span).
    pub fn get(self, n: &Note, per_step: u32) -> f32 {
        self.get_in(n, per_step, KEY_LO)
    }

    /// The note's value as 0..1; the Note tab spans keys `lo..=lo + KEY_SPAN`.
    pub fn get_in(self, n: &Note, per_step: u32, lo: u16) -> f32 {
        let f = |v: u8, max: f32| f32::from(v) / max;
        match self {
            Tab::Note => (f32::from(n.key.clamp(lo, lo + KEY_SPAN) - lo) / f32::from(KEY_SPAN)).clamp(0.0, 1.0),
            Tab::Vel => f(n.velocity, 128.0),
            Tab::Rel => f(n.release, 128.0),
            Tab::Fine => f(n.fine_pitch, 240.0),
            Tab::Pan => f(n.pan, 128.0),
            Tab::X => f(n.mod_x, 255.0),
            Tab::Y => f(n.mod_y, 255.0),
            Tab::Shift => (n.position % per_step.max(1)) as f32 / per_step.max(1) as f32,
            Tab::Rep => f(n.repeat, f32::from(Note::MAX_REPEAT)),
        }
        .clamp(0.0, 1.0)
    }

    /// Set the note's value from 0..1; the Note tab spans keys `lo..=lo + KEY_SPAN`.
    pub fn set_in(self, n: &mut Note, v: f32, per_step: u32, lo: u16) {
        let v = v.clamp(0.0, 1.0);
        let b = |max: f32| (v * max).round() as u8;
        match self {
            Tab::Note => n.key = (lo + (v * f32::from(KEY_SPAN)).round() as u16).min(KEY_MAX),
            Tab::Vel => n.velocity = b(128.0),
            Tab::Rel => n.release = b(128.0),
            Tab::Fine => n.fine_pitch = b(240.0),
            Tab::Pan => n.pan = b(128.0),
            Tab::X => n.mod_x = b(255.0),
            Tab::Y => n.mod_y = b(255.0),
            Tab::Shift => {
                let ps = per_step.max(1);
                n.position = n.position / ps * ps + ((v * (ps - 1) as f32).round() as u32).min(ps - 1);
            }
            Tab::Rep => n.repeat = b(f32::from(Note::MAX_REPEAT)),
        }
    }

    /// Hint text ("Velocity 39%").
    pub fn text(self, n: &Note, per_step: u32) -> String {
        let v = self.get(n, per_step);
        let pct = |v: f32| format!("{:.0}%", v * 100.0);
        match self {
            Tab::Note => format!("Note {}", key_name(n.key)),
            Tab::Vel => format!("Velocity {}", pct(v)),
            Tab::Rel => format!("Release {}", pct(v)),
            Tab::Fine => format!("Fine pitch {:+.0} cents", (f32::from(n.fine_pitch) - 120.0) * 100.0 / 120.0),
            Tab::Pan => format!("Panning {}", crate::knobs::pan_text((f32::from(n.pan) - 64.0) / 64.0)),
            Tab::X => format!("Mod X {}", pct(v)),
            Tab::Y => format!("Mod Y {}", pct(v)),
            Tab::Shift => format!("Time shift {}", pct(v)),
            Tab::Rep => format!("Note repeat {}", n.repeat),
        }
    }
}

/// "C5" for key 60.
pub fn key_name(k: u16) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[usize::from(k % 12)], k / 12)
}

/// A named set of channels (§21).
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub name: String,
    pub members: BTreeSet<usize>,
}

/// Sort keys of Options > Sort by (§17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Color,
    Name,
    Track,
    Type,
}

/// The rack's name boxes (they use the pattern name box, spec: ui/pattern-menu.md §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RBox {
    /// Rename (and colour) a channel (§13).
    Channel(usize),
    /// Group selected: name of the new filter group (§21).
    GroupSel,
    /// Filter menu "Add filter group".
    NewGroup,
    RenameGroup(usize),
    /// Transpose selected: semitones.
    Transpose,
    /// Set swing mix for selected > Custom: percent.
    SwingMix,
    /// Channel button width > Custom: percent.
    NameWidth,
    /// Color selected > All: one colour (#RRGGBB) for every selected channel.
    ColorAll,
}

/// A channel's loop point under Advanced looping (§15; FL's manual: "play to the end of the last used Bar, Beat or
/// Step" before looping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdvLoop {
    /// Don't loop: the channel plays the pattern length.
    Off,
    Step,
    Beat,
    /// Lemon Studio choice: a channel nobody set loops at its last used bar.
    #[default]
    Bar,
}

impl AdvLoop {
    pub const ALL: [AdvLoop; 4] = [AdvLoop::Off, AdvLoop::Step, AdvLoop::Beat, AdvLoop::Bar];

    pub fn label(self) -> &'static str {
        match self {
            AdvLoop::Off => "Don't loop",
            AdvLoop::Step => "Step",
            AdvLoop::Beat => "Beat",
            AdvLoop::Bar => "Bar",
        }
    }

    /// Steps the loop length rounds up to (`None` = no loop).
    pub fn unit(self) -> Option<u32> {
        match self {
            AdvLoop::Off => None,
            AdvLoop::Step => Some(1),
            AdvLoop::Beat => Some(4),
            AdvLoop::Bar => Some(16),
        }
    }
}

/// Loop types of the loop button's menu (§7), `RackUi::loop_type`.
pub const LOOP_ADVANCED: u8 = 2;

/// Rack UI state. Lock, zip, groups, the filter and loop lengths are saved through [`to_model`] / [`from_model`].
#[derive(Debug, Clone)]
pub struct RackUi {
    /// Selected channels (§8); the focused channel is `State::selected_channel`.
    pub sel: BTreeSet<usize>,
    pub filter: Filter,
    pub groups: Vec<Group>,
    pub zipped: BTreeSet<usize>,
    /// Channels whose mute state solo leaves alone (§10).
    pub locked: BTreeSet<usize>,
    /// The channel the last solo click soloed.
    pub soloed: Option<usize>,
    /// Name button width in percent (§17).
    pub name_pct: u32,
    pub show_mixer: bool,
    pub colorful: bool,
    pub align_left: bool,
    pub auto_resize: AutoResize,
    pub graph_open: bool,
    pub graph_tab: Tab,
    pub loop_mode: bool,
    /// Per-channel loop length in steps (§15).
    pub loop_len: BTreeMap<usize, u32>,
    pub loop_type: u8,
    /// Position of "Unsorted" in the file's group list (`None` = last; see [`to_model`]).
    pub unsorted_at: Option<usize>,
    /// Steps copied by Cut / Copy (the channel's notes of the pattern).
    pub clipboard: Option<Vec<Note>>,
    /// Copied knob value (knob menu, §19).
    pub value_clip: Option<f32>,
    /// Menu checkboxes that have no effect yet (greyed out): their default ticks.
    pub opts: BTreeSet<&'static str>,
    /// Channels waiting for the delete confirmation's Ok.
    pub pending_delete: Option<Vec<usize>>,
    /// The channels waiting are unused audio clips (Purge unused audio clips, spec: ui/unused-channels.md §4).
    pub purging: bool,
    /// Channels to delete now (the confirmation was accepted; the app removes them).
    pub delete_now: Option<Vec<usize>>,
    /// Advanced fill dialog (§22).
    pub fill: Option<FillDlg>,
    /// Lowest key of the graph editor's Note tab (its octave buttons move it).
    pub key_lo: u16,
    /// Per-channel loop points under Advanced looping (rows left out loop at their last used bar).
    pub adv_loop: BTreeMap<usize, AdvLoop>,
}

impl Default for RackUi {
    fn default() -> Self {
        RackUi {
            sel: BTreeSet::from([0]),
            filter: Filter::All,
            groups: Vec::new(),
            zipped: BTreeSet::new(),
            locked: BTreeSet::new(),
            soloed: None,
            name_pct: 100,
            show_mixer: true,
            colorful: false,
            align_left: false,
            auto_resize: AutoResize::Min,
            graph_open: false,
            graph_tab: Tab::Vel,
            loop_mode: false,
            loop_len: BTreeMap::new(),
            loop_type: 0,
            unsorted_at: None,
            clipboard: None,
            value_clip: None,
            opts: ["Show complete piano roll preview", "Show a mute button on the piano roll preview"].into_iter().collect(),
            pending_delete: None,
            purging: false,
            delete_now: None,
            fill: None,
            key_lo: KEY_LO,
            adv_loop: BTreeMap::new(),
        }
    }
}

impl RackUi {
    pub fn select_only(&mut self, r: usize) {
        self.sel.clear();
        self.sel.insert(r);
    }

    /// Back to the defaults for a new or opened project (view options stay).
    pub fn reset(&mut self) {
        let keep = (self.name_pct, self.show_mixer, self.colorful, self.align_left, self.auto_resize, self.opts.clone());
        *self = RackUi::default();
        (self.name_pct, self.show_mixer, self.colorful, self.align_left, self.auto_resize, self.opts) = keep;
    }

    /// Follow a channel renumbering (`m(old row) = new row`, `None` = removed).
    fn remap(&mut self, m: &dyn Fn(usize) -> Option<usize>) {
        let set = |s: &BTreeSet<usize>| s.iter().filter_map(|&r| m(r)).collect::<BTreeSet<usize>>();
        self.sel = set(&self.sel);
        self.zipped = set(&self.zipped);
        self.locked = set(&self.locked);
        for g in &mut self.groups {
            g.members = set(&g.members);
        }
        self.soloed = self.soloed.and_then(m);
        self.loop_len = self.loop_len.iter().filter_map(|(&r, &l)| m(r).map(|n| (n, l))).collect();
        self.adv_loop = self.adv_loop.iter().filter_map(|(&r, &l)| m(r).map(|n| (n, l))).collect();
        if let Some(f) = self.fill.as_mut() {
            match m(f.row) {
                Some(r) => f.row = r,
                None => self.fill = None,
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// Advanced fill (§22)

/// Fill type of the Advanced fill dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillType {
    Manual,
    Every,
    Euclid,
}

impl FillType {
    pub fn label(self) -> &'static str {
        match self {
            FillType::Manual => "Manual",
            FillType::Every => "Fill every",
            FillType::Euclid => "Euclidean",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FillDlg {
    pub row: usize,
    pub kind: FillType,
    /// Sequence length in steps.
    pub len: u32,
    /// Interval (Fill every) or step count (Euclidean).
    pub amount: u32,
    /// Rotation in steps (centre knob).
    pub rotate: u32,
    /// The sequence (one flag per step).
    pub steps: Vec<bool>,
    /// Top-left on screen.
    pub at: (f32, f32),
}

impl FillDlg {
    /// Recompute the sequence from the fill type (Manual keeps the edited steps).
    pub fn rebuild(&mut self) {
        let n = self.len.max(1) as usize;
        let base: Vec<bool> = match self.kind {
            FillType::Manual => {
                let mut v = self.steps.clone();
                v.resize(n, false);
                return self.steps = v;
            }
            FillType::Every => (0..n).map(|i| i % self.amount.max(1) as usize == 0).collect(),
            FillType::Euclid => euclid(n, (self.amount as usize).min(n)),
        };
        let r = self.rotate as usize % n;
        self.steps = (0..n).map(|i| base[(i + n - r) % n]).collect();
    }
}

/// `k` hits spread as evenly as possible over `n` steps, the first on step 0.
pub fn euclid(n: usize, k: usize) -> Vec<bool> {
    (0..n).map(|i| k > 0 && (i * k) % n < k).collect()
}

// ---------------------------------------------------------------------------------------------------------
// Rows, steps and lengths

/// Whether row `r` passes the display filter.
pub fn in_filter(s: &State, r: usize) -> bool {
    match s.rack.filter {
        Filter::All => true,
        Filter::Group(g) => s.rack.groups.get(g).is_some_and(|g| g.members.contains(&r)),
        Filter::Unsorted => !s.rack.groups.iter().any(|g| g.members.contains(&r)),
    }
}

/// The rows the rack shows, top to bottom.
pub fn shown(s: &State) -> Vec<usize> {
    (0..s.project.channels.len()).filter(|&r| in_filter(s, r)).collect()
}

/// Ticks per step.
pub fn per_step(s: &State) -> u32 {
    (u32::from(s.project.ppq.max(4)) / 4).max(1)
}

/// The current pattern's explicit length in steps (`None` = Auto).
pub fn explicit_len(s: &State) -> Option<u32> {
    let ps = per_step(s);
    s.project.pattern(s.pattern).and_then(|p| p.length).filter(|&l| l > 0).map(|l| (l / ps).max(1))
}

/// Pattern length in steps: the explicit length, else whole bars past the last used step, at least one bar (§3, §7).
pub fn pattern_steps(s: &State) -> u32 {
    if let Some(l) = explicit_len(s) {
        return l;
    }
    let ps = per_step(s);
    let last = s.project.pattern(s.pattern).and_then(|p| p.notes.iter().map(|n| n.position / ps).max());
    last.map_or(16, |l| (l / 16 + 1) * 16)
}

/// Set the current pattern's length (`None` = Auto).
pub fn set_len(s: &mut State, steps: Option<u32>) {
    let ps = per_step(s);
    let n = s.pattern.max(1);
    crate::patterns::pat_mut(s, n).length = steps.filter(|&v| v > 0).map(|v| v.min(MAX_STEPS) * ps);
    s.piano.dirty = true;
}

/// Length box value: 0 = Auto.
pub fn len_value(s: &State) -> u32 {
    explicit_len(s).unwrap_or(0)
}

/// A row's own length in steps: its loop length in loop mode, else the pattern length (§15).
pub fn row_len(s: &State, r: usize) -> u32 {
    let pat = pattern_steps(s);
    if !s.rack.loop_mode {
        return pat;
    }
    let own = if s.rack.loop_type == LOOP_ADVANCED { s.project.pattern(s.pattern).and_then(|p| adv_len(s, r, p)) } else { s.rack.loop_len.get(&r).copied() };
    own.unwrap_or(pat).max(1)
}

/// Advanced looping (§15): row `r`'s loop length in pattern `p`, its last used step rounded up to its loop point
/// (`None`: it does not loop, or has no notes there).
pub fn adv_len(s: &State, r: usize, p: &os_model::Pattern) -> Option<u32> {
    let unit = s.rack.adv_loop.get(&r).copied().unwrap_or_default().unit()?;
    let ch = ch_index(s, r)?;
    let ps = per_step(s);
    let last = p.notes.iter().filter(|n| n.channel == ch).map(|n| n.position / ps).max()?;
    Some((last / unit + 1) * unit)
}

/// The loop lengths the transport and the file see: the set ones, or under Advanced looping the current pattern's
/// automatic ones.
pub fn loop_lens(s: &State) -> BTreeMap<usize, u32> {
    if s.rack.loop_type != LOOP_ADVANCED {
        return s.rack.loop_len.clone();
    }
    let Some(p) = s.project.pattern(s.pattern) else { return BTreeMap::new() };
    (0..s.project.channels.len()).filter_map(|r| adv_len(s, r, p).map(|l| (r, l))).collect()
}

/// Channel index (note `channel`) of row `r`.
fn ch_index(s: &State, r: usize) -> Option<u16> {
    s.project.channels.get(r).map(|c| c.index)
}

/// Whether step `k` of channel `ch` has a note (a note anywhere inside the step counts; a muted one is a removed
/// step, see "Mute removed steps").
pub fn step_on(notes: &[Note], ch: u16, k: u32, ps: u32) -> bool {
    notes.iter().any(|n| n.channel == ch && n.position / ps == k && !n.muted())
}

/// The current pattern's notes of row `r`.
pub fn row_notes(s: &State, r: usize) -> Vec<Note> {
    let Some(ch) = ch_index(s, r) else { return Vec::new() };
    s.project.pattern(s.pattern).map(|p| p.notes.iter().filter(|n| n.channel == ch).copied().collect()).unwrap_or_default()
}

/// Replace row `r`'s notes in the current pattern.
fn set_row_notes(s: &mut State, r: usize, notes: Vec<Note>) {
    let Some(ch) = ch_index(s, r) else { return };
    let n = s.pattern.max(1);
    let p = crate::patterns::pat_mut(s, n);
    p.notes.retain(|x| x.channel != ch);
    p.notes.extend(notes.into_iter().map(|x| Note { channel: ch, ..x }));
    p.notes.sort_by_key(|x| (x.position, x.channel));
    s.piano.dirty = true;
}

/// Turn step `k` of row `r` on or off.
pub fn set_step(s: &mut State, r: usize, k: u32, on: bool) {
    let Some(ch) = ch_index(s, r) else { return };
    let ps = per_step(s);
    let keep = s.rack.opts.contains(OPT_MUTE_REMOVED);
    let n = s.pattern.max(1);
    let p = crate::patterns::pat_mut(s, n);
    let has = step_on(&p.notes, ch, k, ps);
    let here = |x: &Note| x.channel == ch && x.position / ps == k;
    if on && !has {
        // A removed step that was kept comes back with its values.
        match p.notes.iter_mut().find(|x| here(x) && x.muted()) {
            Some(x) => x.set_muted(false),
            None => {
                p.notes.push(Note::step(ch, k * ps));
                p.notes.sort_by_key(|x| (x.position, x.channel));
            }
        }
    } else if !on && has {
        if keep {
            // Mute removed steps (§17): the note stays, muted, with its values.
            p.notes.iter_mut().filter(|x| here(x)).for_each(|x| x.set_muted(true));
        } else {
            p.notes.retain(|x| !here(x));
        }
    }
    s.piano.dirty = true;
}

/// Toggle step `k` of row `r`; returns its new state (§9).
pub fn toggle_step(s: &mut State, r: usize, k: u32) -> bool {
    let on = !ch_index(s, r).is_some_and(|ch| s.project.pattern(s.pattern).is_some_and(|p| step_on(&p.notes, ch, k, per_step(s))));
    set_step(s, r, k, on);
    on
}

/// Fill each N steps (§18): the row's notes inside the pattern length become steps 1, 1+N, 1+2N, …
pub fn fill_each(s: &mut State, r: usize, every: u32) {
    let (len, ps) = (row_len(s, r), per_step(s));
    let mut notes: Vec<Note> = row_notes(s, r).into_iter().filter(|n| n.position >= len * ps).collect();
    notes.extend((0..len).step_by(every.max(1) as usize).map(|k| Note::step(0, k * ps)));
    set_row_notes(s, r, notes);
}

/// Write a step sequence (Advanced fill's Accept) over the row's notes inside the sequence length.
pub fn write_sequence(s: &mut State, r: usize, steps: &[bool]) {
    let ps = per_step(s);
    let len = steps.len() as u32;
    let mut notes: Vec<Note> = row_notes(s, r).into_iter().filter(|n| n.position >= len * ps).collect();
    notes.extend(steps.iter().enumerate().filter(|(_, on)| **on).map(|(k, _)| Note::step(0, k as u32 * ps)));
    set_row_notes(s, r, notes);
}

/// Rotate the row's notes by one step, wrapping inside the pattern length (§18).
pub fn rotate(s: &mut State, r: usize, dir: i32) {
    let (len, ps) = (row_len(s, r), per_step(s));
    let span = i64::from(len * ps);
    let notes = row_notes(s, r)
        .into_iter()
        .map(|mut n| {
            if i64::from(n.position) < span {
                n.position = (i64::from(n.position) + i64::from(dir) * i64::from(ps)).rem_euclid(span) as u32;
            }
            n
        })
        .collect();
    set_row_notes(s, r, notes);
}

/// Reverse the row's notes inside its length: step k becomes step len-1-k (channel menu, Lemon Studio choice: notes
/// keep their length and values; a long note's end lands where its start was mirrored).
pub fn reverse(s: &mut State, r: usize) {
    let (len, ps) = (row_len(s, r), per_step(s));
    let notes = row_notes(s, r)
        .into_iter()
        .map(|mut n| {
            let k = n.position / ps;
            if k < len {
                n.position = (len - 1 - k) * ps + n.position % ps;
            }
            n
        })
        .collect();
    set_row_notes(s, r, notes);
}

/// Randomize the row's steps inside its length (channel menu, Lemon Studio choice: about a third of the steps on,
/// each with a random velocity; notes past the length stay).
pub fn randomize(s: &mut State, r: usize, salt: u64) {
    let (len, ps) = (row_len(s, r), per_step(s));
    let mut x = (seed() ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15)) | 1;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut notes: Vec<Note> = row_notes(s, r).into_iter().filter(|n| n.position >= len * ps).collect();
    for k in 0..len {
        if next() % 100 < 35 {
            let mut n = Note::step(0, k * ps);
            n.velocity = (60 + next() % 69) as u8;
            notes.push(n);
        }
    }
    set_row_notes(s, r, notes);
}

/// Shift the row's notes by one step without wrapping (channel menu, Lemon Studio choice: notes pushed before the
/// first step go; rotate wraps instead).
pub fn shift(s: &mut State, r: usize, dir: i32) {
    let ps = i64::from(per_step(s));
    let notes = row_notes(s, r)
        .into_iter()
        .filter_map(|mut n| {
            let p = i64::from(n.position) + i64::from(dir) * ps;
            (p >= 0).then(|| {
                n.position = p as u32;
                n
            })
        })
        .collect();
    set_row_notes(s, r, notes);
}

/// The steps the row shows: on/off for steps `0..len` (used by the Advanced fill dialog).
pub fn row_steps(s: &State, r: usize, len: u32) -> Vec<bool> {
    let ps = per_step(s);
    let notes = row_notes(s, r);
    (0..len).map(|k| notes.iter().any(|n| n.position / ps == k)).collect()
}

// ---------------------------------------------------------------------------------------------------------
// Channel edits

fn channel_name(s: &State, r: usize) -> String {
    s.project.channels.get(r).map(|c| c.name.clone().unwrap_or_else(|| format!("Channel {}", c.index + 1))).unwrap_or_default()
}

/// "<name> #k" with the lowest free k >= 2 (§18 Clone).
pub fn clone_name(s: &State, r: usize) -> String {
    let name = channel_name(s, r);
    let base = match name.rsplit_once(" #") {
        Some((b, k)) if k.parse::<u32>().is_ok() => b.to_string(),
        _ => name,
    };
    let taken = |cand: &str| s.project.channels.iter().any(|c| c.name.as_deref() == Some(cand));
    (2..).map(|k| format!("{base} #{k}")).find(|c| !taken(c)).expect("unbounded")
}

/// Lowest insert (1..=125) no channel is routed to.
pub fn free_insert(s: &State) -> u16 {
    let max = crate::mixer::max_insert(&s.project);
    (1..=max).find(|t| !s.project.channels.iter().any(|c| c.mixer_track == *t)).unwrap_or(max)
}

fn seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

pub fn random_color(s: &mut State, r: usize, salt: u64) {
    if let Some(c) = s.project.channels.get_mut(r) {
        c.color = Some(crate::patterns::random_color(seed() ^ salt.wrapping_mul(0x9e37_79b9)));
    }
}

/// Color selected > Gradient: from the first selected channel's colour to the last one's (a hue sweep when they
/// match or have none; Lemon Studio's version of FL's gradient dialog).
pub fn gradient(s: &mut State) {
    let rows: Vec<usize> = s.rack.sel.iter().copied().filter(|&r| r < s.project.channels.len()).collect();
    if rows.len() < 2 {
        return;
    }
    let (a, b) = (s.project.channels[rows[0]].color, s.project.channels[*rows.last().expect("len >= 2")].color);
    let n = (rows.len() - 1) as f32;
    for (i, &r) in rows.iter().enumerate() {
        let t = i as f32 / n;
        let c = match (a, b) {
            (Some(a), Some(b)) if a != b => {
                let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
                Rgb::new(l(a.r, b.r), l(a.g, b.g), l(a.b, b.b))
            }
            _ => crate::playlist::hsl_rgb(300.0 * t, 0.35, 0.45),
        };
        s.project.channels[r].color = Some(c);
        s.project.channels[r].rack.stored_color = Some(c);
    }
}

/// Ctrl / Alt + click on a LED (§10): solo this channel; the same click again on the soloed channel unmutes all.
/// Locked channels keep their state.
pub fn solo(s: &mut State, r: usize) {
    let n = s.project.channels.len();
    let locked = s.rack.locked.clone();
    let soloed_now = s.project.channels.get(r).is_some_and(|c| c.enabled)
        && (0..n).filter(|&i| i != r && !locked.contains(&i)).all(|i| !s.project.channels[i].enabled);
    for (i, c) in s.project.channels.iter_mut().enumerate() {
        if i == r {
            c.enabled = true;
        } else if !locked.contains(&i) {
            c.enabled = soloed_now;
        }
    }
    s.rack.soloed = (!soloed_now).then_some(r);
    s.piano.dirty = true;
}

pub fn toggle_mute(s: &mut State, r: usize) {
    if let Some(c) = s.project.channels.get_mut(r) {
        c.enabled = !c.enabled;
        s.piano.dirty = true;
    }
}

pub fn toggle_lock(s: &mut State, r: usize) {
    if !s.rack.locked.remove(&r) {
        s.rack.locked.insert(r);
    }
}

/// Channels with no notes in any pattern that no playlist clip uses (Select unused channels,
/// spec: ui/unused-channels.md §1).
pub fn unused(s: &State) -> BTreeSet<usize> {
    let used_clip = |i: u16| s.project.arrangements.iter().any(|a| a.clips.iter().any(|c| c.source == ClipSource::Channel(i)));
    s.project
        .channels
        .iter()
        .enumerate()
        .filter(|(_, c)| !s.project.patterns.iter().any(|p| p.notes.iter().any(|n| n.channel == c.index)) && !used_clip(c.index))
        .map(|(r, _)| r)
        .collect()
}

/// Transpose the selected channels' notes of the current pattern.
pub fn transpose_selected(s: &mut State, semis: i32) {
    let chans: Vec<u16> = s.rack.sel.iter().filter_map(|&r| ch_index(s, r)).collect();
    let n = s.pattern.max(1);
    for note in crate::patterns::pat_mut(s, n).notes.iter_mut().filter(|x| chans.contains(&x.channel)) {
        note.key = (i32::from(note.key) + semis).clamp(0, 131) as u16;
    }
    s.piano.dirty = true;
}

/// Window height for Auto resize (§17): Minimum = fit the shown rows; Maximum = down to the screen bottom.
pub fn auto_height(s: &State) -> Option<f32> {
    let fit = 96.0 + crate::views::rack::rows_h(s).max(crate::views::rack::ROW_H);
    let room = (s.screen.1 - s.rack_pos.1 - 4.0).max(crate::views::rack::MIN_H);
    match s.rack.auto_resize {
        AutoResize::Off => None,
        AutoResize::Min => Some(fit.min(room)),
        AutoResize::Max => Some(room),
    }
}

/// Apply Auto resize after the shown rows changed.
pub fn autosize(s: &mut State) {
    if let Some(h) = auto_height(s) {
        s.rack_size.1 = h.max(crate::views::rack::MIN_H);
    }
}

/// Keep the step count when a column left of the steps changes width (§17: the window width follows).
pub fn relayout(s: &mut State, f: impl FnOnce(&mut State)) {
    let before = crate::views::rack::base_w(s);
    f(s);
    let after = crate::views::rack::base_w(s);
    s.rack_size.0 = (s.rack_size.0 + after - before).max(after + 3.0 * crate::views::rack::STEP_W);
}

// ---------------------------------------------------------------------------------------------------------
// Renumbering (delete, clone, move, sort)

/// Reorder / remove channels in the project: `map[old row] = new row`. Every reference follows (notes, control
/// points, automation clips and their targets, playlist clips, layer children); indices become positions.
pub fn remap_project(p: &mut Project, map: &[Option<usize>]) {
    let idx: BTreeMap<u16, u16> =
        p.channels.iter().enumerate().filter_map(|(pos, c)| map.get(pos).copied().flatten().map(|n| (c.index, n as u16))).collect();
    let mut chans: Vec<(usize, os_model::Channel)> =
        std::mem::take(&mut p.channels).into_iter().enumerate().filter_map(|(pos, c)| map.get(pos).copied().flatten().map(|n| (n, c))).collect();
    chans.sort_by_key(|(n, _)| *n);
    p.channels = chans
        .into_iter()
        .map(|(n, mut c)| {
            c.index = n as u16;
            c.layer_children = c.layer_children.iter().filter_map(|k| idx.get(k).copied()).collect();
            c
        })
        .collect();
    for pat in &mut p.patterns {
        pat.notes.retain_mut(|n| idx.get(&n.channel).map(|&m| n.channel = m).is_some());
        pat.controls.retain_mut(|c| idx.get(&c.channel()).map(|&m| *c = os_model::ControlPoint::new(c.position, m, c.param(), c.value)).is_some());
    }
    p.automations.retain_mut(|a| {
        let Some(&m) = idx.get(&a.channel) else { return false };
        a.channel = m;
        // Channel targets keep the channel in the high half (spec: flp/events/automation.md "Targets").
        a.targets.retain_mut(|t| {
            let hi = (*t >> 16) as u16;
            if *t == target::TEMPO || hi >= 0x4000 {
                return true;
            }
            idx.get(&hi).map(|&n| *t = target::channel(n, *t as u16)).is_some()
        });
        true
    });
    for arr in &mut p.arrangements {
        arr.clips.retain_mut(|c| match c.source {
            ClipSource::Channel(ch) => idx.get(&ch).map(|&m| c.source = ClipSource::Channel(m)).is_some(),
            ClipSource::Pattern(_) => true,
        });
    }
}

/// Apply a new channel order (`order` = old rows top to bottom; rows left out are deleted) to the project, the
/// app state and the hosted plugins.
pub fn reorder(app: &mut App, order: &[usize]) {
    let n = app.state.project.channels.len();
    let mut map = vec![None; n];
    for (new, &old) in order.iter().enumerate() {
        if old < n {
            map[old] = Some(new);
        }
    }
    let m = |r: usize| map.get(r).copied().flatten();
    let s = &mut app.state;
    remap_project(&mut s.project, &map);
    let len = s.project.channels.len().max(1);
    s.rack.remap(&m);
    let keep = |r: usize| m(r).unwrap_or(r.min(len - 1));
    s.selected_channel = keep(s.selected_channel);
    if m(s.plugin_channel).is_none() {
        s.hide(Win::Plugin);
    }
    s.plugin_channel = keep(s.plugin_channel);
    s.hosted_rows = s.hosted_rows.iter().filter_map(|&r| m(r)).collect();
    s.sample_loads = std::mem::take(&mut s.sample_loads).into_iter().filter_map(|(r, p)| m(r).map(|r| (r, p))).collect();
    s.pending_open = s.pending_open.and_then(m);
    s.pending_plugin = s.pending_plugin.and_then(|(r, i)| m(r).map(|r| (r, i)));
    s.piano.clear_selection();
    s.piano.dirty = true;
    s.playlist.sel.clear();
    s.playlist.hover_clip = None;
    if s.rack.sel.is_empty() && !s.project.channels.is_empty() {
        s.rack.select_only(s.selected_channel);
    }
    autosize(s);
    app.remap_rows(&m);
    app.invalidate();
}

/// Clone rows (§18): each copy goes right below its channel, named "<name> #2", same settings and mixer track,
/// no steps; the hosted instrument is copied with its state. The clones become the selection.
pub fn clone_rows(app: &mut App, rows: &[usize]) {
    let n = app.state.project.channels.len();
    let rows: BTreeSet<usize> = rows.iter().copied().filter(|&r| r < n).collect();
    if rows.is_empty() {
        return;
    }
    let mut order = Vec::new();
    let mut sources = Vec::new();
    for r in 0..n {
        order.push(r);
        if rows.contains(&r) {
            let name = clone_name(&app.state, r);
            let mut c = app.state.project.channels[r].clone();
            c.index = app.state.project.channels.len() as u16;
            c.name = Some(name);
            app.state.project.channels.push(c);
            order.push(app.state.project.channels.len() - 1);
            sources.push(r);
        }
    }
    let states: Vec<Option<(usize, Vec<u8>)>> = sources.iter().map(|&r| app.plugin_copy(r)).collect();
    reorder(app, &order);
    let new_rows: Vec<usize> = order.iter().enumerate().filter(|(_, o)| **o >= n).map(|(i, _)| i).collect();
    for (row, st) in new_rows.iter().zip(states) {
        if let Some((index, blob)) = st {
            app.plugin_paste(*row, index, &blob);
        }
    }
    let s = &mut app.state;
    s.rack.sel = new_rows.iter().copied().collect();
    if let Some(&last) = new_rows.last() {
        s.selected_channel = last;
    }
    s.message = Some(if new_rows.len() == 1 { format!("Cloned as {}", channel_name(s, new_rows[0])) } else { format!("Cloned {} channels", new_rows.len()) });
    auto_filter(s);
}

/// Ask before deleting rows (§18: "The selected channel is about to be deleted.").
pub fn ask_delete(s: &mut State, rows: Vec<usize>) {
    s.rack.purging = false;
    if rows.is_empty() {
        return;
    }
    if rows.len() >= s.project.channels.len() {
        s.message = Some("A project keeps at least one channel".into());
        return;
    }
    s.rack.pending_delete = Some(rows);
    s.pat_ui.confirm = true;
}

/// Purge unused audio clips: the unused audio-clip channels go, after the delete confirmation (Lemon Studio cannot
/// undo a channel deletion yet; spec: ui/unused-channels.md §4).
pub fn ask_purge(s: &mut State) -> Option<String> {
    let rows: Vec<usize> = unused(s).into_iter().filter(|&r| s.project.channels[r].kind == ChannelKind::AudioClip).collect();
    if rows.is_empty() {
        return Some("No unused audio clips".into());
    }
    ask_delete(s, rows);
    s.rack.purging = s.rack.pending_delete.is_some();
    None
}

/// Delete rows now: the channels, their notes and clips go.
pub fn delete_rows(app: &mut App, rows: &[usize]) {
    let n = app.state.project.channels.len();
    let order: Vec<usize> = (0..n).filter(|r| !rows.contains(r)).collect();
    let first = rows.iter().copied().min().unwrap_or(0);
    reorder(app, &order);
    let s = &mut app.state;
    let r = first.min(s.project.channels.len().saturating_sub(1));
    s.rack.select_only(r);
    s.selected_channel = r;
    s.message = Some(if rows.len() == 1 { "Deleted the channel".into() } else { format!("Deleted {} channels", rows.len()) });
}

/// Move the selected channels one row up (`dir` -1) or down (+1).
pub fn move_selected(app: &mut App, dir: i32) {
    let n = app.state.project.channels.len();
    let sel = app.state.rack.sel.clone();
    if sel.is_empty() || (dir < 0 && sel.contains(&0)) || (dir > 0 && sel.contains(&(n - 1))) {
        return;
    }
    let mut order: Vec<usize> = (0..n).collect();
    if dir < 0 {
        for i in 1..n {
            if sel.contains(&order[i]) && !sel.contains(&order[i - 1]) {
                order.swap(i, i - 1);
            }
        }
    } else {
        for i in (0..n - 1).rev() {
            if sel.contains(&order[i]) && !sel.contains(&order[i + 1]) {
                order.swap(i, i + 1);
            }
        }
    }
    reorder(app, &order);
}

pub fn sort(app: &mut App, by: SortBy) {
    let s = &app.state;
    let mut order: Vec<usize> = (0..s.project.channels.len()).collect();
    let ch = |r: usize| &s.project.channels[r];
    match by {
        SortBy::Color => order.sort_by_key(|&r| ch(r).color.map_or(u32::MAX, |c| u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b))),
        SortBy::Name => order.sort_by_key(|&r| channel_name(s, r).to_lowercase()),
        SortBy::Track => order.sort_by_key(|&r| ch(r).mixer_track),
        SortBy::Type => order.sort_by_key(|&r| match ch(r).kind {
            ChannelKind::Sampler => 0,
            ChannelKind::Plugin => 1,
            ChannelKind::Layer => 2,
            ChannelKind::AudioClip => 3,
            ChannelKind::Automation => 4,
            ChannelKind::Other(k) => 5 + u32::from(k),
        }),
    }
    reorder(app, &order);
}

// ---------------------------------------------------------------------------------------------------------
// New channels, layers and per-channel settings of the selection

/// Whether a channel cuts itself (cuts and is cut by the same group).
pub fn cuts_itself(c: &os_model::Channel) -> bool {
    c.misc.cut != 0 && c.misc.cut == c.misc.cut_by
}

/// Cut itself (§18): on, the channel cuts and is cut by its cut group (group 1 when it has none), as the Misc tab's
/// Cut self button does; off, both go back to none (Lemon Studio choice).
pub fn toggle_cut_self(app: &mut App, r: usize) {
    use crate::misc_page::M;
    let Some(c) = app.state.project.channels.get(r) else { return };
    let g = if cuts_itself(c) { 0 } else { c.misc.cut.max(1) };
    crate::misc::set(app, r, M::Cut, g as f32);
    crate::misc::set(app, r, M::CutBy, g as f32);
}

/// Set swing mix for selected (§17): the Misc tab's Time > Swing of each selected channel (0..1).
pub fn swing_mix_selected(app: &mut App, v: f32) {
    let sel: Vec<usize> = app.state.rack.sel.iter().copied().collect();
    for r in sel {
        crate::misc::set(app, r, crate::misc_page::M::TimeSwing, v.clamp(0.0, 1.0));
    }
}

/// Change declicking for selected channels (§17): declick mode `m` (spec: audio/declick.md).
pub fn declick_selected(s: &mut State, m: u8) {
    for &r in &s.rack.sel {
        if let Some(c) = s.project.channels.get_mut(r) {
            c.sound.declick = m.min(6);
        }
    }
    s.piano.dirty = true;
}

/// A new channel of `kind` at the bottom of the rack, named `name` (or `name #2`...), selected and in view. Returns
/// its row.
pub fn add_kind(app: &mut App, kind: ChannelKind, name: &str) -> usize {
    let s = &mut app.state;
    let name = crate::playlist::unique_channel_name(s, name);
    let r = crate::app::add_channel(&mut s.project);
    let c = &mut s.project.channels[r];
    c.kind = kind;
    c.name = Some(name);
    s.selected_channel = r;
    s.rack.select_only(r);
    s.piano.dirty = true;
    autosize(s);
    let view_h = crate::views::rack::panel(crate::views::rack::rect(s)).h;
    s.rack_scroll = (crate::views::rack::content_h(s) - view_h).max(0.0);
    auto_filter(s);
    app.invalidate();
    r
}

/// Create layer with selected channels (§17; spec: audio/note-rules.md "Layer channels"): a new layer channel whose
/// children are the selected channels (other layers included, as FL allows).
pub fn create_layer(app: &mut App, rows: &[usize]) -> Option<String> {
    let kids: Vec<u16> = rows.iter().filter_map(|&r| app.state.project.channels.get(r)).map(|c| c.index).collect();
    if kids.is_empty() {
        return Some("Select the channels to layer first".into());
    }
    let r = add_kind(app, ChannelKind::Layer, "Layer");
    app.state.project.channels[r].layer_children = kids;
    Some(format!("Layered {} channel(s)", rows.len()))
}

/// ADD > Audio clip (spec: ui/menus.md, ADD): an audio clip channel, with the sample picked in the file dialog (an
/// empty one when cancelled).
pub fn add_audio_clip(app: &mut App) -> Option<String> {
    let dlg = rfd::FileDialog::new().add_filter("Audio", &["wav", "mp3", "flac", "ogg", "aif", "aiff"]);
    let picked = crate::dialog::pick_file(dlg);
    let name = picked.as_ref().and_then(|p| p.file_stem()).map_or("Audio clip".to_string(), |n| n.to_string_lossy().into_owned());
    let r = add_kind(app, ChannelKind::AudioClip, &name);
    if let Some(p) = picked {
        app.state.project.channels[r].sample_path = Some(p.to_string_lossy().into_owned());
    }
    Some(format!("Added {}", channel_name(&app.state, r)))
}

/// ADD > Automation clip (spec: ui/menus.md, ADD): an automation clip channel that controls nothing yet (one bar,
/// flat at half; Lemon Studio choice), linked later from a control's menu.
pub fn add_automation_clip(app: &mut App) -> Option<String> {
    let r = add_kind(app, ChannelKind::Automation, "Automation Clip");
    let s = &mut app.state;
    let c = &mut s.project.channels[r];
    (c.mixer_track, c.volume) = (0, 1.0);
    let index = c.index;
    let mut a = os_flp::project::new_automation(index, target::TEMPO, 0.5, 4.0);
    a.targets.clear();
    s.project.automations.push(a);
    Some(format!("Added {}", channel_name(s, r)))
}

/// ADD > Layer: an empty layer channel.
pub fn add_layer(app: &mut App) -> Option<String> {
    let r = add_kind(app, ChannelKind::Layer, "Layer");
    Some(format!("Added {}", channel_name(&app.state, r)))
}

/// Replace (§18): generator `index` takes row `r`'s place: the new channel gets the old one's notes (every
/// pattern), mixer track, colour, levels, group and layer parent links, and the old channel goes.
pub fn replace(app: &mut App, r: usize, index: usize) -> Option<String> {
    let old = app.state.project.channels.get(r)?.clone();
    let groups: Vec<usize> = (0..app.state.rack.groups.len()).filter(|&g| app.state.rack.groups[g].members.contains(&r)).collect();
    let before = app.state.project.channels.len();
    let msg = crate::plugin_menu::add_generator(app, index, Some(r));
    if app.state.project.channels.len() == before {
        return Some("That plugin is not available".into());
    }
    // The new channel is right below the old one.
    let n = r + 1;
    let s = &mut app.state;
    let new_index = s.project.channels.get(n)?.index;
    {
        let c = &mut s.project.channels[n];
        (c.color, c.mixer_track, c.volume, c.pan, c.enabled) = (old.color, old.mixer_track, old.volume, old.pan, old.enabled);
        c.rack.stored_color = old.rack.stored_color;
    }
    for p in &mut s.project.patterns {
        p.notes.iter_mut().filter(|x| x.channel == old.index).for_each(|x| x.channel = new_index);
    }
    for c in &mut s.project.channels {
        c.layer_children.iter_mut().filter(|k| **k == old.index).for_each(|k| *k = new_index);
    }
    for g in groups {
        s.rack.groups[g].members.insert(n);
    }
    let order: Vec<usize> = (0..s.project.channels.len()).filter(|&i| i != r).collect();
    reorder(app, &order);
    let s = &mut app.state;
    s.selected_channel = r;
    s.rack.select_only(r);
    msg.or_else(|| Some(format!("Replaced with {}", channel_name(s, r))))
}

/// Color selected > All (§17): `text` as #RRGGBB (or RRGGBB) for every selected channel.
fn color_all(s: &mut State, text: &str) {
    let hex = text.trim().trim_start_matches('#');
    let Ok(v) = u32::from_str_radix(hex, 16) else { return };
    if hex.len() != 6 {
        return;
    }
    let c = Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8);
    for &r in &s.rack.sel {
        if let Some(ch) = s.project.channels.get_mut(r) {
            ch.color = Some(c);
            ch.rack.stored_color = Some(c);
        }
    }
}

/// Layer window (spec: audio/note-rules.md "Layer channels"): row `child` in or out of the children of the layer in
/// row `layer`.
pub fn toggle_child(s: &mut State, layer: usize, child: usize) {
    let Some(ci) = s.project.channels.get(child).map(|c| c.index) else { return };
    let Some(l) = s.project.channels.get_mut(layer).filter(|c| c.kind == ChannelKind::Layer && child != layer) else { return };
    if l.layer_children.contains(&ci) {
        l.layer_children.retain(|&k| k != ci);
    } else {
        l.layer_children.push(ci);
        l.layer_children.sort_unstable();
    }
    s.piano.dirty = true;
}

/// Layer window: the rack selection (the layer itself left out) becomes the layer's children (FL's "Set children").
pub fn set_children(s: &mut State, layer: usize) {
    let kids: Vec<u16> = s.rack.sel.iter().filter(|&&r| r != layer).filter_map(|&r| s.project.channels.get(r)).map(|c| c.index).collect();
    if let Some(l) = s.project.channels.get_mut(layer).filter(|c| c.kind == ChannelKind::Layer) {
        l.layer_children = kids;
        s.piano.dirty = true;
    }
}

/// Layer window: a layer setting flag (`LayerSettings::RANDOM`, `CROSSFADE`, `IN_TURN`) on / off. Random and In turn
/// both pick one child per note, so turning one on turns the other off (Lemon Studio choice).
pub fn toggle_layer_flag(s: &mut State, layer: usize, flag: u32) {
    use os_model::LayerSettings as L;
    let Some(l) = s.project.channels.get_mut(layer).filter(|c| c.kind == ChannelKind::Layer) else { return };
    let f = &mut l.misc.layer.flags;
    if *f & flag != 0 {
        *f &= !flag;
    } else {
        *f |= flag;
        if flag & (L::RANDOM | L::IN_TURN) != 0 {
            *f &= !((L::RANDOM | L::IN_TURN) & !flag);
        }
    }
    s.piano.dirty = true;
}

/// Auto switch display filter (§17): when the focused channel is hidden by the display filter, show its group (the
/// first it is in), else Unsorted.
pub fn auto_filter(s: &mut State) {
    let r = s.selected_channel;
    if !s.rack.opts.contains(OPT_AUTO_FILTER) || r >= s.project.channels.len() || in_filter(s, r) {
        return;
    }
    s.rack.filter = s.rack.groups.iter().position(|g| g.members.contains(&r)).map_or(Filter::Unsorted, Filter::Group);
    autosize(s);
}

/// Focus selected channel on playback (§17): scroll the rack so the selected channel (the highest selected one) is
/// in view.
pub fn focus_on_play(s: &mut State) {
    if !s.rack.opts.contains(OPT_FOCUS) {
        return;
    }
    let r = s.rack.sel.iter().next().copied().unwrap_or(s.selected_channel);
    let Some(p) = crate::views::rack::layout(s).into_iter().find(|p| p.row == r) else { return };
    let view_h = crate::views::rack::panel(crate::views::rack::rect(s)).h;
    let pad = (crate::views::rack::content_h(s) - crate::views::rack::rows_h(s)) / 2.0;
    // The row's band starts 2 px above its name button.
    let (top, bottom) = (p.top - pad - 2.0, p.top + p.pitch() + pad);
    if top < s.rack_scroll {
        s.rack_scroll = top.max(0.0);
    } else if bottom > s.rack_scroll + view_h {
        s.rack_scroll = bottom - view_h;
    }
}

// ---------------------------------------------------------------------------------------------------------
// Name boxes

pub fn box_text(s: &State, b: RBox) -> String {
    match b {
        RBox::Channel(r) => channel_name(s, r),
        RBox::GroupSel | RBox::NewGroup => format!("Group {}", s.rack.groups.len() + 1),
        RBox::RenameGroup(g) => s.rack.groups.get(g).map(|g| g.name.clone()).unwrap_or_default(),
        RBox::Transpose => String::new(),
        RBox::SwingMix => shared(s, |c| (c.misc.swing * 100.0).round() as u32).unwrap_or(100).to_string(),
        RBox::NameWidth => s.rack.name_pct.to_string(),
        RBox::ColorAll => {
            let c = s.project.channels.get(s.selected_channel).and_then(|c| c.color).unwrap_or(Rgb::new(0x92, 0x57, 0x55));
            format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
        }
    }
}

pub fn box_caption(s: &State, b: RBox) -> String {
    match b {
        RBox::Channel(r) => format!("Channel #{} name", s.project.channels.get(r).map_or(r + 1, |c| usize::from(c.index) + 1)),
        RBox::GroupSel | RBox::NewGroup | RBox::RenameGroup(_) => "Filter group name".into(),
        RBox::Transpose => "Semitones".into(),
        RBox::SwingMix => "Swing mix %".into(),
        RBox::NameWidth => "Channel button width %".into(),
        RBox::ColorAll => "Color #RRGGBB".into(),
    }
}

pub fn box_accept(s: &mut State, b: RBox, text: &str) {
    let text = text.trim();
    match b {
        RBox::Channel(r) => {
            if let Some(c) = s.project.channels.get_mut(r) {
                c.name = (!text.is_empty()).then(|| text.to_string());
            }
        }
        RBox::GroupSel | RBox::NewGroup if !text.is_empty() => {
            let members = if b == RBox::GroupSel { s.rack.sel.clone() } else { BTreeSet::new() };
            s.rack.groups.push(Group { name: text.to_string(), members });
            s.rack.filter = Filter::Group(s.rack.groups.len() - 1);
            autosize(s);
        }
        RBox::RenameGroup(g) if !text.is_empty() => {
            if let Some(g) = s.rack.groups.get_mut(g) {
                g.name = text.to_string();
            }
        }
        RBox::Transpose => {
            if let Ok(v) = text.trim_start_matches('+').parse::<i32>() {
                transpose_selected(s, v);
            }
        }
        RBox::SwingMix => {
            // The model directly: the Misc tab's swing has no hosted copy (crate::misc::set only adds the sampler's
            // root / fine).
            if let Ok(v) = text.trim_end_matches('%').trim().parse::<f32>() {
                let v = (v / 100.0).clamp(0.0, 1.0);
                for &r in &s.rack.sel {
                    if let Some(c) = s.project.channels.get_mut(r) {
                        crate::misc::put(&mut c.misc, crate::misc_page::M::TimeSwing, v);
                    }
                }
            }
        }
        RBox::NameWidth => {
            if let Ok(v) = text.trim_end_matches('%').trim().parse::<u32>() {
                relayout(s, |s| s.rack.name_pct = v.clamp(30, 400));
            }
        }
        RBox::ColorAll => color_all(s, text),
        _ => {}
    }
}

/// Open a rack name box at `at`.
pub fn open_box(s: &mut State, b: RBox, at: (f32, f32)) {
    crate::patterns::open_name_box(s, crate::patterns::BoxTarget::Rack(b), at);
}

// ---------------------------------------------------------------------------------------------------------
// Popup menus

/// Clicks on the rack's own controls that are not menu commands (`Action::Rack`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAct {
    /// Layer window: child row in / out of the layer (layer row, child row).
    LayerChild(usize, usize),
    /// Layer window: a `LayerSettings` flag on / off (layer row, flag).
    LayerFlag(usize, u32),
    /// Layer window: the rack selection becomes the layer's children (layer row).
    SetChildren(usize),
    /// The mute button on a row's piano roll preview (§17 "Show a mute button on the piano roll preview").
    PreviewMute(usize),
    /// Graph editor, Note tab: its key span an octave down (-1) or up (+1).
    KeyOctave(i32),
}

/// Commands of the rack's popup menus.
#[derive(Debug, Clone, PartialEq)]
pub enum RCmd {
    // Channel menu (§18), on a row.
    PianoRoll(usize),
    Graph(usize),
    Rename(usize),
    ChangeColor(usize),
    RandomColor(usize),
    LoadSample(usize),
    Clone(usize),
    Delete(usize),
    Cut(usize),
    Copy(usize),
    Paste(usize),
    Fill(usize, u32),
    AdvancedFill(usize),
    Rotate(usize, i32),
    Reverse(usize),
    Randomize(usize),
    Shift(usize, i32),
    /// Cut itself: the channel cuts and is cut by the same group (the Misc tab's "Cut self").
    CutSelf(usize),
    /// Replace ▸: row, generator index (into `Plugins::available`).
    Replace(usize, usize),
    // LED menu (§10).
    Solo(usize),
    Lock(usize),
    // Knob menu (§19) and the mixer box.
    Reset(KnobId),
    CopyValue(KnobId),
    PasteValue(KnobId),
    MixerReset(usize),
    AssignFree(usize),
    // Display filter (§21).
    SetFilter(Filter),
    AddGroup,
    DeleteGroup,
    RenameGroup,
    // Pattern length (§7).
    SetLen(Option<u32>),
    // Loop button (§7).
    LoopType(u8),
    Burn(bool),
    /// A channel's loop point under Advanced looping (§15).
    AdvLoop(usize, AdvLoop),
    // Options menu (§17).
    SelectUnused,
    Purge,
    CloneSel,
    DeleteSel,
    MoveSel(i32),
    Sort(SortBy),
    GroupSel,
    ColorGradient,
    ColorRandom,
    ColorAll,
    CreateLayer,
    /// Set swing mix for selected: percent, `None` = Custom...
    SwingMix(Option<u32>),
    /// Change declicking for selected channels to: declick mode 1..=6.
    Declick(u8),
    MuteSel(bool),
    TransposeSel,
    AssignFreeSel,
    SelectMixerTracks,
    ZipSel,
    UnzipAll,
    AutoResize(AutoResize),
    ResizeNow,
    NameWidth(u32),
    NameWidthCustom,
    SizeToFit,
    AlignLeft(bool),
    ShowMixer,
    Colorful,
    /// A view option of `RackUi::opts` on / off.
    Opt(&'static str),
}

/// Declick modes of "Change declicking for selected channels to" (spec: audio/declick.md: mode 0 is the default,
/// the menu lists modes 1..=6).
pub const DECLICK: [&str; 6] =
    ["Out only (no bleeding)", "Transient (no bleeding)", "Transient (bleeding)", "Generic (bleeding)", "Smooth (bleeding)", "Crossfade (bleeding)"];

/// Percentages of "Set swing mix for selected".
const SWING_MIX: [u32; 5] = [0, 25, 50, 75, 100];

fn it(label: impl Into<std::borrow::Cow<'static, str>>, c: RCmd) -> PItem {
    PItem::new(label, Cmd::Rack(c))
}

/// A menu option that is not available yet: greyed out, showing its default tick.
fn opt(s: &State, label: &'static str) -> PItem {
    PItem::not_yet(label).mark(Mark::Check(s.rack.opts.contains(label)))
}

/// A view option that works: ticked while on.
fn opt_on(s: &State, label: &'static str) -> PItem {
    it(label, RCmd::Opt(label)).mark(Mark::Check(s.rack.opts.contains(label)))
}

/// The value every selected channel shares, if they all do.
fn shared<T: PartialEq>(s: &State, f: impl Fn(&os_model::Channel) -> T) -> Option<T> {
    let mut it = s.rack.sel.iter().filter_map(|&r| s.project.channels.get(r)).map(f);
    let first = it.next()?;
    it.all(|v| v == first).then_some(first)
}

/// Options menu (title ▸, §17).
pub fn options_menu(s: &State, x: f32, y: f32) -> Popup {
    let r = &s.rack;
    let none = r.sel.is_empty();
    let off = |i: PItem, cond: bool| if cond { i.off() } else { i };
    let widths = [100, 200, 300, 400];
    let mut width: Vec<PItem> = widths.iter().map(|&w| it(format!("{w}%"), RCmd::NameWidth(w)).mark(Mark::Radio(r.name_pct == w))).collect();
    let custom = !widths.contains(&r.name_pct);
    width.push(it(if custom { format!("Custom ({}%)...", r.name_pct) } else { "Custom...".into() }, RCmd::NameWidthCustom).mark(Mark::Radio(custom)));
    width.push(PItem::sep());
    width.push(it("Size to fit", RCmd::SizeToFit));
    let items = vec![
        PItem::sub("Add one", add_one_items(s)),
        PItem::not_yet("Loop starter").mark(Mark::Check(false)),
        it("Purge unused audio clips", RCmd::Purge),
        PItem::head("Channels"),
        it("Select unused channels", RCmd::SelectUnused),
        off(it("Clone selected", RCmd::CloneSel).key("Alt+C"), none),
        off(it("Delete selected...", RCmd::DeleteSel).key("Alt+Del"), none),
        off(it("Create layer with selected channels", RCmd::CreateLayer), none),
        PItem::sep(),
        off(it("Move selected up", RCmd::MoveSel(-1)).key("Alt+Up"), none),
        off(it("Move selected down", RCmd::MoveSel(1)).key("Alt+Down"), none),
        PItem::sub(
            "Sort by",
            vec![it("Color", RCmd::Sort(SortBy::Color)), it("Name", RCmd::Sort(SortBy::Name)), it("Track number", RCmd::Sort(SortBy::Track)), it("Type", RCmd::Sort(SortBy::Type))],
        ),
        PItem::sep(),
        off(it("Group selected...", RCmd::GroupSel).key("Alt+G"), none),
        PItem::sub(
            "Color selected",
            vec![it("Gradient...", RCmd::ColorGradient).key("Shift+Alt+C"), it("Random", RCmd::ColorRandom).key("Ctrl+Alt+C"), off(it("All...", RCmd::ColorAll), none)],
        ),
        off(it("Mute selected", RCmd::MuteSel(true)), none),
        off(it("Unmute selected", RCmd::MuteSel(false)), none),
        off(it("Transpose selected...", RCmd::TransposeSel), none),
        PItem::sub("Set swing mix for selected", swing_items(s)),
        PItem::sub("Set truncate swing notes for selected", vec![PItem::not_yet("Turn on"), PItem::not_yet("Turn off")]),
        off(it("Burn selected loops to pattern", RCmd::Burn(false)), none || !r.loop_mode),
        off(it("Assign selected to free mixer track(s)", RCmd::AssignFreeSel).key("Ctrl+L"), none),
        off(it("Select mixer tracks for the selected channels", RCmd::SelectMixerTracks), none),
        PItem::sub("Change declicking for selected channels to", declick_items(s)),
        PItem::sep(),
        off(it("Zip selected", RCmd::ZipSel).key("Alt+Z"), none),
        it("Unzip all", RCmd::UnzipAll).key("Alt+U"),
        PItem::sep(),
        PItem::not_yet("Restretch all").key("Ctrl+Alt+R"),
        PItem::head("Options"),
        PItem::sub(
            "Group playlist tracks created by Loop starter",
            vec![PItem::not_yet("Don't group tracks"), PItem::not_yet("Group tracks"), PItem::not_yet("Always create a track for the pattern clip")],
        ),
        PItem::sub(
            "Auto resize",
            vec![
                it("Off", RCmd::AutoResize(AutoResize::Off)).mark(Mark::Radio(r.auto_resize == AutoResize::Off)),
                it("Minimum height", RCmd::AutoResize(AutoResize::Min)).mark(Mark::Radio(r.auto_resize == AutoResize::Min)),
                it("Maximum height", RCmd::AutoResize(AutoResize::Max)).mark(Mark::Radio(r.auto_resize == AutoResize::Max)),
                PItem::sep(),
                it("Resize now", RCmd::ResizeNow),
            ],
        ),
        opt_on(s, OPT_AUTO_FILTER),
        PItem::sub("Channel button width", width),
        PItem::sub(
            "Channel name alignment",
            vec![it("Left", RCmd::AlignLeft(true)).mark(Mark::Radio(r.align_left)), it("Center", RCmd::AlignLeft(false)).mark(Mark::Radio(!r.align_left))],
        ),
        it("Colorful channel controls", RCmd::Colorful).mark(Mark::Check(r.colorful)),
        opt_on(s, OPT_FOCUS),
        opt_on(s, OPT_MUTE_REMOVED),
        it("Show mixer track selectors", RCmd::ShowMixer).key("Alt+M").mark(Mark::Check(r.show_mixer)),
        opt_on(s, OPT_COMPLETE),
        opt_on(s, OPT_PREVIEW_MUTE),
        opt(s, "Detached"),
        PItem::head("Switch"),
        PItem::not_yet("Switch smart disable for all plugins"),
        PItem::sub("Switch all audio clip channels to", vec![PItem::not_yet("Resample"), PItem::not_yet("Stretch")]),
        PItem::sub("Switch selected audio clip channels to", vec![PItem::not_yet("Resample"), PItem::not_yet("Stretch")]),
        PItem::sub("Switch all plugin editors to", vec![PItem::not_yet("Attached"), PItem::not_yet("Detached")]),
    ];
    Popup { x, y, items, sub: None }
}

/// Set swing mix for selected ▸ (§17): the selected channels' Misc tab swing; ticked when they all have it.
fn swing_items(s: &State) -> Vec<PItem> {
    let none = s.rack.sel.is_empty();
    let cur = shared(s, |c| (c.misc.swing * 100.0).round() as u32);
    let mut v: Vec<PItem> = SWING_MIX
        .iter()
        .map(|&p| {
            let i = it(format!("{p}%"), RCmd::SwingMix(Some(p))).mark(Mark::Radio(cur == Some(p)));
            if none { i.off() } else { i }
        })
        .collect();
    let custom = it("Custom...", RCmd::SwingMix(None)).mark(Mark::Radio(cur.is_some_and(|c| !SWING_MIX.contains(&c))));
    v.push(if none { custom.off() } else { custom });
    v
}

/// Change declicking for selected channels to ▸ (§17): ticked when every selected channel has the mode.
fn declick_items(s: &State) -> Vec<PItem> {
    let cur = shared(s, |c| c.sound.declick);
    DECLICK
        .iter()
        .zip(1u8..)
        .map(|(&label, m)| {
            let i = it(label, RCmd::Declick(m)).mark(Mark::Radio(cur == Some(m)));
            if s.rack.sel.is_empty() { i.off() } else { i }
        })
        .collect()
}

/// Replace ▸ (§18): the generator menu; a pick swaps the channel's generator and keeps its steps, mixer track,
/// colour and levels.
fn replace_items(s: &State, r: usize) -> Vec<PItem> {
    fn map(items: Vec<PItem>, r: usize) -> Vec<PItem> {
        items
            .into_iter()
            .filter_map(|mut i| {
                match i.cmd {
                    Cmd::Plugin(crate::plugin_menu::PlugCmd::Generator { index, .. }) => i.cmd = Cmd::Rack(RCmd::Replace(r, index)),
                    // The lists' footer (Find, Manage, display modes) belongs to the full plugin menus.
                    Cmd::Plugin(_) => return None,
                    _ => {}
                }
                i.children = map(std::mem::take(&mut i.children), r);
                Some(i)
            })
            .collect()
    }
    let mut v = map(crate::plugin_menu::items(s, crate::plugin_menu::Target::Gen(Some(r)), true), r);
    while v.last().is_some_and(|i| i.sep) {
        v.pop();
    }
    v
}

/// Add one ▸: the generator menu, a pick goes to the bottom of the rack (spec: ui/plugin-menu.md §1).
pub(crate) fn add_one_items(s: &State) -> Vec<PItem> {
    crate::plugin_menu::items(s, crate::plugin_menu::Target::Gen(None), true)
}

/// Right click on a name button (§18): two columns.
pub fn channel_menu(s: &State, r: usize, x: f32, y: f32) -> Popup {
    let sampler = s.project.channels.get(r).is_some_and(|c| c.kind.is_sampler());
    let piano_on = s.shown(Win::PianoRoll) && s.selected_channel == r;
    let mut items = vec![
        it("Piano roll", RCmd::PianoRoll(r)).key("F7").mark(Mark::Check(piano_on)),
        it("Graph editor", RCmd::Graph(r)),
        PItem::sep(),
        it("Rename, color and icon...", RCmd::Rename(r)),
        it("Change color...", RCmd::ChangeColor(r)),
        it("Random color", RCmd::RandomColor(r)),
        PItem::not_yet("Change icon..."),
        PItem::sep(),
        if sampler { it("Load sample...", RCmd::LoadSample(r)) } else { PItem::new("Load sample...", Cmd::Nothing).off() },
        it("Cut itself", RCmd::CutSelf(r)).mark(Mark::Check(s.project.channels.get(r).is_some_and(cuts_itself))),
        PItem::new("Show waveform preview", Cmd::Nothing).off(),
        PItem::sep(),
        PItem::sub("Insert", crate::plugin_menu::items(s, crate::plugin_menu::Target::Gen(Some(r)), true)),
        PItem::sub("Replace", replace_items(s, r)),
        PItem::new("Patcherize", Cmd::Nothing).off(),
        it("Clone", RCmd::Clone(r)),
        it("Delete...", RCmd::Delete(r)),
        PItem::sep(),
        PItem::not_yet("Assign to new instrument track"),
        PItem::col(),
        it("Cut", RCmd::Cut(r)),
        it("Copy", RCmd::Copy(r)),
    ];
    let paste = it("Paste", RCmd::Paste(r));
    items.push(if s.rack.clipboard.is_some() { paste } else { paste.off() });
    items.extend([
        PItem::sep(),
        it("Fill each 2 steps", RCmd::Fill(r, 2)),
        it("Fill each 4 steps", RCmd::Fill(r, 4)),
        it("Fill each 8 steps", RCmd::Fill(r, 8)),
        it("Advanced fill...", RCmd::AdvancedFill(r)),
        PItem::new("Auto fill", Cmd::Nothing).off(),
        PItem::sep(),
        it("Rotate left", RCmd::Rotate(r, -1)).key("Shift+Ctrl+Left"),
        it("Rotate right", RCmd::Rotate(r, 1)).key("Shift+Ctrl+Right"),
        it("Shift left", RCmd::Shift(r, -1)),
        it("Shift right", RCmd::Shift(r, 1)),
        it("Reverse", RCmd::Reverse(r)),
        it("Randomize", RCmd::Randomize(r)),
        PItem::sep(),
        PItem::not_yet("MIDI channel through").mark(Mark::Check(false)),
        PItem::sub("Receive notes from", vec![PItem::new("Unlock", Cmd::Nothing).off(), PItem::not_yet("Typing keyboard").mark(Mark::Check(true))]),
        PItem::not_yet("Create instrument..."),
        PItem::sub("Burn MIDI to", vec![PItem::not_yet("Current pattern"), PItem::not_yet("New pattern")]),
    ]);
    Popup { x, y, items, sub: None }
}

/// The Automation / Remote control / Value groups shared by knob and LED menus (§19).
fn value_groups(s: &State, knob: Option<KnobId>, auto: Option<(u32, String, f64)>) -> Vec<PItem> {
    let mut v = vec![
        PItem::head("Automation"),
        PItem::not_yet("Edit events"),
        PItem::new("Edit events in new window", Cmd::Nothing).off(),
        PItem::new("Edit events in piano roll", Cmd::Nothing).off(),
        PItem::not_yet("Init song with this position"),
        PItem::new("Delete initial value", Cmd::Nothing).off(),
        PItem::sep(),
    ];
    v.push(match auto {
        Some((tgt, name, val)) => PItem::new("Create automation clip", Cmd::CreateAutomation(tgt, name, val)),
        None => PItem::new("Create automation clip", Cmd::Nothing).off(),
    });
    v.extend([
        PItem::new("Edit automation clip...", Cmd::Nothing).off(),
        PItem::head("Remote control"),
        PItem::not_yet("Link to controller..."),
        PItem::not_yet("Override global link..."),
        PItem::head("Value"),
    ]);
    match knob {
        Some(k) => {
            v.push(it("Copy value", RCmd::CopyValue(k)));
            let p = it("Paste value", RCmd::PasteValue(k));
            v.push(if s.rack.value_clip.is_some() { p } else { p.off() });
        }
        None => v.extend([PItem::not_yet("Copy value"), PItem::new("Paste value", Cmd::Nothing).off()]),
    }
    v.extend([PItem::not_yet("Type in value..."), PItem::not_yet("Set...")]);
    v
}

/// Right click on a knob (§19): Reset, then the shared groups.
pub fn knob_menu(s: &State, k: KnobId, auto: Option<(u32, String, f64)>, x: f32, y: f32) -> Popup {
    let mut items = vec![it("Reset", RCmd::Reset(k))];
    items.extend(value_groups(s, Some(k), auto));
    Popup { x, y, items, sub: None }
}

/// Right click on the mixer box (§12).
pub fn mixer_menu(s: &State, r: usize, x: f32, y: f32) -> Popup {
    let mut items = vec![it("Reset", RCmd::MixerReset(r))];
    items.extend(value_groups(s, None, None));
    items.push(PItem::sep());
    items.push(it("Assign to free mixer track", RCmd::AssignFree(r)));
    Popup { x, y, items, sub: None }
}

/// Right click on a LED (§10).
pub fn led_menu(s: &State, r: usize, x: f32, y: f32) -> Popup {
    let soloed = s.rack.soloed == Some(r);
    let mut items = vec![
        it("Solo", RCmd::Solo(r)).mark(Mark::Check(soloed)),
        it("Lock active/mute state", RCmd::Lock(r)).mark(Mark::Check(s.rack.locked.contains(&r))),
    ];
    items.extend(value_groups(s, None, None));
    Popup { x, y, items, sub: None }
}

/// Loop box click under Advanced looping (§15; FL's per-channel menu: Don't loop, Step, Beat, Bar).
pub fn adv_loop_menu(s: &State, r: usize, x: f32, y: f32) -> Popup {
    let cur = s.rack.adv_loop.get(&r).copied().unwrap_or_default();
    let mut items = vec![PItem::head("Loop point")];
    items.extend(AdvLoop::ALL.into_iter().map(|a| it(a.label(), RCmd::AdvLoop(r, a)).mark(Mark::Radio(cur == a))));
    Popup { x, y, items, sub: None }
}

/// Group combo (§7): left click lists the filters.
pub fn filter_menu(s: &State, x: f32, y: f32) -> Popup {
    let f = s.rack.filter;
    let mut items = vec![it("All", RCmd::SetFilter(Filter::All)).mark(Mark::Radio(f == Filter::All))];
    items.extend(s.rack.groups.iter().enumerate().map(|(i, g)| it(g.name.clone(), RCmd::SetFilter(Filter::Group(i))).mark(Mark::Radio(f == Filter::Group(i)))));
    items.push(it("Unsorted", RCmd::SetFilter(Filter::Unsorted)).mark(Mark::Radio(f == Filter::Unsorted)));
    Popup { x, y, items, sub: None }
}

/// Group combo right click (§7).
pub fn filter_edit_menu(s: &State, x: f32, y: f32) -> Popup {
    let group = matches!(s.rack.filter, Filter::Group(_));
    let del = it("Delete filter group", RCmd::DeleteGroup);
    let ren = it("Rename filter group", RCmd::RenameGroup);
    Popup { x, y, items: vec![it("Add filter group", RCmd::AddGroup), if group { del } else { del.off() }, if group { ren } else { ren.off() }], sub: None }
}

/// Pattern length box right click (§7).
pub fn length_menu(s: &State, x: f32, y: f32) -> Popup {
    let cur = explicit_len(s);
    let mut items = vec![PItem::head("Set to"), it("Auto", RCmd::SetLen(None)).mark(Mark::Radio(cur.is_none()))];
    for bars in [1u32, 2, 4, 8, 16, 32] {
        let label = if bars == 1 { "1 bar".to_string() } else { format!("{bars} bars") };
        items.push(it(label, RCmd::SetLen(Some(bars * 16))).mark(Mark::Radio(cur == Some(bars * 16))));
    }
    Popup { x, y, items, sub: None }
}

/// Loop button right click (§7).
pub fn loop_menu(s: &State, x: f32, y: f32) -> Popup {
    let t = s.rack.loop_type;
    let on = s.rack.loop_mode;
    let burn = |label: &'static str, all: bool| if on { it(label, RCmd::Burn(all)) } else { it(label, RCmd::Burn(all)).off() };
    let items = vec![
        PItem::head("Loop type"),
        it("Loop step channels", RCmd::LoopType(0)).mark(Mark::Radio(t == 0)),
        it("Loop all channels", RCmd::LoopType(1)).mark(Mark::Radio(t == 1)),
        it("Advanced looping", RCmd::LoopType(LOOP_ADVANCED)).mark(Mark::Radio(t == LOOP_ADVANCED)),
        PItem::head("Burn to pattern"),
        burn("Burn selected channels", false),
        burn("Burn all looping channels", true),
        PItem::head("Options"),
        opt(s, "Always show advanced controls"),
    ];
    Popup { x, y, items, sub: None }
}

// ---------------------------------------------------------------------------------------------------------
// Commands

/// Where the rack's name boxes open: over a row's name button, else near the mouse.
fn box_at_row(s: &State, r: usize, mouse: (f32, f32)) -> (f32, f32) {
    crate::views::rack::name_rect(s, r).map_or(crate::patterns::box_at_mouse(mouse), |nb| (nb.x, nb.y))
}

/// Run a rack menu command. Returns a status message.
pub fn run(app: &mut App, c: RCmd) -> Option<String> {
    let mouse = app.mouse;
    let sel: Vec<usize> = app.state.rack.sel.iter().copied().collect();
    let mut first = None;
    match c {
        RCmd::CutSelf(r) => toggle_cut_self(app, r),
        RCmd::Replace(r, index) => first = replace(app, r, index),
        RCmd::CreateLayer => first = create_layer(app, &sel),
        RCmd::SwingMix(Some(p)) => swing_mix_selected(app, p as f32 / 100.0),
        RCmd::Clone(r) => clone_rows(app, &[r]),
        RCmd::CloneSel => clone_rows(app, &sel),
        RCmd::MoveSel(d) => move_selected(app, d),
        RCmd::Sort(by) => sort(app, by),
        RCmd::LoadSample(r) => {
            if let Some(p) = crate::dialog::modal(|| rfd::FileDialog::new().add_filter("Audio", &["wav", "mp3", "flac", "ogg", "aif", "aiff"]).pick_file()) {
                let s = &mut app.state;
                let path = p.to_string_lossy().into_owned();
                if let Some(ch) = s.project.channels.get_mut(r) {
                    ch.name = p.file_stem().map(|n| n.to_string_lossy().into_owned());
                    ch.sample_path = Some(path.clone());
                }
                s.sample_loads.push((r, path));
                s.piano.dirty = true;
            }
        }
        RCmd::PianoRoll(r) => {
            app.focus_channel(r);
            app.state.raise(Win::PianoRoll);
        }
        _ => {}
    }
    let s = &mut app.state;
    let msg = match c {
        RCmd::Graph(r) => {
            s.selected_channel = r;
            s.rack.select_only(r);
            s.rack.graph_open = true;
            None
        }
        RCmd::Rename(r) => {
            let at = box_at_row(s, r, mouse);
            open_box(s, RBox::Channel(r), at);
            None
        }
        RCmd::ChangeColor(r) => {
            let at = box_at_row(s, r, mouse);
            open_box(s, RBox::Channel(r), at);
            if let Some(b) = s.pat_ui.name_box.as_mut() {
                b.palette = true;
            }
            None
        }
        RCmd::RandomColor(r) => {
            random_color(s, r, r as u64);
            None
        }
        RCmd::Delete(r) => {
            ask_delete(s, vec![r]);
            None
        }
        RCmd::DeleteSel => {
            ask_delete(s, sel.clone());
            None
        }
        RCmd::Cut(r) => {
            s.rack.clipboard = Some(row_notes(s, r));
            set_row_notes(s, r, Vec::new());
            None
        }
        RCmd::Copy(r) => {
            s.rack.clipboard = Some(row_notes(s, r));
            None
        }
        RCmd::Paste(r) => {
            if let Some(n) = s.rack.clipboard.clone() {
                set_row_notes(s, r, n);
            }
            None
        }
        RCmd::Fill(r, n) => {
            fill_each(s, r, n);
            None
        }
        RCmd::AdvancedFill(r) => {
            open_fill(s, r);
            None
        }
        RCmd::Rotate(r, d) => {
            rotate(s, r, d);
            None
        }
        RCmd::Reverse(r) => {
            reverse(s, r);
            None
        }
        RCmd::Randomize(r) => {
            randomize(s, r, r as u64);
            None
        }
        RCmd::Shift(r, d) => {
            shift(s, r, d);
            None
        }
        RCmd::SwingMix(None) => {
            open_box(s, RBox::SwingMix, crate::patterns::box_at_mouse(mouse));
            None
        }
        RCmd::Declick(m) => {
            declick_selected(s, m);
            None
        }
        RCmd::ColorAll => {
            open_box(s, RBox::ColorAll, crate::patterns::box_at_mouse(mouse));
            None
        }
        RCmd::NameWidthCustom => {
            open_box(s, RBox::NameWidth, crate::patterns::box_at_mouse(mouse));
            None
        }
        RCmd::Opt(label) => {
            if !s.rack.opts.remove(label) {
                s.rack.opts.insert(label);
            }
            if label == OPT_AUTO_FILTER {
                auto_filter(s);
            }
            None
        }
        RCmd::AdvLoop(r, a) => {
            s.rack.adv_loop.insert(r, a);
            s.piano.dirty = true;
            None
        }
        RCmd::Solo(r) => {
            solo(s, r);
            None
        }
        RCmd::Lock(r) => {
            toggle_lock(s, r);
            None
        }
        RCmd::Reset(k) => {
            s.set_knob(k, k.default_value());
            None
        }
        RCmd::CopyValue(k) => {
            s.rack.value_clip = Some(s.knob(k));
            None
        }
        RCmd::PasteValue(k) => {
            if let Some(v) = s.rack.value_clip {
                s.set_knob(k, v);
            }
            None
        }
        RCmd::MixerReset(r) => {
            // The default target: the insert the channel was made with (its own number).
            if let Some(c) = s.project.channels.get_mut(r) {
                c.mixer_track = (c.index + 1).min(crate::mixer::max_insert_of(&s.project.mixer));
                s.piano.dirty = true;
            }
            None
        }
        RCmd::AssignFree(r) => {
            let t = free_insert(s);
            if let Some(c) = s.project.channels.get_mut(r) {
                c.mixer_track = t;
                s.piano.dirty = true;
            }
            Some(format!("Routed to Insert {t}"))
        }
        RCmd::AssignFreeSel => {
            for &r in &sel {
                let t = free_insert(s);
                if let Some(c) = s.project.channels.get_mut(r) {
                    c.mixer_track = t;
                }
            }
            s.piano.dirty = true;
            None
        }
        RCmd::SelectMixerTracks => {
            if let Some(t) = sel.first().and_then(|&r| s.project.channels.get(r)).map(|c| c.mixer_track) {
                s.selected_mixer = t;
                s.raise(Win::Mixer);
            }
            None
        }
        RCmd::SetFilter(f) => {
            s.rack.filter = f;
            s.rack_scroll = 0.0;
            autosize(s);
            None
        }
        RCmd::AddGroup => {
            open_box(s, RBox::NewGroup, crate::patterns::box_at_mouse(mouse));
            None
        }
        RCmd::DeleteGroup => {
            if let Filter::Group(g) = s.rack.filter {
                if g < s.rack.groups.len() {
                    s.rack.groups.remove(g);
                }
                s.rack.filter = Filter::All;
                autosize(s);
            }
            None
        }
        RCmd::RenameGroup => {
            if let Filter::Group(g) = s.rack.filter {
                open_box(s, RBox::RenameGroup(g), crate::patterns::box_at_mouse(mouse));
            }
            None
        }
        RCmd::GroupSel => {
            open_box(s, RBox::GroupSel, crate::patterns::box_at_mouse(mouse));
            None
        }
        RCmd::SetLen(v) => {
            set_len(s, v);
            None
        }
        RCmd::LoopType(t) => {
            s.rack.loop_type = t;
            s.piano.dirty = true;
            None
        }
        RCmd::Burn(all) => {
            burn(s, all);
            None
        }
        RCmd::SelectUnused => {
            s.rack.sel = unused(s);
            Some(format!("{} unused channel(s) selected", s.rack.sel.len()))
        }
        RCmd::Purge => ask_purge(s),
        RCmd::ColorGradient => {
            gradient(s);
            None
        }
        RCmd::ColorRandom => {
            for &r in &sel {
                random_color(s, r, r as u64 + 1);
            }
            None
        }
        RCmd::MuteSel(mute) => {
            for &r in &sel {
                if let Some(c) = s.project.channels.get_mut(r) {
                    c.enabled = !mute;
                }
            }
            s.piano.dirty = true;
            None
        }
        RCmd::TransposeSel => {
            open_box(s, RBox::Transpose, crate::patterns::box_at_mouse(mouse));
            None
        }
        RCmd::ZipSel => {
            s.rack.zipped.extend(sel.iter().copied());
            autosize(s);
            None
        }
        RCmd::UnzipAll => {
            s.rack.zipped.clear();
            autosize(s);
            None
        }
        RCmd::AutoResize(a) => {
            s.rack.auto_resize = a;
            autosize(s);
            None
        }
        RCmd::ResizeNow => {
            let keep = s.rack.auto_resize;
            if keep == AutoResize::Off {
                s.rack.auto_resize = AutoResize::Min;
            }
            autosize(s);
            s.rack.auto_resize = keep;
            None
        }
        RCmd::NameWidth(p) => {
            relayout(s, |s| s.rack.name_pct = p);
            None
        }
        RCmd::SizeToFit => {
            let w = crate::views::rack::fit_name_w(s);
            relayout(s, |s| s.rack.name_pct = ((w / NAME_W * 100.0).ceil() as u32).clamp(40, 400));
            None
        }
        RCmd::AlignLeft(l) => {
            s.rack.align_left = l;
            None
        }
        RCmd::ShowMixer => {
            relayout(s, |s| s.rack.show_mixer = !s.rack.show_mixer);
            None
        }
        RCmd::Colorful => {
            s.rack.colorful = !s.rack.colorful;
            None
        }
        RCmd::Clone(_)
        | RCmd::CloneSel
        | RCmd::MoveSel(_)
        | RCmd::Sort(_)
        | RCmd::LoadSample(_)
        | RCmd::PianoRoll(_)
        | RCmd::CutSelf(_)
        | RCmd::Replace(..)
        | RCmd::CreateLayer
        | RCmd::SwingMix(Some(_)) => first,
    };
    app.invalidate();
    msg
}

// ---------------------------------------------------------------------------------------------------------
// Advanced fill dialog

pub fn open_fill(s: &mut State, r: usize) {
    let len = pattern_steps(s).min(64);
    let steps = row_steps(s, r, len);
    let at = (((s.screen.0 - crate::views::rack::FILL_W) / 2.0).round(), ((s.screen.1 - crate::views::rack::FILL_H) / 2.0).round().max(90.0));
    s.rack.fill = Some(FillDlg { row: r, kind: FillType::Manual, len, amount: 4, rotate: 0, steps, at });
}

// ---------------------------------------------------------------------------------------------------------
// Loops (§15)

/// The project as the transport plays it: in loop mode, channels with their own loop length repeat their first
/// steps across the pattern (polyrhythms).
pub fn play_project(s: &State) -> Project {
    let mut p = s.project.clone();
    p.tempo = Some(s.tempo);
    let advanced = s.rack.loop_type == LOOP_ADVANCED;
    if s.rack.loop_mode && (advanced || !s.rack.loop_len.is_empty()) {
        let rows: Vec<usize> = if advanced { (0..p.channels.len()).collect() } else { s.rack.loop_len.keys().copied().collect() };
        expand_loops(&mut p, s, &rows);
    }
    p
}

/// Repeat each row's first `loop_len` steps up to the end of every pattern; notes past the loop length go. Under
/// Advanced looping each pattern has its own lengths (the last used step rounded up to the channel's loop point).
fn expand_loops(p: &mut Project, s: &State, rows: &[usize]) {
    let ps = per_step(s);
    let bar = ps * 16;
    let advanced = s.rack.loop_type == LOOP_ADVANCED;
    for &r in rows {
        let Some(ch) = p.channels.get(r).map(|c| c.index) else { continue };
        for pat in &mut p.patterns {
            let len = if advanced { adv_len(s, r, pat) } else { s.rack.loop_len.get(&r).copied() };
            let Some(len) = len else { continue };
            let span = len.max(1) * ps;
            let end = pat.length.filter(|&l| l > 0).unwrap_or_else(|| {
                let last = pat.notes.iter().map(|n| n.position + n.length).max().unwrap_or(0);
                (last / bar + 1) * bar
            });
            let src: Vec<Note> = pat.notes.iter().filter(|n| n.channel == ch && n.position < span).copied().collect();
            if src.is_empty() {
                continue;
            }
            pat.notes.retain(|n| n.channel != ch);
            let mut off = 0;
            while off < end {
                pat.notes.extend(src.iter().filter(|n| n.position + off < end).map(|n| Note { position: n.position + off, ..*n }));
                off += span;
            }
            pat.notes.sort_by_key(|n| (n.position, n.channel));
        }
    }
}

/// Loop button > Burn: write the loops into the current pattern's notes and drop the loop lengths.
fn burn(s: &mut State, all: bool) {
    let lens = loop_lens(s);
    let rows: Vec<usize> = if all { lens.keys().copied().collect() } else { s.rack.sel.iter().copied().filter(|r| lens.contains_key(r)).collect() };
    let mut p = s.project.clone();
    expand_loops(&mut p, s, &rows);
    if let (Some(src), Some(dst)) = (p.pattern(s.pattern).cloned(), s.project.patterns.iter_mut().find(|x| x.number == s.pattern)) {
        dst.notes = src.notes;
    }
    for r in rows {
        s.rack.loop_len.remove(&r);
    }
    s.piano.dirty = true;
}

// ---------------------------------------------------------------------------------------------------------
// Keyboard (§20)

/// Keys while the rack is the front window. Returns whether the key was taken.
pub fn key(app: &mut App, key: &winit::keyboard::Key, ctrl: bool, alt: bool) -> bool {
    use winit::keyboard::{Key, NamedKey};
    let shift = app.shift;
    let s = &mut app.state;
    if s.playlist.popup.is_some() {
        if matches!(key, Key::Named(NamedKey::Escape)) {
            s.playlist.popup = None;
            app.invalidate();
            return true;
        }
        return false;
    }
    if s.rack.fill.is_some() {
        match key {
            Key::Named(NamedKey::Escape) => s.rack.fill = None,
            Key::Named(NamedKey::Enter) => fill_accept(s),
            _ => return false,
        }
        app.invalidate();
        return true;
    }
    let sel: Vec<usize> = s.rack.sel.iter().copied().collect();
    let focus = s.selected_channel;
    let letter = |c: &str, l: &str| c.eq_ignore_ascii_case(l);
    let cmd = match key {
        Key::Named(NamedKey::ArrowUp) if alt => RCmd::MoveSel(-1),
        Key::Named(NamedKey::ArrowDown) if alt => RCmd::MoveSel(1),
        Key::Named(k @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) if !ctrl => {
            let rows = shown(s);
            let cur = rows.iter().position(|&r| Some(&r) == s.rack.sel.iter().next()).or_else(|| rows.iter().position(|&r| r == focus));
            let next = match (cur, k) {
                (Some(i), NamedKey::ArrowUp) => i.saturating_sub(1),
                (Some(i), _) => (i + 1).min(rows.len().saturating_sub(1)),
                (None, _) => 0,
            };
            if let Some(&r) = rows.get(next) {
                s.rack.select_only(r);
                app.focus_channel(r);
            }
            app.invalidate();
            return true;
        }
        Key::Named(k @ (NamedKey::ArrowLeft | NamedKey::ArrowRight)) if ctrl && shift => RCmd::Rotate(focus, if *k == NamedKey::ArrowLeft { -1 } else { 1 }),
        Key::Named(k @ (NamedKey::PageUp | NamedKey::PageDown)) => {
            let mut all = vec![Filter::All];
            all.extend((0..s.rack.groups.len()).map(Filter::Group));
            all.push(Filter::Unsorted);
            let i = all.iter().position(|&f| f == s.rack.filter).unwrap_or(0);
            let j = if *k == NamedKey::PageUp { (i + all.len() - 1) % all.len() } else { (i + 1) % all.len() };
            RCmd::SetFilter(all[j])
        }
        Key::Named(NamedKey::Delete) if alt => RCmd::DeleteSel,
        Key::Named(NamedKey::Escape) => {
            s.hide(Win::ChannelRack);
            app.invalidate();
            return true;
        }
        Key::Character(c) if alt && shift && letter(c, "c") => RCmd::ColorGradient,
        Key::Character(c) if ctrl && alt && letter(c, "c") => RCmd::ColorRandom,
        Key::Character(c) if alt && !ctrl && letter(c, "c") => RCmd::CloneSel,
        Key::Character(c) if alt && !ctrl && letter(c, "g") => RCmd::GroupSel,
        Key::Character(c) if alt && !ctrl && letter(c, "z") => RCmd::ZipSel,
        Key::Character(c) if alt && !ctrl && letter(c, "u") => RCmd::UnzipAll,
        Key::Character(c) if alt && !ctrl && letter(c, "m") => RCmd::ShowMixer,
        Key::Character(c) if ctrl && !alt && letter(c, "l") => RCmd::AssignFreeSel,
        Key::Character(c) if ctrl && !alt && letter(c, "k") => {
            s.rack.graph_open = !s.rack.graph_open;
            app.invalidate();
            return true;
        }
        // (shift / ctrl +) 0..9: mute / solo channels 1..10 when the typing keyboard does not take the digits (§10).
        Key::Character(c) if !alt && !app.typing_on() && c.len() == 1 && c.chars().all(|d| d.is_ascii_digit()) => {
            let d = c.parse::<usize>().unwrap_or(0);
            let r = if d == 0 { 9 } else { d - 1 };
            let s = &mut app.state;
            if r < s.project.channels.len() {
                if ctrl { solo(s, r) } else { toggle_mute(s, r) }
            }
            app.invalidate();
            return true;
        }
        _ => return false,
    };
    let _ = sel;
    if let Some(m) = run(app, cmd) {
        app.state.message = Some(m);
    }
    true
}

/// Accept in the Advanced fill dialog: write the sequence to the channel.
pub fn fill_accept(s: &mut State) {
    if let Some(f) = s.rack.fill.take() {
        write_sequence(s, f.row, &f.steps);
    }
}

/// Clicks in the Advanced fill dialog (§22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillHit {
    Close,
    /// Fill type combo: Manual -> Fill every -> Euclidean.
    Type,
    Len(i32),
    Amount(i32),
    Rotate(i32),
    Dot(usize),
    Random,
    Reset,
    Accept,
    Inside,
}

pub fn fill_hit(s: &mut State, h: FillHit) {
    match h {
        FillHit::Accept => return fill_accept(s),
        FillHit::Close => {
            s.rack.fill = None;
            return;
        }
        _ => {}
    }
    let Some(f) = s.rack.fill.as_mut() else { return };
    match h {
        FillHit::Type => {
            f.kind = match f.kind {
                FillType::Manual => FillType::Every,
                FillType::Every => FillType::Euclid,
                FillType::Euclid => FillType::Manual,
            };
            f.amount = f.amount.clamp(1, f.len.max(1));
        }
        FillHit::Len(d) => {
            f.len = (f.len as i32 + d).clamp(1, 64) as u32;
            f.amount = f.amount.clamp(1, f.len);
        }
        FillHit::Amount(d) => f.amount = (f.amount as i32 + d).clamp(1, f.len.max(1) as i32) as u32,
        FillHit::Rotate(d) => {
            let n = f.len.max(1) as i32;
            if f.kind == FillType::Manual {
                let k = d.rem_euclid(n) as usize;
                { let n = f.steps.len(); f.steps.rotate_right(k.min(n)); }
            } else {
                f.rotate = (f.rotate as i32 + d).rem_euclid(n) as u32;
            }
        }
        FillHit::Dot(k) => {
            if let Some(b) = f.steps.get_mut(k) {
                *b = !*b;
            }
            f.kind = FillType::Manual;
            f.rotate = 0;
        }
        FillHit::Random => {
            let mut x = seed() | 1;
            for b in f.steps.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = x % 100 < 35;
            }
            f.kind = FillType::Manual;
            f.rotate = 0;
        }
        FillHit::Reset => {
            f.steps.iter_mut().for_each(|b| *b = false);
            f.kind = FillType::Manual;
            f.rotate = 0;
        }
        FillHit::Accept | FillHit::Close | FillHit::Inside => {}
    }
    f.rebuild();
}

/// Graph editor (§14): set step `k`'s value of the focused channel from 0..1; a column without a step edits the
/// nearest step note to its left.
pub fn graph_set(s: &mut State, k: u32, v: f32) -> Option<String> {
    let r = s.selected_channel;
    let ch = ch_index(s, r)?;
    let ps = per_step(s);
    let tab = s.rack.graph_tab;
    let lo = s.rack.key_lo;
    let n = s.pattern.max(1);
    let p = crate::patterns::pat_mut(s, n);
    let idx = p
        .notes
        .iter()
        .enumerate()
        .filter(|(_, x)| x.channel == ch && x.position / ps <= k)
        .max_by_key(|(_, x)| x.position)
        .map(|(i, _)| i)?;
    let note = &mut p.notes[idx];
    tab.set_in(note, v, ps, lo);
    let text = tab.text(note, ps);
    if tab == Tab::Shift {
        p.notes.sort_by_key(|x| (x.position, x.channel));
    }
    s.piano.dirty = true;
    Some(text)
}

// ---------------------------------------------------------------------------------------------------------
// Saving (spec: ui/channel-rack.md "Saving"; flp/events/channels.md, project-info.md, patterns.md)

/// Loop type values of event 162 for the rack's loop types (Loop step channels, Loop all channels, Advanced
/// looping). Advanced looping was not measured; it is written like step channels.
const LOOP_TYPE_CODES: [u32; 3] = [3, 2, 3];

/// The rack state of an opened (or new) project: lock, zip, filter groups and the current filter from the channels,
/// loop lengths and loop mode from the current pattern.
pub fn from_model(s: &mut State) {
    let p = &s.project;
    let rack = &mut s.rack;
    rack.locked = p.channels.iter().enumerate().filter(|(_, c)| c.rack.locked).map(|(r, _)| r).collect();
    rack.zipped = p.channels.iter().enumerate().filter(|(_, c)| c.rack.zipped).map(|(r, _)| r).collect();
    rack.groups.clear();
    rack.unsorted_at = None;
    let mut file_to_ui = Vec::new();
    for (i, name) in p.groups.iter().enumerate() {
        if name == os_model::UNSORTED_GROUP && rack.unsorted_at.is_none() {
            rack.unsorted_at = Some(i);
            file_to_ui.push(None);
            continue;
        }
        let members = p.channels.iter().enumerate().filter(|(_, c)| c.rack.group as usize == i).map(|(r, _)| r).collect();
        file_to_ui.push(Some(rack.groups.len()));
        rack.groups.push(Group { name: name.clone(), members });
    }
    rack.filter = match p.current_group.map(|g| g as usize) {
        None => Filter::All,
        Some(g) => match file_to_ui.get(g) {
            Some(Some(u)) => Filter::Group(*u),
            Some(None) => Filter::Unsorted,
            None => Filter::All,
        },
    };
    let pat = p.pattern(s.pattern);
    let loops = pat.map(|p| p.loops.clone()).unwrap_or_default();
    rack.loop_len = loops
        .iter()
        // 0 = no length (FL writes it for the other channels while loop mode is off).
        .filter(|&&(_, steps)| steps != 0 && steps != os_model::Pattern::LOOP_FOLLOW)
        .filter_map(|&(ch, steps)| p.channels.iter().position(|c| c.index == ch).map(|r| (r, steps)))
        .collect();
    let all = loops.iter().any(|&(_, steps)| steps == os_model::Pattern::LOOP_FOLLOW);
    let off = pat.and_then(|p| p.loop_off);
    rack.loop_type = match off {
        Some(2) => 1,
        Some(_) => 0,
        None if all => 1,
        None => 0,
    };
    rack.loop_mode = !loops.is_empty() && off.is_none();
}

/// Writes the rack state into the project before a save (the inverse of [`from_model`]).
pub fn to_model(s: &mut State) {
    // Advanced looping writes the current pattern's automatic lengths while loop mode is on (its per-channel loop
    // points are not saved).
    let lens = if s.rack.loop_type == LOOP_ADVANCED && !s.rack.loop_mode { BTreeMap::new() } else { loop_lens(s) };
    let rack = &s.rack;
    let p = &mut s.project;
    // Groups in file order: the user groups, with "Unsorted" where the file had it (else last, like FL's Alt+G).
    let mut names: Vec<String> = rack.groups.iter().map(|g| g.name.clone()).collect();
    let unsorted = rack.unsorted_at.unwrap_or(names.len()).min(names.len());
    names.insert(unsorted, os_model::UNSORTED_GROUP.to_string());
    let file_index = |g: usize| if g < unsorted { g } else { g + 1 } as u32;
    for (r, c) in p.channels.iter_mut().enumerate() {
        c.rack.locked = rack.locked.contains(&r);
        c.rack.zipped = rack.zipped.contains(&r);
        // A channel is in one group in the file: its first one.
        c.rack.group = rack.groups.iter().position(|g| g.members.contains(&r)).map_or(unsorted as u32, file_index);
    }
    // A file without groups keeps none when nothing was grouped.
    if !(p.groups.is_empty() && rack.groups.is_empty()) {
        p.groups = names;
    }
    p.current_group = match rack.filter {
        Filter::All => None,
        Filter::Group(g) => Some(file_index(g)),
        Filter::Unsorted => Some(unsorted as u32),
    };
    if p.groups.is_empty() {
        p.current_group = None;
    }
    // Loop lengths belong to the current pattern.
    let mut loops: Vec<(u16, u32)> = lens.iter().filter_map(|(&r, &l)| p.channels.get(r).map(|c| (c.index, l))).collect();
    if rack.loop_type == 1 && !loops.is_empty() {
        let set: BTreeSet<u16> = loops.iter().map(|l| l.0).collect();
        loops.extend(p.channels.iter().filter(|c| !set.contains(&c.index)).map(|c| (c.index, os_model::Pattern::LOOP_FOLLOW)));
    }
    loops.sort_by_key(|l| l.0);
    let loop_off = (!rack.loop_mode && !loops.is_empty()).then(|| LOOP_TYPE_CODES[usize::from(rack.loop_type.min(2))]);
    let n = s.pattern;
    // Pairs with no length (0) carry nothing: a file that has them is unchanged.
    let set = |l: &[(u16, u32)]| l.iter().filter(|x| x.1 != 0).copied().collect::<BTreeSet<_>>();
    let changed = p.pattern(n).map_or(!loops.is_empty(), |pat| set(&pat.loops) != set(&loops) || pat.loop_off != loop_off);
    if changed {
        let pat = crate::patterns::pat_mut(s, n);
        pat.loops = loops;
        pat.loop_off = loop_off;
    }
}

// ---------------------------------------------------------------------------------------------------------
// Mouse input in the rack (spec: ui/channel-rack.md §7-15, §22): presses (`App::rack_act`, `App::rack_right`) and the
// drags that follow them (`drag_move`).

/// A channel rack drag moved to (`x`, `y`) (`App::mouse_move`).
pub(crate) fn drag_move(app: &mut App, d: Drag, x: f32, y: f32) {
    match d {
        Drag::RackResize { edges, m0, s0 } => {
            let old = views::rack::bounds(&app.state);
            let s = &mut app.state;
            let w = if edges & views::rack::EDGE_RIGHT != 0 { s0.0 + x - m0.0 } else { s0.0 };
            let h = if edges & views::rack::EDGE_BOTTOM != 0 { s0.1 + y - m0.1 } else { s0.1 };
            s.rack_size = views::rack::snap_size(s, w, h);
            let new = views::rack::bounds(&app.state);
            app.damage_rect(old.union(&new));
        }
        Drag::RackMixer { ch, y0, v0 } => {
            // One insert per 10 px, up = higher (spec: ui/channel-rack.md §12).
            let v = (i32::from(v0) + ((y0 - y) / crate::rack::MIXER_DRAG).round() as i32).clamp(0, i32::from(crate::mixer::max_insert(&app.state.project))) as u16;
            let s = &mut app.state;
            if let Some(c) = s.project.channels.get_mut(ch) {
                if c.mixer_track != v {
                    c.mixer_track = v;
                    s.piano.dirty = true;
                }
            }
            app.hint = Cow::Owned(format!("Target mixer track: {}", views::rack::insert_name(v)));
            let r = app.hits.iter().rev().find(|h| h.action == Action::RackMixer(ch)).map(|h| h.rect);
            match r {
                Some(r) => app.damage_rect(r.outset(2.0)),
                None => app.invalidate(),
            }
            app.damage_rect(HINT_RECT);
        }
        Drag::RackPaint { row, on, last } => {
            if let Some(k) = views::rack::step_at(&app.state, x) {
                if k != last {
                    for j in last.min(k)..=last.max(k) {
                        crate::rack::set_step(&mut app.state, row, j, on);
                    }
                    app.drag = Some(Drag::RackPaint { row, on, last: k });
                    app.invalidate();
                }
            }
        }
        Drag::RackSel { from } => {
            if let Some(r) = views::rack::row_at(&app.state, y) {
                let shown = crate::rack::shown(&app.state);
                if let (Some(a), Some(b)) = (shown.iter().position(|&x| x == from), shown.iter().position(|&x| x == r)) {
                    app.state.rack.sel = shown[a.min(b)..=a.max(b)].iter().copied().collect();
                }
            }
            app.invalidate();
        }
        Drag::RackDots { x0, p0 } => {
            let w = p0 as f32 / 100.0 * crate::rack::NAME_W + (x - x0);
            let pct = (w / crate::rack::NAME_W * 100.0).round().clamp(30.0, 400.0) as u32;
            crate::rack::relayout(&mut app.state, |s| s.rack.name_pct = pct);
            app.hint = Cow::Owned(format!("Channel button width: {pct}%"));
            app.invalidate();
        }
        Drag::RackLen { y0, v0 } => {
            let v = (v0 as i32 + ((y0 - y) / crate::rack::LEN_DRAG).round() as i32).clamp(0, crate::rack::MAX_STEPS as i32) as u32;
            crate::rack::set_len(&mut app.state, (v > 0).then_some(v));
            app.hint = Cow::Owned(if v == 0 { "Pattern length: Auto".into() } else { format!("Pattern length: {v} steps") });
            app.invalidate();
        }
        Drag::RackLoopLen { row, y0, v0 } => {
            let v = (v0 as i32 + ((y0 - y) / crate::rack::LEN_DRAG).round() as i32).clamp(1, crate::rack::MAX_STEPS as i32) as u32;
            app.state.rack.loop_len.insert(row, v);
            app.state.piano.dirty = true;
            app.hint = Cow::Owned(format!("Loop point: {v} steps"));
            app.invalidate();
        }
        Drag::RackGraph => app.rack_graph_at((x, y)),
        _ => {}
    }
}

impl App {
    /// Name button click (spec: ui/channel-rack.md §13): opens the channel's plugin window, or closes it when open.
    pub(crate) fn toggle_channel_window(&mut self, i: usize) {
        if self.state.hosted_rows.contains(&i) {
            if !self.plugins.close_if_open(i) {
                self.state.pending_open = Some(i);
            }
            return;
        }
        let s = &mut self.state;
        if s.project.channels.get(i).is_some_and(|c| c.kind.is_sampler()) {
            // Not hosted yet (just added): Lemon Sampler starts on the next tick and its window opens then.
            s.pending_open = Some(i);
            self.sampler_failed.retain(|&r| r != i);
        } else if s.shown(Win::Plugin) && s.plugin_channel == i {
            s.hide(Win::Plugin);
        } else {
            if !s.shown(Win::Plugin) {
                let r = views::rack::rect(s);
                s.plugin_pos = ((r.right() + 8.0).min(1920.0 - 440.0), r.y.min(1080.0 - 210.0));
            }
            s.plugin_channel = i;
            s.raise(Win::Plugin);
        }
    }

    /// Press on a step (spec: ui/channel-rack.md §9): selects the row, toggles the step, starts painting.
    pub(crate) fn rack_step(&mut self, i: usize, k: u32) {
        self.state.rack.select_only(i);
        self.focus_channel(i);
        let s = &mut self.state;
        s.piano.clear_selection();
        // A step past an explicit length grows the pattern to the bar holding it (§5).
        if let Some(l) = crate::rack::explicit_len(s) {
            if k >= l {
                crate::rack::set_len(s, Some((k / 16 + 1) * 16));
            }
        }
        let on = crate::rack::toggle_step(s, i, k);
        self.drag = Some(Drag::RackPaint { row: i, on, last: k });
    }

    /// Speaker (spec: ui/channel-rack.md §7): plays the current pattern; while it plays, pauses / resumes.
    pub(crate) fn rack_speaker(&mut self) {
        match self.transport.state() {
            TransportState::Playing if !self.state.song_mode => self.transport.pause(),
            TransportState::Paused if !self.state.song_mode => self.transport.resume(),
            _ => {
                if self.transport.playing() {
                    self.transport.stop();
                }
                self.state.song_mode = false;
                crate::rack::focus_on_play(&mut self.state);
                self.start_playback();
            }
        }
    }

    /// Graph editor click / drag at a screen point (spec: ui/channel-rack.md §14).
    pub(crate) fn rack_graph_at(&mut self, (x, y): (f32, f32)) {
        let Some(b) = views::rack::graph_bars(&self.state) else { return };
        let Some(k) = views::rack::step_at(&self.state, x) else { return };
        let v = ((b.bottom() - y) / b.h).clamp(0.0, 1.0);
        if let Some(t) = crate::rack::graph_set(&mut self.state, k, v) {
            self.hint = Cow::Owned(t);
        }
        self.invalidate();
    }

    /// Channel rack left clicks (spec: ui/channel-rack.md §7-15, §22). Returns whether `a` was one.
    pub(crate) fn rack_act(&mut self, a: &Action) -> bool {
        use crate::rack as rk;
        let (shift, ctrl, alt) = (self.shift, self.ctrl, self.alt);
        let now = std::time::Instant::now();
        let double = matches!(&self.rack_click, Some((p, t)) if p == a && now.duration_since(*t).as_millis() < 400);
        if matches!(a, Action::RackSelect(_) | Action::RackMixer(_)) {
            self.rack_click = if double { None } else { Some((a.clone(), now)) };
        }
        let mouse = self.mouse;
        match *a {
            Action::RackSelect(i) => {
                if double {
                    self.state.rack.sel = rk::shown(&self.state).into_iter().collect();
                } else if shift {
                    let sel = &mut self.state.rack.sel;
                    if !sel.remove(&i) {
                        sel.insert(i);
                    }
                } else {
                    self.state.rack.select_only(i);
                    self.focus_channel(i);
                    self.drag = Some(Drag::RackSel { from: i });
                }
            }
            Action::ChannelSelect(i) => {
                if shift {
                    let at = views::rack::name_rect(&self.state, i).map_or(mouse, |r| (r.x, r.y));
                    rk::open_box(&mut self.state, rk::RBox::Channel(i), at);
                } else if !ctrl {
                    self.state.rack.select_only(i);
                    self.focus_channel(i);
                    self.toggle_channel_window(i);
                }
            }
            Action::ChannelEnable(i) => {
                let s = &mut self.state;
                if ctrl || alt {
                    rk::solo(s, i);
                } else if shift {
                    rk::toggle_lock(s, i);
                } else {
                    rk::toggle_mute(s, i);
                }
            }
            Action::Step(i, k) => self.rack_step(i, k),
            Action::RackMixer(i) => {
                let s = &mut self.state;
                if double {
                    if let Some(t) = s.project.channels.get(i).map(|c| c.mixer_track) {
                        s.selected_mixer = t;
                        s.raise(Win::Mixer);
                    }
                } else {
                    let v0 = s.project.channels.get(i).map_or(0, |c| c.mixer_track);
                    self.drag = Some(Drag::RackMixer { ch: i, y0: mouse.1, v0 });
                }
            }
            Action::RackOptions => {
                let p = rk::options_menu(&self.state, self.state.rack_pos.0, self.state.rack_pos.1 + 28.0);
                self.state.playlist.popup = Some(p);
            }
            Action::RackFilter => {
                let p = rk::filter_menu(&self.state, mouse.0, mouse.1 + 8.0);
                self.state.playlist.popup = Some(p);
            }
            Action::RackDots => self.drag = Some(Drag::RackDots { x0: mouse.0, p0: self.state.rack.name_pct }),
            Action::RackSpeaker => self.rack_speaker(),
            Action::RackLen => self.drag = Some(Drag::RackLen { y0: mouse.1, v0: rk::len_value(&self.state) }),
            Action::RackLoop => {
                let s = &mut self.state;
                rk::relayout(s, |s| s.rack.loop_mode = !s.rack.loop_mode);
                s.piano.dirty = true;
            }
            Action::RackGraph => self.state.rack.graph_open = !self.state.rack.graph_open,
            Action::RackLoopBox(i) if self.state.rack.loop_type == rk::LOOP_ADVANCED => {
                self.state.playlist.popup = Some(rk::adv_loop_menu(&self.state, i, mouse.0, mouse.1 + 8.0));
            }
            Action::RackLoopBox(i) => self.drag = Some(Drag::RackLoopLen { row: i, y0: mouse.1, v0: rk::row_len(&self.state, i) }),
            Action::Rack(r) => self.rack_click(r),
            Action::RackGraphArea => {
                self.rack_graph_at(mouse);
                self.drag = Some(Drag::RackGraph);
            }
            Action::RackGraphTab(i) => self.state.rack.graph_tab = rk::Tab::ALL[i.min(rk::Tab::ALL.len() - 1)],
            Action::RackFill(h) => rk::fill_hit(&mut self.state, h),
            _ => return false,
        }
        self.invalidate();
        true
    }

    /// The rack's own controls (`Action::Rack`): the layer window, the preview mute buttons, the Note tab's octaves.
    fn rack_click(&mut self, a: crate::rack::RAct) {
        use crate::rack as rk;
        let s = &mut self.state;
        match a {
            rk::RAct::LayerChild(l, c) => rk::toggle_child(s, l, c),
            rk::RAct::LayerFlag(l, f) => rk::toggle_layer_flag(s, l, f),
            rk::RAct::SetChildren(l) => rk::set_children(s, l),
            rk::RAct::PreviewMute(r) => rk::toggle_mute(s, r),
            rk::RAct::KeyOctave(d) => {
                s.rack.key_lo = (i32::from(s.rack.key_lo) + 12 * d).clamp(0, i32::from(rk::KEY_MAX - rk::KEY_SPAN)) as u16;
                self.hint = Cow::Owned(format!("Note range {} - {}", rk::key_name(s.rack.key_lo), rk::key_name(s.rack.key_lo + rk::KEY_SPAN)));
            }
        }
    }

    /// Channel rack right clicks (spec: ui/channel-rack.md §8-19). Returns whether `a` was one.
    pub(crate) fn rack_right(&mut self, a: &Action) -> bool {
        use crate::knobs::KnobId;
        use crate::rack as rk;
        let (x, y) = self.mouse;
        let s = &self.state;
        let popup = match *a {
            Action::ChannelSelect(i) => rk::channel_menu(s, i, x, y),
            Action::ChannelEnable(i) => rk::led_menu(s, i, x, y),
            Action::RackMixer(i) => rk::mixer_menu(s, i, x, y),
            Action::Knob(k @ (KnobId::ChanVol(_) | KnobId::ChanPan(_) | KnobId::Swing)) => rk::knob_menu(s, k, self.automation_target(a), x, y),
            Action::RackFilter => rk::filter_edit_menu(s, x, y),
            Action::RackLen => rk::length_menu(s, x, y),
            Action::RackLoop => rk::loop_menu(s, x, y),
            Action::RackLoopBox(i) if s.rack.loop_type == rk::LOOP_ADVANCED => rk::adv_loop_menu(s, i, x, y),
            Action::RackSelect(i) => {
                let sel = &mut self.state.rack.sel;
                if !sel.remove(&i) {
                    sel.insert(i);
                }
                return true;
            }
            Action::Step(i, k) => {
                self.rack_step(i, k);
                return true;
            }
            Action::RackGraphArea => {
                self.rack_graph_at((x, y));
                self.drag = Some(Drag::RackGraph);
                return true;
            }
            _ => return false,
        };
        self.state.playlist.popup = Some(popup);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new project's channel name, "Lemon Sampler", fits the 100 % name button at every UI scale: the label keeps
    /// 4 px on each side (§17). Runs with the platform's text engine (GDI's Cuprum on Windows, Barlow on Linux).
    #[test]
    fn default_name_button_shows_lemon_sampler_whole() {
        let app = App::new(None);
        let w = crate::views::rack::cols(&app.state).name_w;
        assert_eq!(w, NAME_W);
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let mut c = os_ui::Canvas::new(64, 64, scale).expect("canvas");
            let text = c.text_width(os_ui::Font::new(14), crate::plugins::LEMON_SAMPLER);
            assert!(text <= w - 8.0, "{text} px at scale {scale}");
        }
    }

    /// Clone, move and delete renumber the channels' notes so each note keeps its channel (§17-18).
    #[test]
    fn restructuring_keeps_notes_with_their_channel() {
        let mut app = App::new(None);
        let n = app.state.project.channels.len();
        assert!(n >= 3);
        let name = |app: &App, i: usize| app.state.project.channels[i].name.clone();
        let (n1, n2) = (name(&app, 1), name(&app, 2));
        app.state.project.patterns[0].notes = vec![os_model::Note::step(2, 0)];
        clone_rows(&mut app, &[2]);
        assert_eq!(app.state.project.channels.len(), n + 1);
        assert_eq!(app.state.project.channels[3].rack.origin, app.state.project.channels[2].rack.origin, "a clone keeps its source block");
        app.state.rack.select_only(2);
        move_selected(&mut app, -1);
        assert_eq!(name(&app, 1), n2);
        assert!(app.state.project.patterns[0].notes.iter().any(|x| x.channel == 1), "the note moved with its channel");
        delete_rows(&mut app, &[1]);
        assert_eq!(app.state.project.channels.len(), n);
        assert_eq!(name(&app, 1), n1);
        assert!(app.state.project.patterns[0].notes.iter().all(|x| usize::from(x.channel) < n));
    }

    /// TOOLS > Select unused channels and Purge unused audio clips (spec: ui/unused-channels.md): unused = no notes in any
    /// pattern and no playlist clip; purge takes only the unused audio-clip channels, after the delete confirmation.
    #[test]
    fn select_unused_and_purge_audio_clips() {
        use crate::views::menus::{item_at, works, TOOLS_MENU};
        use os_model::Clip;
        use winit::keyboard::{Key, NamedKey};
        let mut app = App::new(None);
        // Rows: 0 sampler with a note, 1 sampler unused, 2 audio clip with a note only, 3 audio clip with a playlist
        // clip, 4 audio clip unused.
        for r in 2..5 {
            app.state.project.channels[r].kind = ChannelKind::AudioClip;
        }
        let idx = |app: &App, r: usize| app.state.project.channels[r].index;
        let (i0, i2, i3) = (idx(&app, 0), idx(&app, 2), idx(&app, 3));
        crate::patterns::pat_mut(&mut app.state, 2).notes = vec![Note::step(i0, 0), Note::step(i2, 3)];
        crate::playlist::arr_mut(&mut app.state).clips.push(Clip::new(0, 384, 0, ClipSource::Channel(i3)));
        let tools = |label: &str| (0..80).find(|&i| item_at(TOOLS_MENU, i).is_some_and(|it| it.label == label)).unwrap();
        for label in ["Select unused channels", "Purge unused audio clips"] {
            assert!(works(TOOLS_MENU, item_at(TOOLS_MENU, tools(label)).unwrap()), "{label} is not greyed out");
        }

        app.menu_item(TOOLS_MENU, tools("Select unused channels"));
        assert_eq!(app.state.rack.sel, BTreeSet::from([1, 4]));
        assert_eq!(app.state.message.as_deref(), Some("2 unused channel(s) selected"));

        // Purge asks first (no undo for channels yet); Cancel keeps everything.
        let names = |app: &App| app.state.project.channels.iter().map(|c| c.name.clone().unwrap_or_default()).collect::<Vec<_>>();
        let before = names(&app);
        app.menu_item(TOOLS_MENU, tools("Purge unused audio clips"));
        assert_eq!(app.state.rack.pending_delete, Some(vec![4]));
        assert!(app.state.rack.purging && app.state.pat_ui.confirm);
        app.key(&Key::Named(NamedKey::Escape), false, false);
        assert_eq!(names(&app), before);
        // Ok deletes only the unused audio-clip channel.
        assert_eq!(run(&mut app, RCmd::Purge), None);
        app.key(&Key::Named(NamedKey::Enter), false, false);
        assert_eq!(names(&app), before[..4].to_vec());
        assert!(!app.state.pat_ui.confirm);
        // Nothing left to purge: said, not asked.
        assert_eq!(run(&mut app, RCmd::Purge).as_deref(), Some("No unused audio clips"));
        assert!(!app.state.pat_ui.confirm);
        // The usual delete confirmation is worded as before.
        ask_delete(&mut app.state, vec![1]);
        assert!(!app.state.rack.purging);
    }

    /// Lock, zip, groups, swing, loop lengths and a restructured rack survive saving into an opened file ("Saving").
    #[test]
    fn rack_settings_save_and_reopen() {
        let path = std::env::temp_dir().join(format!("os_rack_save_{}.flp", std::process::id()));
        let mut app = App::new(None);
        crate::patterns::pat_mut(&mut app.state, 1).notes = vec![os_model::Note::step(2, 0), os_model::Note::step(0, 1)];
        app.save_to(path.clone());
        app.open(path.clone());
        let n = app.state.project.channels.len();
        let names: Vec<Option<String>> = app.state.project.channels.iter().map(|c| c.name.clone()).collect();
        // Clone channel 0, delete channel 1, then set the rack state on the new rows.
        clone_rows(&mut app, &[0]);
        delete_rows(&mut app, &[2]);
        let s = &mut app.state;
        s.project.swing = Some(64);
        s.rack.locked = [1].into();
        s.rack.zipped = [2].into();
        s.rack.groups = vec![Group { name: "Drums".into(), members: [0, 2].into() }];
        s.rack.filter = Filter::Group(0);
        s.rack.loop_len = [(0, 5)].into();
        s.rack.loop_mode = true;
        app.save_to(path.clone());
        assert!(app.state.message.as_deref().is_some_and(|m| m.starts_with("Saved")), "{:?}", app.state.message);
        app.open(path.clone());
        let _ = std::fs::remove_file(&path);
        let s = &app.state;
        let p = &s.project;
        assert_eq!(p.channels.len(), n);
        assert_eq!(p.channels[0].name, names[0]);
        assert_eq!(p.channels[2].name, names[2]);
        assert!(p.channels.iter().enumerate().all(|(i, c)| usize::from(c.index) == i));
        assert_eq!(p.swing, Some(64));
        let mut notes: Vec<u16> = p.patterns[0].notes.iter().map(|x| x.channel).collect();
        notes.sort_unstable();
        assert_eq!(notes, vec![0, 2], "the clone has no notes; channel 2 moved back to row 2");
        assert_eq!(s.rack.locked, [1].into());
        assert_eq!(s.rack.zipped, [2].into());
        assert_eq!(s.rack.groups.len(), 1);
        assert_eq!(s.rack.groups[0].members, [0, 2].into());
        assert_eq!(s.rack.filter, Filter::Group(0));
        assert_eq!(s.rack.loop_len, [(0, 5)].into());
        assert!(s.rack.loop_mode);
    }

    /// The popup entries this package made work are live (not greyed, not "not available yet").
    fn live(items: &[PItem], label: &str) -> PItem {
        fn find(items: &[PItem], label: &str) -> Option<PItem> {
            items.iter().find_map(|i| if i.label == label { Some(i.clone()) } else { find(&i.children, label) })
        }
        let i = find(items, label).unwrap_or_else(|| panic!("no entry {label}"));
        assert!(i.enabled && i.cmd != Cmd::Nothing, "{label} is greyed out");
        i
    }

    fn names(app: &App) -> Vec<String> {
        app.state.project.channels.iter().map(|c| c.name.clone().unwrap_or_default()).collect()
    }

    /// Options and channel menu entries that used to be placeholders now run commands (§17-18).
    #[test]
    fn rack_menus_have_no_placeholders_left_for_these() {
        let mut app = App::new(None);
        app.state.rack.sel = [0, 1].into();
        app.state.rack.loop_mode = true;
        let o = options_menu(&app.state, 0.0, 0.0).items;
        for label in ["Create layer with selected channels", "All...", "0%", "50%", "Custom...", "Burn selected loops to pattern",
            "Out only (no bleeding)", "Crossfade (bleeding)", OPT_AUTO_FILTER, OPT_FOCUS, OPT_MUTE_REMOVED, OPT_COMPLETE, OPT_PREVIEW_MUTE] {
            live(&o, label);
        }
        assert_eq!(live(&o, "Custom...").cmd, Cmd::Rack(RCmd::SwingMix(None)));
        let c = channel_menu(&app.state, 1, 0.0, 0.0).items;
        for label in ["Cut itself", "Reverse", "Randomize", "Shift left", "Shift right"] {
            live(&c, label);
        }
        let replace = c.iter().find(|i| i.label == "Replace").expect("Replace");
        assert!(replace.children.iter().all(|i| !matches!(i.cmd, Cmd::Plugin(_))), "a pick replaces, never adds");
        assert_eq!(live(&loop_menu(&app.state, 0.0, 0.0).items, "Advanced looping").cmd, Cmd::Rack(RCmd::LoopType(LOOP_ADVANCED)));
        // Custom channel button width.
        run(&mut app, RCmd::NameWidthCustom);
        assert_eq!(app.state.pat_ui.name_box.as_ref().map(|b| b.target), Some(crate::patterns::BoxTarget::Rack(RBox::NameWidth)));
        box_accept(&mut app.state, RBox::NameWidth, "150%");
        assert_eq!(app.state.rack.name_pct, 150);
        assert!(options_menu(&app.state, 0.0, 0.0).items.iter().flat_map(|i| i.children.iter()).any(|i| i.label == "Custom (150%)..."));
    }

    /// Create layer with selected channels: the layer's children are the selection; its window edits them and the
    /// Random / Crossfade / In turn settings, and all of it saves and reopens (events 94 / 144).
    #[test]
    fn layers_are_created_edited_saved_and_reopened() {
        use os_model::LayerSettings as L;
        let path = std::env::temp_dir().join(format!("os_rack_layer_{}.flp", std::process::id()));
        let mut app = App::new(None);
        app.save_to(path.clone());
        app.open(path.clone());
        let n = app.state.project.channels.len();
        app.state.rack.sel = [0, 2].into();
        assert_eq!(run(&mut app, RCmd::CreateLayer).as_deref(), Some("Layered 2 channel(s)"));
        let l = n;
        let s = &app.state;
        assert_eq!(s.project.channels[l].kind, ChannelKind::Layer);
        assert_eq!(s.project.channels[l].layer_children, vec![0, 2]);
        assert_eq!((s.selected_channel, s.rack.sel.clone()), (l, [l].into()));
        // The layer window (a plain channel window: layers host nothing).
        app.toggle_channel_window(l);
        assert!(app.state.shown(Win::Plugin) && app.state.plugin_channel == l);
        app.act(Action::Rack(RAct::LayerChild(l, 1)));
        app.act(Action::Rack(RAct::LayerChild(l, 0)));
        app.act(Action::Rack(RAct::LayerFlag(l, L::RANDOM)));
        app.act(Action::Rack(RAct::LayerFlag(l, L::CROSSFADE)));
        let c = &app.state.project.channels[l];
        assert_eq!((c.layer_children.clone(), c.misc.layer.flags), (vec![1, 2], L::RANDOM | L::CROSSFADE));
        // In turn and Random exclude each other.
        app.act(Action::Rack(RAct::LayerFlag(l, L::IN_TURN)));
        assert_eq!(app.state.project.channels[l].misc.layer.flags, L::IN_TURN | L::CROSSFADE);
        if let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) {
            app.paint(&mut c, None);
            assert!(app.hits.iter().any(|h| h.action == Action::Rack(RAct::LayerChild(l, 3))), "the layer window lists the channels");
            assert!(app.hits.iter().any(|h| h.action == Action::Rack(RAct::SetChildren(l))));
        }
        app.save_to(path.clone());
        app.open(path.clone());
        let c = &app.state.project.channels[l];
        assert_eq!((c.kind, c.layer_children.clone(), c.misc.layer.flags), (ChannelKind::Layer, vec![1, 2], L::IN_TURN | L::CROSSFADE));
        // Edit the reopened layer (the file's own block): Set children from the rack selection, a flag off.
        app.state.rack.sel = [3, 4, l].into();
        app.act(Action::Rack(RAct::SetChildren(l)));
        app.act(Action::Rack(RAct::LayerFlag(l, L::CROSSFADE)));
        app.save_to(path.clone());
        app.open(path.clone());
        let _ = std::fs::remove_file(&path);
        let c = &app.state.project.channels[l];
        assert_eq!((c.layer_children.clone(), c.misc.layer.flags), (vec![3, 4], L::IN_TURN));
        assert_eq!(app.state.project.channels.len(), n + 1);
    }

    /// ADD > Audio clip / Automation clip / Layer work through the registry and add those channels.
    #[test]
    fn add_menu_misc_channels() {
        use crate::views::menus::{item_at, works, ADD_MENU};
        let mut app = App::new(None);
        let n = app.state.project.channels.len();
        let idx = |label: &str| (0..80).find(|&i| item_at(ADD_MENU, i).is_some_and(|it| it.label == label)).unwrap();
        for (k, (label, kind)) in [("Audio clip", ChannelKind::AudioClip), ("Automation clip", ChannelKind::Automation), ("Layer", ChannelKind::Layer)].into_iter().enumerate() {
            assert!(works(ADD_MENU, item_at(ADD_MENU, idx(label)).unwrap()), "{label} is greyed out");
            app.menu_item(ADD_MENU, idx(label));
            assert_eq!(app.state.project.channels.len(), n + k + 1, "{label}");
            assert_eq!(app.state.project.channels[n + k].kind, kind);
            assert_eq!(app.state.selected_channel, n + k);
        }
        let auto = app.state.project.channels[n + 1].index;
        assert!(app.state.project.automations.iter().any(|a| a.channel == auto && a.targets.is_empty()));
        assert_eq!(names(&app)[n..].to_vec(), vec!["Audio clip".to_string(), "Automation Clip".into(), "Layer".into()]);
        // A second layer gets its own name; all of them save.
        app.menu_item(ADD_MENU, idx("Layer"));
        assert_eq!(names(&app).last().map(String::as_str), Some("Layer #2"));
        let path = std::env::temp_dir().join(format!("os_rack_add_{}.flp", std::process::id()));
        app.save_to(path.clone());
        assert!(app.state.message.as_deref().is_some_and(|m| m.starts_with("Saved")), "{:?}", app.state.message);
        app.open(path.clone());
        let _ = std::fs::remove_file(&path);
        assert_eq!(app.state.project.channels.len(), n + 4);
        assert_eq!(app.state.project.channels[n + 2].kind, ChannelKind::Layer);
    }

    /// Cut itself (§18) sets cut = cut by, as the Misc tab's Cut self; again turns it off.
    #[test]
    fn cut_itself_toggles() {
        let mut app = App::new(None);
        app.state.project.channels[1].misc.cut = 3;
        run(&mut app, RCmd::CutSelf(1));
        let m = app.state.project.channels[1].misc;
        assert_eq!((m.cut, m.cut_by), (3, 3));
        assert!(channel_menu(&app.state, 1, 0.0, 0.0).items.iter().any(|i| i.label == "Cut itself" && i.mark == Mark::Check(true)));
        run(&mut app, RCmd::CutSelf(1));
        let m = app.state.project.channels[1].misc;
        assert_eq!((m.cut, m.cut_by), (0, 0));
        run(&mut app, RCmd::CutSelf(0));
        assert_eq!((app.state.project.channels[0].misc.cut, app.state.project.channels[0].misc.cut_by), (1, 1));
    }

    /// Set swing mix and declicking for the selected channels (§17).
    #[test]
    fn swing_mix_and_declick_for_selected() {
        let mut app = App::new(None);
        app.state.rack.sel = [1, 3].into();
        run(&mut app, RCmd::SwingMix(Some(25)));
        let sw = |app: &App, r: usize| app.state.project.channels[r].misc.swing;
        assert_eq!((sw(&app, 1), sw(&app, 3), sw(&app, 0)), (0.25, 0.25, 1.0));
        assert!(options_menu(&app.state, 0.0, 0.0).items.iter().flat_map(|i| i.children.iter()).any(|i| i.label == "25%" && i.mark == Mark::Radio(true)));
        run(&mut app, RCmd::SwingMix(None));
        box_accept(&mut app.state, RBox::SwingMix, "60%");
        assert_eq!(sw(&app, 3), 0.6);
        run(&mut app, RCmd::Declick(5));
        let d = |app: &App, r: usize| app.state.project.channels[r].sound.declick;
        assert_eq!((d(&app, 1), d(&app, 3), d(&app, 0)), (5, 5, 0));
    }

    /// Color selected > All: one colour for every selected channel.
    #[test]
    fn color_all_selected() {
        let mut app = App::new(None);
        app.state.rack.sel = [0, 2].into();
        run(&mut app, RCmd::ColorAll);
        assert_eq!(app.state.pat_ui.name_box.as_ref().map(|b| b.target), Some(crate::patterns::BoxTarget::Rack(RBox::ColorAll)));
        box_accept(&mut app.state, RBox::ColorAll, "#3366cc");
        let c = Some(Rgb::new(0x33, 0x66, 0xcc));
        assert_eq!((app.state.project.channels[0].color, app.state.project.channels[2].color), (c, c));
        assert_ne!(app.state.project.channels[1].color, c);
        box_accept(&mut app.state, RBox::ColorAll, "nonsense");
        assert_eq!(app.state.project.channels[0].color, c);
    }

    /// Replace: the new generator takes the row with the old channel's notes, mixer track and colour.
    #[test]
    fn replace_keeps_the_row() {
        let mut app = App::new(None);
        app.plugins = crate::plugins::Plugins::builtins();
        let index = app.plugins.index_of(crate::osc3::LEMON_3X_OSC).expect("built in");
        let n = app.state.project.channels.len();
        let i1 = app.state.project.channels[1].index;
        crate::patterns::pat_mut(&mut app.state, 1).notes = vec![Note::step(i1, 0), Note::step(i1, 96)];
        app.state.project.channels[1].mixer_track = 7;
        app.state.project.channels[1].color = Some(Rgb::new(1, 2, 3));
        let old = names(&app);
        run(&mut app, RCmd::Replace(1, index));
        let s = &app.state;
        assert_eq!(s.project.channels.len(), n);
        assert_eq!(names(&app)[1], crate::osc3::LEMON_3X_OSC);
        assert_eq!((s.project.channels[1].kind, s.pending_plugin), (ChannelKind::Plugin, Some((1, index))));
        assert_eq!((names(&app)[0].clone(), names(&app)[2].clone()), (old[0].clone(), old[2].clone()));
        let c = &s.project.channels[1];
        assert_eq!((c.mixer_track, c.color), (7, Some(Rgb::new(1, 2, 3))));
        assert_eq!(s.project.patterns[0].notes.iter().filter(|x| x.channel == c.index).count(), 2);
        assert!(s.project.channels.iter().enumerate().all(|(i, c)| usize::from(c.index) == i));
    }

    /// Reverse, Randomize and Shift (channel menu).
    #[test]
    fn reverse_randomize_shift() {
        let mut app = App::new(None);
        let ps = per_step(&app.state);
        set_len(&mut app.state, Some(16));
        for k in [0, 1, 5] {
            set_step(&mut app.state, 2, k, true);
        }
        let steps = |app: &App| row_steps(&app.state, 2, 16).iter().enumerate().filter(|(_, b)| **b).map(|(k, _)| k).collect::<Vec<_>>();
        run(&mut app, RCmd::Reverse(2));
        assert_eq!(steps(&app), vec![10, 14, 15]);
        run(&mut app, RCmd::Shift(2, 1));
        assert_eq!(steps(&app), vec![11, 15]);
        assert!(row_notes(&app.state, 2).iter().any(|n| n.position == 16 * ps), "shifted past the end, kept");
        run(&mut app, RCmd::Shift(2, -1));
        run(&mut app, RCmd::Randomize(2));
        assert!(row_notes(&app.state, 2).iter().all(|n| n.position < 16 * ps && n.position % ps == 0));
    }

    /// Mute removed steps (§17): a step turned off keeps its note, muted, with its values; turned on again it comes
    /// back as it was.
    #[test]
    fn mute_removed_steps_keeps_the_note() {
        let mut app = App::new(None);
        set_step(&mut app.state, 0, 3, true);
        let ch = app.state.project.channels[0].index;
        crate::patterns::pat_mut(&mut app.state, 1).notes.iter_mut().for_each(|n| n.velocity = 20);
        run(&mut app, RCmd::Opt(OPT_MUTE_REMOVED));
        assert!(!toggle_step(&mut app.state, 0, 3));
        let notes = row_notes(&app.state, 0);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].muted() && notes[0].velocity == 20);
        assert!(!step_on(&app.state.project.patterns[0].notes, ch, 3, per_step(&app.state)));
        assert!(toggle_step(&mut app.state, 0, 3));
        let notes = row_notes(&app.state, 0);
        assert_eq!(notes.len(), 1);
        assert!(!notes[0].muted() && notes[0].velocity == 20);
        // Off: removed steps go.
        run(&mut app, RCmd::Opt(OPT_MUTE_REMOVED));
        toggle_step(&mut app.state, 0, 3);
        assert!(row_notes(&app.state, 0).is_empty());
    }

    /// Advanced looping (§15): each channel loops at its last used step rounded up to its loop point.
    #[test]
    fn advanced_looping_rounds_to_the_loop_point() {
        let mut app = App::new(None);
        set_len(&mut app.state, Some(32));
        set_step(&mut app.state, 1, 5, true);
        set_step(&mut app.state, 2, 2, true);
        let s = &mut app.state;
        s.rack.loop_mode = true;
        run(&mut app, RCmd::LoopType(LOOP_ADVANCED));
        assert_eq!((row_len(&app.state, 1), row_len(&app.state, 2)), (16, 16), "Bar by default");
        run(&mut app, RCmd::AdvLoop(1, AdvLoop::Beat));
        run(&mut app, RCmd::AdvLoop(2, AdvLoop::Step));
        run(&mut app, RCmd::AdvLoop(0, AdvLoop::Off));
        assert_eq!((row_len(&app.state, 1), row_len(&app.state, 2), row_len(&app.state, 0)), (8, 3, 32));
        let p = play_project(&app.state);
        let ps = per_step(&app.state);
        let ch2 = app.state.project.channels[2].index;
        let mut at: Vec<u32> = p.patterns[0].notes.iter().filter(|n| n.channel == ch2).map(|n| n.position / ps).collect();
        at.sort_unstable();
        assert_eq!(at, (0..10).map(|k| 2 + 3 * k).collect::<Vec<u32>>());
        // The loop box picks the loop point.
        app.state.rack_pos = (100.0, 100.0);
        app.mouse = (300.0, 300.0);
        assert!(app.rack_act(&Action::RackLoopBox(1)));
        assert!(app.state.playlist.popup.as_ref().is_some_and(|p| p.items.iter().any(|i| i.label == "Beat" && i.mark == Mark::Radio(true))));
        // Burn writes the loops.
        app.state.rack.sel = [2].into();
        run(&mut app, RCmd::Burn(false));
        assert_eq!(row_notes(&app.state, 2).len(), 10);
    }

    /// View options: auto switch display filter, focus on playback, the piano roll preview's mute button, the Note
    /// tab's octaves.
    #[test]
    fn view_options_work() {
        let mut app = App::new(None);
        // Auto switch display filter: a channel added while a group is shown brings Unsorted up.
        app.state.rack.groups = vec![Group { name: "Drums".into(), members: [0].into() }];
        app.state.rack.filter = Filter::Group(0);
        run(&mut app, RCmd::Opt(OPT_AUTO_FILTER));
        let r = add_layer(&mut app).map(|_| app.state.selected_channel).unwrap();
        assert_eq!(app.state.rack.filter, Filter::Unsorted);
        assert!(in_filter(&app.state, r));
        // Focus selected channel on playback.
        app.state.rack.filter = Filter::All;
        app.state.rack_scroll = 500.0;
        app.state.rack.sel = [0].into();
        run(&mut app, RCmd::Opt(OPT_FOCUS));
        focus_on_play(&mut app.state);
        assert_eq!(app.state.rack_scroll, 0.0);
        // Preview mute button.
        app.act(Action::Rack(RAct::PreviewMute(1)));
        assert!(!app.state.project.channels[1].enabled);
        // Note tab: an octave up moves the span; the graph edits keys in it.
        app.act(Action::Rack(RAct::KeyOctave(1)));
        assert_eq!(app.state.rack.key_lo, KEY_LO + 12);
        app.state.selected_channel = 0;
        set_step(&mut app.state, 0, 0, true);
        app.state.rack.graph_tab = Tab::Note;
        graph_set(&mut app.state, 0, 1.0);
        assert_eq!(row_notes(&app.state, 0)[0].key, KEY_HI + 12);
        for _ in 0..20 {
            app.act(Action::Rack(RAct::KeyOctave(1)));
        }
        assert_eq!(app.state.rack.key_lo, KEY_MAX - KEY_SPAN);
        if let Ok(mut c) = os_ui::Canvas::new(1920, 1080, 1.0) {
            app.state.rack_roll_view = true;
            app.state.rack.graph_open = true;
            // An incomplete preview fits the visible notes' keys; the others still draw (clipped).
            let ps = per_step(&app.state);
            let i0 = app.state.project.channels[0].index;
            crate::patterns::pat_mut(&mut app.state, 1).notes.extend([Note { key: 90, ..Note::step(i0, 300 * ps) }, Note { key: 20, ..Note::step(i0, 400 * ps) }]);
            run(&mut app, RCmd::Opt(OPT_COMPLETE));
            assert!(!app.state.rack.opts.contains(OPT_COMPLETE));
            app.state.raise(Win::ChannelRack);
            app.paint(&mut c, None);
            assert!(app.hits.iter().any(|h| h.action == Action::Rack(RAct::PreviewMute(0))), "the preview has its mute button");
            assert!(app.hits.iter().any(|h| h.action == Action::Rack(RAct::KeyOctave(-1))));
            assert!(!app.hits.iter().any(|h| matches!(h.action, Action::Placeholder(n) if n.contains("piano roll preview"))));
        }
    }

    #[test]
    fn euclid_spreads_hits() {
        let e = euclid(16, 4);
        assert_eq!(e.iter().filter(|&&b| b).count(), 4);
        assert!(e[0] && e[4] && e[8] && e[12]);
        let e = euclid(8, 3);
        assert_eq!(e.iter().filter(|&&b| b).count(), 3);
        assert!(e[0]);
    }

    #[test]
    fn graph_values_round_trip() {
        let mut n = Note::step(0, 96);
        for t in Tab::ALL {
            t.set_in(&mut n, 0.5, 24, KEY_LO);
            assert!((t.get(&n, 24) - 0.5).abs() < 0.08, "{t:?}: {}", t.get(&n, 24));
        }
        assert_eq!(n.position / 24, 4, "time shift stays inside its step");
        assert_eq!(Tab::Vel.text(&Note::step(0, 0), 24), "Velocity 78%");
        assert_eq!(key_name(60), "C5");
    }
}
