//! Channel plugin window, opened from a channel's name button. A layer channel's window edits the layer: its
//! children and how it hands them its notes.
// spec: ui/channel-rack.md §5 (name button). FL shows the generator plugin's own window here; until
// plugins have editors Lemon Studio shows a plain settings window for the channel (Lemon Studio layout).
// Layer: audio/note-rules.md "Layer channels" (Lemon Studio layout).

use os_model::{ChannelKind, LayerSettings};
use os_ui::{Font, Rect};

use super::{ball_knob, rack, FrameInfo};
use crate::knobs::KnobId;
use crate::app::{Action, State, Ui, Win};
use crate::icons::{self, Icon};
use crate::rack::RAct;

pub const SIZE: (f32, f32) = (430.0, 200.0);
/// Layer window: a child row's pitch.
const CHILD_H: f32 = 20.0;

pub fn rect(s: &State) -> Rect {
    let layer = s.project.channels.get(s.plugin_channel).is_some_and(|c| c.kind == ChannelKind::Layer);
    // A layer lists every other channel below the header.
    let h = if layer { SIZE.1.max(84.0 + CHILD_H * s.project.channels.len().saturating_sub(1) as f32) } else { SIZE.1 };
    Rect::new(s.plugin_pos.0, s.plugin_pos.1, SIZE.0, h)
}

pub fn bounds(s: &State) -> Rect {
    rect(s).outset(1.0)
}

pub fn paint(ui: &mut Ui, s: &State, _f: &FrameInfo) {
    let t = ui.t;
    let win = rect(s);
    let Some(ch) = s.project.channels.get(s.plugin_channel) else { return };
    ui.c.fill_rect(win, t.window_border);
    ui.c.fill_rect(win.inset(1.0), t.window_body);
    ui.hit(win, Action::Swallow, "");
    ui.c.fill_rect(Rect::new(win.x + 1.0, win.y + 1.0, win.w - 2.0, 1.0), t.window_title_hi);
    let bar = Rect::span(win.x + 1.0, win.right() - 1.0, win.y + 1.0, win.y + 29.0);
    ui.hit(bar, Action::WinDrag(Win::Plugin), "Drag to move");
    let name = ch.name.clone().unwrap_or_else(|| format!("Channel {}", ch.index + 1));
    icons::draw(ui.c, Icon::Plug, Rect::new(win.x + 9.0, win.y + 7.0, 15.0, 15.0), t.glyph);
    ui.c.text_clip(Font::new(17), win.x + 30.0, win.y + 5.0, win.w - 60.0, t.text, &name);
    let close = Rect::new(win.right() - 22.0, win.y + 6.0, 16.0, 16.0);
    let hov = ui.hit(close, Action::CloseWin(Win::Plugin), "Close");
    icons::draw(ui.c, Icon::Close, close.inset(1.5), if hov { ui.t.tx(0xffffff) } else { t.glyph });

    let body = Rect::span(win.x + 5.0, win.right() - 6.0, win.y + 30.0, win.bottom() - 6.0);
    ui.c.fill_rect(body.outset(1.0), t.rack_inset_border);
    ui.c.fill_rect(body, t.rack_inset);
    if ch.kind == ChannelKind::Layer {
        layer_body(ui, s, body);
        return knobs(ui, s, body);
    }
    let kind = match ch.kind {
        ChannelKind::Sampler => "Sampler".to_string(),
        ChannelKind::Plugin => crate::project_plugins::plugin_title(ch).unwrap_or_else(|| "Plugin".into()),
        ChannelKind::Layer => "Layer".into(),
        ChannelKind::AudioClip => "Audio clip".into(),
        ChannelKind::Automation => "Automation clip".into(),
        ChannelKind::Other(n) => format!("Unknown type {n}"),
    };
    let sample = match (&ch.sample_path, ch.kind) {
        (Some(p), _) => p.rsplit(['/', '\\']).next().unwrap_or(p).to_string(),
        (None, ChannelKind::Sampler) => "(empty - plays the built-in pluck)".into(),
        (None, _) => "-".into(),
    };
    let lines = [
        ("Generator", kind),
        ("Sample", sample),
        ("Mixer track", rack::insert_name(ch.mixer_track)),
        ("Volume", format!("{:.0}%", ch.volume * 100.0)),
        ("Panning", format!("{:+.0}", ch.pan * 100.0)),
        ("Pitch", format!("{:+} cents", ch.pitch_cents)),
    ];
    let f = Font::new(15);
    for (i, (k, v)) in lines.iter().enumerate() {
        let ly = body.y + 10.0 + 22.0 * i as f32;
        ui.c.text(f, body.x + 12.0, ly, t.text_dim, k);
        ui.c.text_clip(f, body.x + 100.0, ly, body.w - 180.0, t.text, v);
    }
    knobs(ui, s, body);
}

/// Volume and panning knobs at the body's right.
fn knobs(ui: &mut Ui, s: &State, body: Rect) {
    let t = ui.t;
    let (vol, pan) = (KnobId::ChanVol(s.plugin_channel), KnobId::ChanPan(s.plugin_channel));
    ui.hit(Rect::new(body.right() - 68.0, body.y + 20.0, 28.0, 28.0), Action::Knob(vol), s.knob_text(vol));
    ball_knob(ui, body.right() - 54.0, body.y + 34.0, 12.0, s.knob(vol));
    ui.hit(Rect::new(body.right() - 68.0, body.y + 70.0, 28.0, 28.0), Action::Knob(pan), s.knob_text(pan));
    ball_knob(ui, body.right() - 54.0, body.y + 84.0, 12.0, s.knob(pan));
    ui.c.text_center(Font::new(13), Rect::new(body.right() - 84.0, body.y + 48.0, 60.0, 14.0), t.text_dim, "VOL");
    ui.c.text_center(Font::new(13), Rect::new(body.right() - 84.0, body.y + 98.0, 60.0, 14.0), t.text_dim, "PAN");
}

/// A ticked / unticked box.
fn check(ui: &mut Ui, r: Rect, on: bool, hov: bool) {
    let t = ui.t;
    os_ui::kit::sunken(ui.c, t, r, if hov { t.tempo_box.shade(0.08) } else { t.tempo_box });
    if on {
        os_ui::kit::tick(ui.c, r.inset(2.0), t.glyph);
    }
}

/// Layer channel (spec: audio/note-rules.md "Layer channels"): its children, ticked in the list of the other
/// channels, the rack selection as children, and the Random / Crossfade / In turn settings.
fn layer_body(ui: &mut Ui, s: &State, body: Rect) {
    let t = ui.t;
    let l = s.plugin_channel;
    let Some(ch) = s.project.channels.get(l) else { return };
    let f = Font::new(14);
    ui.c.text(f, body.x + 12.0, body.y + 8.0, t.text_dim, "Children");
    let mut y = body.y + 32.0;
    for (r, c) in s.project.channels.iter().enumerate().filter(|&(r, _)| r != l) {
        let row = Rect::new(body.x + 8.0, y, 200.0, CHILD_H - 2.0);
        let on = ch.layer_children.contains(&c.index);
        let name = c.name.clone().unwrap_or_else(|| format!("Channel {}", c.index + 1));
        let hov = ui.hit(row, Action::Rack(RAct::LayerChild(l, r)), if on { "Child of this layer - click to remove" } else { "Click to make it a child of this layer" });
        check(ui, Rect::new(row.x + 4.0, row.y + 3.0, 12.0, 12.0), on, hov);
        ui.c.text_clip(f, row.x + 22.0, row.y + 1.0, row.w - 26.0, if on { t.text } else { t.text_dim }, &name);
        y += CHILD_H;
    }
    let x = body.x + 222.0;
    let flags = ch.misc.layer.flags;
    let opts = [
        (LayerSettings::RANDOM, "Random", "Each note plays one child, drawn at random"),
        (LayerSettings::CROSSFADE, "Crossfade", "The children's volumes follow the crossfade position"),
        (LayerSettings::IN_TURN, "In turn", "Each note plays the next child in turn"),
    ];
    for (k, (flag, label, tip)) in opts.into_iter().enumerate() {
        let row = Rect::new(x, body.y + 8.0 + 24.0 * k as f32, 100.0, 20.0);
        let hov = ui.hit(row, Action::Rack(RAct::LayerFlag(l, flag)), tip);
        check(ui, Rect::new(row.x, row.y + 4.0, 12.0, 12.0), flags & flag != 0, hov);
        ui.c.text(f, row.x + 18.0, row.y + 2.0, t.text, label);
    }
    let br = Rect::new(x, body.y + 84.0, 100.0, 22.0);
    let hov = ui.hit(br, Action::Rack(RAct::SetChildren(l)), "The channels selected in the Channel rack become the children");
    os_ui::kit::button(ui.c, t, br, os_ui::kit::State::hover(hov));
    ui.c.text_center(f, br, t.text, "Set children");
}
