//! Mouse input in the piano roll: what a press on the grid, a note, the event panel or the keyboard does with the
//! tool in use, the drags that follow (move, resize, erase, mute, paint, marquee, slice, zoom box, scrub, velocity and
//! control points, the keyboard, the event panel's reset and straight lines, time markers) and the helpers they share.
//! `App::mouse_down` / `mouse_move` / `mouse_up` route the piano roll's actions and drags here. Alt held while clicking
//! or dragging bypasses the snap.
// spec: ui/piano-roll.md §7a (event panel), §8 (Draw tool), §9 (other tools), §10a (time markers), §11

use std::time::Instant;

use os_model::{ControlPoint, Note};
use os_ui::Rect;

use crate::app::{resize_zone, Action, App, Drag, State};
use crate::piano::{self, Geom, Overlay, PianoState, Tool};
use crate::views;

/// A double click: a second press within this many milliseconds (Lemon Studio choice, as the other double clicks).
const DOUBLE_MS: u128 = 400;

/// The piano roll's drags that live in this module (`Drag::Piano`).
#[derive(Debug, Clone)]
pub enum PDrag {
    /// A time marker's tab moved along the ruler (§10a).
    Marker { i: usize },
    /// Right-drag over a note lane: the notes passed get the lane's default value (§7a).
    LaneReset { last_x: f32 },
    /// Alt+drag in the event panel: a straight line from the press (x0, y0); `notes` / `controls` are the pattern's
    /// before the drag, put back before each new line (§7a).
    LaneLine { x0: f32, y0: f32, notes: Vec<Note>, controls: Vec<ControlPoint> },
    /// Paint and add: a note `len` long in every slot the mouse enters, on the row under it.
    PaintAdd { len: u32 },
}

impl App {
    /// Left click in the piano roll (spec: ui/piano-roll.md §8 Draw tool, §9 other tools).
    pub(crate) fn piano_click(&mut self, a: &Action) {
        let (x, y) = self.mouse;
        let Some(g) = self.piano_snap_geom() else { return };
        let Some(ch) = piano::channel(&self.state) else {
            self.state.message = Some("Piano roll: no channel selected".into());
            return;
        };
        let tool = self.state.piano.tool;
        if matches!(a, Action::PianoGrid | Action::PianoNote(_)) && self.ghost_double_click(&g, x, y) {
            return;
        }
        if matches!(a, Action::PianoGrid | Action::PianoNote(_)) && tool == Tool::PaintAdd {
            let s = &mut self.state;
            piano::snapshot(s);
            s.piano.clear_selection();
            let ppq = u32::from(s.project.ppq.max(4));
            let len = if s.piano.last_len > 0 { s.piano.last_len } else { ppq / 2 };
            self.drag = Some(Drag::Piano(PDrag::PaintAdd { len }));
            self.paint_add(len);
            return;
        }
        if matches!(a, Action::PianoGrid | Action::PianoNote(_)) && tool != Tool::Draw {
            let s = &mut self.state;
            let note = if let Action::PianoNote(i) = *a { Some(i) } else { None };
            match tool {
                Tool::Delete => {
                    piano::snapshot(s);
                    self.drag = Some(Drag::NoteErase);
                    self.erase_at(x, y);
                    return;
                }
                Tool::Mute => {
                    let Some(i) = note else { return };
                    piano::snapshot(s);
                    let on = !piano::notes(s)[i].muted();
                    self.drag = Some(Drag::NoteMute { on });
                    self.mute_at(x, y, on);
                    return;
                }
                Tool::Paint if note.is_none() => {
                    piano::snapshot(s);
                    s.piano.clear_selection();
                    let key = g.key_at(y).clamp(0, views::piano::TOP_KEY);
                    let start = g.snap_down(g.tick_at(x));
                    let i = piano::add_note(s, ch, start, key);
                    let len = piano::notes(s)[i].length;
                    s.piano.dirty = true;
                    self.drag = Some(Drag::NotePaint { key, start, len });
                    self.hold_key(Some(key), true);
                    self.damage_piano();
                    return;
                }
                Tool::Slice => {
                    self.drag = Some(Drag::Slice);
                    self.state.piano.overlay = Some(Overlay::Slice(g.snap_down(g.tick_at(x) + g.step as f32 / 2.0)));
                    self.damage_piano();
                    return;
                }
                Tool::Zoom => {
                    self.drag = Some(Drag::ZoomBox { x0: x, y0: y });
                    return;
                }
                Tool::Playback => {
                    let t = g.tick_at(x).max(0.0);
                    self.drag = Some(Drag::Scrub { tick: t });
                    self.scrub(t, t);
                    self.state.piano.overlay = Some(Overlay::Scrub(t));
                    self.damage_piano();
                    return;
                }
                Tool::Select if note.is_none() => {
                    let base = if self.ctrl { s.piano.selection(s.pattern, Some(ch)) } else { Vec::new() };
                    s.piano.select(s.pattern, ch, base.clone());
                    self.drag = Some(Drag::Marquee { x0: x, y0: y, base });
                    self.damage_piano();
                    return;
                }
                Tool::Select => {
                    // Clicking an unselected note selects it (Ctrl adds), then drags as Draw does.
                    let i = note.unwrap_or(0);
                    let mut sel = s.piano.selection(s.pattern, Some(ch));
                    if !sel.contains(&i) {
                        if !self.ctrl {
                            sel.clear();
                        }
                        sel.push(i);
                        s.piano.select(s.pattern, ch, sel);
                    }
                }
                // Paint on a note: drags that one note like Draw (no group move).
                _ => s.piano.clear_selection(),
            }
        }
        let s = &mut self.state;
        match *a {
            Action::PianoGrid => {
                piano::snapshot(s);
                s.piano.clear_selection();
                // The chord stamp (§2 helpers) places the whole chord on the clicked key; it moves as one.
                let key = g.key_at(y).clamp(0, views::piano::TOP_KEY);
                let keys = s.piano.stamp.map_or(vec![key], |c| c.keys(key, views::piano::TOP_KEY));
                let tick = g.snap_down(g.tick_at(x));
                let added: Vec<usize> = keys.iter().map(|&k| piano::add_note(s, ch, tick, k)).collect();
                let notes = piano::notes(s);
                let orig: Vec<(usize, Note)> = added.iter().map(|&i| (i, notes[i])).collect();
                let n = orig[0].1;
                s.piano.last_len = n.length;
                self.drag = Some(Drag::NoteMove { x0: x, y0: y, orig });
                self.hold_key(Some(i32::from(n.key)), true);
            }
            Action::PianoNote(i) => {
                let Some(&n) = piano::notes(s).get(i) else { return };
                // A double click opens the note's properties (§12).
                let now = Instant::now();
                if s.piano.note_click.is_some_and(|(j, t)| j == i && now.duration_since(t).as_millis() < DOUBLE_MS) {
                    s.piano.note_click = None;
                    crate::note_props::open(s, crate::note_props::Target::Note(i));
                    return;
                }
                s.piano.note_click = Some((i, now));
                piano::snapshot(s);
                let sel = s.piano.selection(s.pattern, Some(ch));
                let idx = if sel.contains(&i) && tool.moves_groups() {
                    sel
                } else {
                    s.piano.clear_selection();
                    vec![i]
                };
                let notes = piano::notes(s);
                // The clicked note first: it is the one auditioned while dragging.
                let orig: Vec<(usize, Note)> =
                    std::iter::once(i).chain(idx.into_iter().filter(|&j| j != i)).filter_map(|j| notes.get(j).map(|&m| (j, m))).collect();
                s.piano.last_len = n.length.max(g.step);
                let nr = g.note_rect(&n).unwrap_or_default();
                self.drag = Some(if x >= nr.right() - resize_zone(nr.w) {
                    self.resize_cursor = true;
                    Drag::NoteResize { x0: x, orig }
                } else {
                    self.hold_key(Some(i32::from(n.key)), true);
                    Drag::NoteMove { x0: x, y0: y, orig }
                });
            }
            Action::PianoVelocity if self.alt => {
                // Alt+drag: a straight line from here (§7a).
                piano::snapshot(s);
                let (notes, controls) = (piano::notes(s).to_vec(), piano::controls(s).to_vec());
                self.drag = Some(Drag::Piano(PDrag::LaneLine { x0: x, y0: y, notes, controls }));
                self.lane_line(x, y, x, y);
            }
            Action::PianoVelocity => {
                piano::snapshot(s);
                self.drag = Some(Drag::Velocity { last_x: x, last_y: y });
                self.paint_velocity(x, y, x, y);
            }
            Action::PianoKeys => {
                let k = g.key_at(y).clamp(0, views::piano::TOP_KEY);
                self.drag = Some(Drag::PianoKeys { key: k });
                self.hold_key(Some(k), true);
            }
            _ => {}
        }
    }

    /// A piano roll drag moved to (`x`, `y`) (`App::mouse_move`).
    pub(crate) fn piano_drag_move(&mut self, d: Drag, x: f32, y: f32) {
        match d {
            Drag::NoteMove { x0, y0, orig } => {
                self.drag_notes(x0, y0, &orig, false);
                self.state.piano.dirty = true;
            }
            Drag::NoteResize { x0, orig } => {
                self.drag_notes(x0, 0.0, &orig, true);
                self.state.piano.dirty = true;
            }
            Drag::NoteErase => self.erase_at(x, y),
            Drag::NoteMute { on } => self.mute_at(x, y, on),
            Drag::NotePaint { key, start, len } => self.paint_notes(key, start, len),
            Drag::Marquee { x0, y0, base } => {
                let r = Rect::span(x0.min(x), x0.max(x), y0.min(y), y0.max(y));
                self.state.piano.overlay = Some(Overlay::Marquee(r));
                if let Some(g) = self.piano_geom() {
                    let s = &mut self.state;
                    let mut sel = base.clone();
                    sel.extend(piano::notes_in(s, &g, r).into_iter().filter(|i| !base.contains(i)));
                    if let Some(ch) = piano::channel(s) {
                        s.piano.select(s.pattern, ch, sel);
                    }
                }
                self.damage_piano();
            }
            Drag::Slice => {
                if let Some(g) = self.piano_snap_geom() {
                    let t = g.snap_down(g.tick_at(x) + g.step as f32 / 2.0);
                    self.state.piano.overlay = Some(Overlay::Slice(t));
                }
                self.damage_piano();
            }
            Drag::ZoomBox { x0, y0 } => {
                self.state.piano.overlay = Some(Overlay::ZoomBox(Rect::span(x0.min(x), x0.max(x), y0.min(y), y0.max(y))));
                self.damage_piano();
            }
            Drag::Marker => {
                if let Some(g) = self.piano_snap_geom() {
                    self.state.piano.marker = g.snap_down(g.tick_at(x) + g.step as f32 / 2.0);
                }
                self.damage_piano();
            }
            Drag::TimeSel { t0 } => {
                if let Some(g) = self.piano_snap_geom() {
                    let t = g.snap_down(g.tick_at(x) + g.step as f32 / 2.0);
                    piano::select_range(&mut self.state, t0.min(t), t0.max(t));
                }
                self.damage_piano();
            }
            Drag::Scrub { tick } => {
                if let Some(g) = self.piano_geom() {
                    let t = g.tick_at(x).max(0.0);
                    self.scrub(tick, t);
                    self.drag = Some(Drag::Scrub { tick: t });
                    self.state.piano.overlay = Some(Overlay::Scrub(t));
                }
                self.damage_piano();
            }
            Drag::Velocity { last_x, last_y } => {
                self.state.piano.dirty = true;
                self.paint_velocity(last_x, last_y, x, y);
                self.drag = Some(Drag::Velocity { last_x: x, last_y: y });
            }
            Drag::ControlErase { last_x } => {
                self.erase_control_points(last_x, x);
                self.drag = Some(Drag::ControlErase { last_x: x });
            }
            Drag::PianoKeys { key } => {
                if let Some(g) = self.piano_geom() {
                    let k = g.key_at(y).clamp(0, views::piano::TOP_KEY);
                    if k != key {
                        self.drag = Some(Drag::PianoKeys { key: k });
                        self.hold_key(Some(k), true);
                    }
                }
            }
            Drag::Piano(p) => self.piano_pdrag_move(p, x, y),
            _ => {}
        }
    }

    /// The piano roll's geometry with the snap Alt leaves out (§8).
    pub(crate) fn piano_snap_geom(&self) -> Option<Geom> {
        self.piano_geom().map(|g| if self.alt { g.unsnapped() } else { g })
    }

    fn piano_pdrag_move(&mut self, p: PDrag, x: f32, y: f32) {
        match p {
            PDrag::Marker { i } => {
                if let Some(g) = self.piano_snap_geom() {
                    let t = g.snap_down(g.tick_at(x) + g.step as f32 / 2.0);
                    crate::piano_menu::move_marker(&mut self.state, i, t);
                }
                // A press that moved the marker is not the first half of a double click.
                self.state.piano.marker_click = None;
                self.damage_piano();
            }
            PDrag::LaneReset { last_x } => {
                self.lane_reset(last_x, x);
                self.drag = Some(Drag::Piano(PDrag::LaneReset { last_x: x }));
            }
            PDrag::LaneLine { x0, y0, notes, controls } => {
                let p = piano::pattern_mut(&mut self.state);
                p.notes.clone_from(&notes);
                p.controls.clone_from(&controls);
                self.lane_line(x0, y0, x, y);
            }
            PDrag::PaintAdd { len } => self.paint_add(len),
        }
    }

    /// The button went up on one of these drags (`App::mouse_up`).
    pub(crate) fn piano_pdrag_up(&mut self, p: PDrag) {
        match p {
            PDrag::Marker { .. } => crate::piano_menu::sort_markers(&mut self.state),
            PDrag::LaneReset { .. } | PDrag::LaneLine { .. } | PDrag::PaintAdd { .. } => piano::finish_edit(&mut self.state),
        }
        self.state.piano.dirty = true;
        self.damage_piano();
    }

    /// A right press on the event panel's note lane: the notes under the mouse (and those the drag passes) get the
    /// lane's default back (§7a).
    pub(crate) fn piano_lane_right(&mut self) {
        let x = self.mouse.0;
        piano::snapshot(&mut self.state);
        self.drag = Some(Drag::Piano(PDrag::LaneReset { last_x: x }));
        self.lane_reset(x, x);
    }

    /// This channel's notes starting between two x positions (±3 px, the lollipop's width), as (index, x).
    fn lane_notes(&self, g: &Geom, a: f32, b: f32) -> Vec<(usize, f32)> {
        let ch = piano::channel(&self.state);
        let (a, b) = (a.min(b) - 3.0, a.max(b) + 3.0);
        piano::notes(&self.state)
            .iter()
            .enumerate()
            .filter(|(_, n)| Some(n.channel) == ch)
            .map(|(i, n)| (i, g.x_of(n.position as f32)))
            .filter(|&(_, nx)| nx >= a && nx <= b)
            .collect()
    }

    fn lane_reset(&mut self, last_x: f32, x: f32) {
        let Some(g) = self.piano_geom() else { return };
        let lane = self.state.piano.lane;
        let hit = self.lane_notes(&g, last_x, x);
        let p = piano::pattern_mut(&mut self.state);
        for &(i, _) in &hit {
            lane.set(&mut p.notes[i], lane.default_value());
        }
        if !hit.is_empty() {
            self.state.piano.dirty = true;
            self.damage_piano();
        }
    }

    /// Alt+drag in the event panel (§7a): the lane's values along the straight line from (x0, y0) to (x1, y1), for the
    /// notes starting between them, or a control point every snap step in a channel lane.
    fn lane_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let (Some(g), Some(panel)) = (self.piano_geom(), self.piano_panel) else { return };
        let lane = self.state.piano.lane;
        let y_at = |px: f32| if (x1 - x0).abs() < 0.5 { y1 } else { y0 + (y1 - y0) * ((px - x0) / (x1 - x0)).clamp(0.0, 1.0) };
        if lane.is_channel() {
            let (Some(ch), Some(param)) = (piano::channel(&self.state), lane.param()) else { return };
            let center = self.state.piano.pitch_center;
            let t0 = g.snap_down(g.tick_at(x0.min(x1)).max(0.0));
            let t1 = g.snap_down(g.tick_at(x0.max(x1)).max(0.0));
            piano::erase_controls(&mut self.state, ch, param, t0, t1);
            let mut t = t0;
            while t <= t1 {
                let v = lane.control_at(panel, y_at(g.x_of(t as f32)), center);
                piano::set_control(&mut self.state, ch, param, t, v);
                t += g.step.max(1);
            }
        } else {
            let hit = self.lane_notes(&g, x0, x1);
            let p = piano::pattern_mut(&mut self.state);
            for (i, nx) in hit {
                lane.set(&mut p.notes[i], lane.value_at(panel, y_at(nx)));
            }
        }
        self.state.piano.dirty = true;
        self.damage_rect(panel.outset(1.0));
    }

    /// Paint and add: a note `len` long in the slot under the mouse on the row under it, unless this channel already
    /// has a note on that row overlapping the slot.
    pub(crate) fn paint_add(&mut self, len: u32) {
        let (Some(g), Some(ch)) = (self.piano_snap_geom(), piano::channel(&self.state)) else { return };
        let (x, y) = self.mouse;
        if !g.grid.contains(x, y) {
            return;
        }
        let len = len.max(1);
        let tick = g.tick_at(x).max(0.0) as u32;
        let pos = if self.alt { tick } else { tick - tick % len };
        let key = g.key_at(y).clamp(0, views::piano::TOP_KEY);
        let taken = piano::notes(&self.state).iter().any(|n| n.channel == ch && i32::from(n.key) == key && n.position < pos + len && pos < n.position + n.length.max(1));
        if taken {
            return;
        }
        let i = piano::add_note(&mut self.state, ch, pos, key);
        piano::pattern_mut(&mut self.state).notes[i].length = len;
        self.state.piano.dirty = true;
        let r = g.note_rect(&piano::notes(&self.state)[i]);
        self.hold_key(Some(key), true);
        match r {
            Some(r) => self.damage_notes(&[r]),
            None => self.damage_piano(),
        }
    }

    /// A press on a ghost note (another channel's note, shown dimmed, §5): the second press of a double click makes
    /// its channel the piano roll's, taking back the note the first press drew there. Returns true when it switched.
    fn ghost_double_click(&mut self, g: &Geom, x: f32, y: f32) -> bool {
        let s = &self.state;
        let ch = piano::channel(s);
        let ghost = if s.piano.ghosts { piano::notes(s).iter().rev().find(|n| Some(n.channel) != ch && g.note_rect(n).is_some_and(|r| r.contains(x, y))) } else { None };
        let Some(gch) = ghost.map(|n| n.channel) else {
            self.state.piano.ghost_click = None;
            return false;
        };
        let now = Instant::now();
        let count = piano::notes(s).len();
        match s.piano.ghost_click {
            Some((c, t, before)) if c == gch && now.duration_since(t).as_millis() < DOUBLE_MS => {
                self.state.piano.ghost_click = None;
                if count > before {
                    piano::undo(&mut self.state);
                }
                let Some(row) = self.state.project.channels.iter().position(|c| c.index == gch) else { return false };
                self.focus_channel(row);
                self.state.rack.select_only(row);
                let name = self.state.project.channels[row].name.clone().unwrap_or_else(|| format!("Channel {}", row + 1));
                self.state.message = Some(format!("Piano roll: {name}"));
                self.invalidate();
                true
            }
            _ => {
                self.state.piano.ghost_click = Some((gch, now, count));
                false
            }
        }
    }

    /// Move (`resize == false`) or resize the dragged notes from their originals, snapped to steps.
    pub(crate) fn drag_notes(&mut self, x0: f32, y0: f32, orig: &[(usize, Note)], resize: bool) {
        let Some(g) = self.piano_snap_geom() else { return };
        let (x, y) = self.mouse;
        let mut dt = g.snap_delta((x - x0) / g.ppt);
        let mut held = None;
        let was_held = self.state.piano.held;
        let rects = |s: &State| -> Vec<Rect> {
            let notes = piano::notes(s);
            orig.iter().filter_map(|&(i, _)| notes.get(i).and_then(|n| g.note_rect(n))).collect()
        };
        let mut dirty = rects(&self.state);
        let s = &mut self.state;
        let p = piano::pattern_mut(s);
        if resize {
            for &(i, o) in orig {
                if let Some(n) = p.notes.get_mut(i) {
                    let len = i64::from(o.length.max(g.step)) + dt;
                    n.length = len.max(i64::from(g.step)) as u32;
                }
            }
            let first = orig.first().and_then(|&(i, _)| p.notes.get(i)).map(|n| n.length);
            if let Some(len) = first {
                s.piano.last_len = len;
            }
        } else {
            let dk = (g.key_at(y) - g.key_at(y0)).clamp(-131, 131);
            // Keep every note at or after tick 0 and inside the key range.
            let min_pos = orig.iter().map(|(_, o)| i64::from(o.position)).min().unwrap_or(0);
            dt = dt.max(-min_pos);
            let lo = orig.iter().map(|(_, o)| i32::from(o.key)).min().unwrap_or(0);
            let hi = orig.iter().map(|(_, o)| i32::from(o.key)).max().unwrap_or(0);
            let dk = dk.clamp(-lo, views::piano::TOP_KEY - hi);
            // The grabbed note (first) is auditioned again whenever its key changes (§8).
            if let Some(&(_, o)) = orig.first() {
                let k = i32::from(o.key) + dk;
                if was_held != Some(k) {
                    held = Some(k);
                }
            }
            for &(i, o) in orig {
                if let Some(n) = p.notes.get_mut(i) {
                    n.position = (i64::from(o.position) + dt) as u32;
                    n.key = (i32::from(o.key) + dk) as u16;
                }
            }
        }
        dirty.extend(rects(&self.state));
        self.damage_notes(&dirty);
        if held.is_some() {
            self.hold_key(held, true);
        }
    }

    /// Set the key held with the mouse (lit on the keyboard), auditioning it if `play`.
    pub(crate) fn hold_key(&mut self, key: Option<i32>, play: bool) {
        if self.state.piano.held == key {
            return;
        }
        self.state.piano.held = key;
        if let (Some(k), true, Some(ch)) = (key, play, piano::channel(&self.state)) {
            self.preview(ch, k);
        }
        // The keyboard column.
        match self.piano_grid {
            Some(g) => self.damage_rect(Rect::new(g.x - 73.0, g.y, 73.0, g.h)),
            None => self.invalidate(),
        }
    }

    /// Damage note rects (old and new) and the velocity-panel columns under them. The overview
    /// mini-map is refreshed when the drag ends.
    pub(crate) fn damage_notes(&mut self, rects: &[Rect]) {
        let (Some(grid), Some(panel)) = (self.piano_grid, self.piano_panel) else { return self.invalidate() };
        let Some(first) = rects.first() else { return };
        let u = rects.iter().fold(*first, |a, r| a.union(r)).outset(2.0).intersect(&grid);
        if u.w > 0.0 && u.h > 0.0 {
            self.damage_rect(u);
        }
        let (x0, x1) = rects.iter().fold((f32::MAX, f32::MIN), |(a, b), r| (a.min(r.x), b.max(r.right())));
        self.damage_rect(Rect::new(x0 - 6.0, panel.y, x1 - x0 + 12.0, panel.h).intersect(&panel.outset(1.0)));
    }

    /// Delete this channel's note under (x, y), if any.
    pub(crate) fn erase_at(&mut self, x: f32, y: f32) {
        let Some(g) = self.piano_geom() else { return };
        if !g.grid.contains(x, y) {
            return;
        }
        let ch = piano::channel(&self.state);
        let hit = piano::notes(&self.state)
            .iter()
            .enumerate()
            .rev()
            .find(|(_, n)| Some(n.channel) == ch && g.note_rect(n).is_some_and(|r| r.contains(x, y)))
            .map(|(i, _)| i);
        if let Some(i) = hit {
            piano::delete(&mut self.state, vec![i]);
            self.state.piano.dirty = true;
            self.damage_piano();
        }
    }

    /// Mute tool: give the note under (x, y) the mute state `on`.
    pub(crate) fn mute_at(&mut self, x: f32, y: f32, on: bool) {
        let Some(g) = self.piano_geom() else { return };
        if !g.grid.contains(x, y) {
            return;
        }
        let Some(i) = piano::note_at(&self.state, &g, x, y) else { return };
        let p = piano::pattern_mut(&mut self.state);
        if p.notes[i].muted() != on {
            p.notes[i].set_muted(on);
            let r = g.note_rect(&p.notes[i]);
            self.state.piano.dirty = true;
            match r {
                Some(r) => self.damage_notes(&[r]),
                None => self.damage_piano(),
            }
        }
    }

    /// Paint tool: fill `key`'s row with notes `len` apart from `start` up to the mouse (either
    /// direction), skipping slots that already hold a note of this channel on that key.
    pub(crate) fn paint_notes(&mut self, key: i32, start: u32, len: u32) {
        let (Some(g), Some(ch)) = (self.piano_geom(), piano::channel(&self.state)) else { return };
        let len = len.max(1);
        let t = g.tick_at(self.mouse.0).max(0.0) as i64;
        let k = (t - i64::from(start)).div_euclid(i64::from(len));
        let (a, b) = (k.min(0), k.max(0));
        let mut added = Vec::new();
        for j in a..=b {
            let pos = i64::from(start) + j * i64::from(len);
            if pos < 0 {
                continue;
            }
            let pos = pos as u32;
            let taken = piano::notes(&self.state).iter().any(|n| n.channel == ch && i32::from(n.key) == key && n.position == pos);
            if !taken {
                let i = piano::add_note(&mut self.state, ch, pos, key);
                piano::pattern_mut(&mut self.state).notes[i].length = len;
                added.push(i);
            }
        }
        if !added.is_empty() {
            self.state.piano.dirty = true;
            let notes = piano::notes(&self.state);
            let rects: Vec<Rect> = added.iter().filter_map(|&i| g.note_rect(&notes[i])).collect();
            self.damage_notes(&rects);
        }
    }

    /// Playback tool: sound the notes the scrub line enters between ticks `from` and `to`.
    pub(crate) fn scrub(&mut self, from: f32, to: f32) {
        let Some(ch) = piano::channel(&self.state) else { return };
        let (a, b) = (from.min(to), from.max(to));
        let mut keys: Vec<i32> = piano::notes(&self.state)
            .iter()
            .filter(|n| n.channel == ch && !n.muted())
            .filter(|n| {
                let p = n.position as f32;
                let end = p + piano::shown_len(&self.state, n) as f32;
                if from == to {
                    // Press: every note the line touches sounds.
                    p <= to && to < end
                } else {
                    // Drag: notes whose start the line just passed.
                    p > a && p <= b
                }
            })
            .map(|n| i32::from(n.key))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        for k in keys {
            self.preview(ch, k);
        }
    }

    /// Zoom tool: zoom the time axis to the dragged box (a click zooms in 2x around the mouse).
    pub(crate) fn zoom_to(&mut self, x0: f32, y0: f32) {
        let Some(g) = self.piano_geom() else { return };
        let (x1, y1) = self.mouse;
        let p = &mut self.state.piano;
        let back = (p.px_per_beat, p.scroll_x, p.scroll_y);
        let ppq = f32::from(self.state.project.ppq.max(4));
        if (x1 - x0).abs() < 4.0 {
            let tick = g.tick_at(x1);
            p.px_per_beat = (p.px_per_beat * 2.0).min(1600.0);
            p.scroll_x = (tick - (x1 - g.grid.x) * ppq / p.px_per_beat).max(0.0);
        } else {
            let (t0, t1) = (g.tick_at(x0.min(x1)).max(0.0), g.tick_at(x0.max(x1)).max(0.0));
            p.px_per_beat = (g.grid.w / ((t1 - t0).max(1.0) / ppq)).clamp(6.0, 1600.0);
            p.scroll_x = t0;
            p.scroll_y = (p.scroll_y + y0.min(y1) - g.grid.y).max(0.0);
        }
        p.zoom_back = Some(back);
        piano::clamp_scroll(&mut self.state, g.grid.h);
        self.invalidate();
    }

    /// Zoom tool right-click: fully zoomed out (the normal 96 px/beat view) <-> the last zoom.
    pub(crate) fn zoom_toggle(&mut self) {
        let Some(g) = self.piano_geom() else { return };
        let p = &mut self.state.piano;
        let normal = PianoState::default().px_per_beat;
        if (p.px_per_beat - normal).abs() > 0.01 {
            p.zoom_back = Some((p.px_per_beat, p.scroll_x, p.scroll_y));
            p.px_per_beat = normal;
            p.scroll_x = 0.0;
        } else if let Some((z, sx, sy)) = p.zoom_back {
            p.zoom_back = Some((p.px_per_beat, p.scroll_x, p.scroll_y));
            p.px_per_beat = z;
            p.scroll_x = sx;
            p.scroll_y = sy;
        }
        piano::clamp_scroll(&mut self.state, g.grid.h);
        self.invalidate();
    }

    /// Where Ctrl+V pastes (§9): the playhead when it is in view, else the left edge of the view.
    pub(crate) fn paste_tick(&self) -> u32 {
        let s = &self.state;
        let Some(g) = self.piano_geom() else { return 0 };
        let ppq = f64::from(s.project.ppq.max(1));
        let head = if self.transport.playing() && !self.transport.song() {
            (self.transport.tick() * 96.0 / ppq) as f32
        } else {
            s.piano.marker as f32
        };
        let x = g.x_of(head);
        let tick = if x >= g.grid.x && x < g.grid.right() { head } else { g.scroll_x };
        g.snap_down(tick)
    }

    /// Delete the channel lane's control points between two x positions.
    pub(crate) fn erase_control_points(&mut self, last_x: f32, x: f32) {
        let (Some(g), Some(panel)) = (self.piano_geom(), self.piano_panel) else { return };
        let lane = self.state.piano.lane;
        let (Some(ch), Some(param)) = (piano::channel(&self.state), lane.param()) else { return };
        let (a, b) = (last_x.min(x), last_x.max(x));
        let from = g.snap_down(g.tick_at(a).max(0.0));
        let to = g.snap_down(g.tick_at(b).max(0.0));
        if piano::erase_controls(&mut self.state, ch, param, from, to) > 0 {
            self.damage_rect(panel);
        }
    }

    /// Set the event panel's property of this channel's notes whose start lies between the last and
    /// current x; in a channel lane, set a control point at every snap step the cursor path crosses.
    pub(crate) fn paint_velocity(&mut self, last_x: f32, last_y: f32, x: f32, y: f32) {
        let (Some(g), Some(panel)) = (self.piano_geom(), self.piano_panel) else { return };
        let lane = self.state.piano.lane;
        if lane.is_channel() {
            let (Some(ch), Some(param)) = (piano::channel(&self.state), lane.param()) else { return };
            let center = self.state.piano.pitch_center;
            let t0 = g.snap_down(g.tick_at(last_x).max(0.0));
            let t1 = g.snap_down(g.tick_at(x).max(0.0));
            let (lo, hi) = (t0.min(t1), t0.max(t1));
            let step = g.step.max(1);
            // A point is set when the cursor enters a snap step (its value is the path height at the
            // step's start, as in FL's captured ramps) and on the press; staying inside a step keeps it.
            let press = last_x == x && last_y == y;
            let mut t = lo;
            loop {
                if t != t0 || (t0 == t1 && press) {
                    let px = g.x_of(t as f32);
                    let at = if (x - last_x).abs() < 0.5 { 1.0 } else { ((px - last_x) / (x - last_x)).clamp(0.0, 1.0) };
                    let y_at = last_y + (y - last_y) * at;
                    piano::set_control(&mut self.state, ch, param, t, lane.control_at(panel, y_at, center));
                }
                if t >= hi {
                    break;
                }
                t += step;
            }
            self.damage_rect(panel);
            return;
        }
        let v = lane.value_at(panel, y);
        let (a, b) = (last_x.min(x) - 3.0, last_x.max(x) + 3.0);
        let ch = piano::channel(&self.state);
        let p = piano::pattern_mut(&mut self.state);
        let mut span: Option<(f32, f32)> = None;
        for n in p.notes.iter_mut().filter(|n| Some(n.channel) == ch) {
            let nx = g.x_of(n.position as f32);
            if nx >= a && nx <= b {
                lane.set(n, v);
                let end = g.x_of((n.position + n.length.max(g.step)) as f32);
                span = Some(span.map_or((nx, end), |(l, r)| (l.min(nx), r.max(end))));
            }
        }
        // Only the panel columns of the changed lollipops (the hint is left alone: a damage
        // union with the hint panel would cover most of the window).
        if let Some((l, r)) = span {
            self.damage_rect(Rect::new(l - 6.0, panel.y, r - l + 12.0, panel.h).intersect(&panel.outset(1.0)));
        }
    }
}
