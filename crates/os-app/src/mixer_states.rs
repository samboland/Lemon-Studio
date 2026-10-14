//! Mixer track states (track menu > File > Save mixer track state as..., Insert from state): a track's settings and
//! effect slots kept under a name in the user folder (`mixer_states.txt`), shared by every project, and a new track
//! made from one.
//!
//! The file holds one `[name]` section per state with `key=value` lines: `name` (the track's own), `color`
//! (rrggbb), `enabled`, `polarity`, `swap`, `effects` (0 / 1), `volume`, `pan`, `separation` (model values), `eq`
//! (nine stored values, band by band: gain, frequency, width) and `slotN` (N 0..9: `on,mix,state in hex,plugin
//! name`). Routing is not part of a state: an inserted track sends to the master (Lemon Studio choice, as is naming
//! a state after its track instead of asking for a name).
// spec: ui/mixer.md §6.1 (File, Insert from state)

use os_model::{MixerTrack, Rgb, SlotPlugin};

use crate::app::App;
use crate::mixer::{self as mx, FxReq};

/// One saved state: its name and the track as kept (index 0, no routes).
pub type TrackState = (String, MixerTrack);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// The settings of `m` a state keeps (index and routing left out).
fn kept(m: &MixerTrack) -> MixerTrack {
    MixerTrack { index: 0, routes: Vec::new(), send_levels: Vec::new(), ..m.clone() }
}

/// The file's text for `states`.
pub fn text(states: &[TrackState]) -> String {
    let mut out = String::new();
    for (name, m) in states {
        out.push_str(&format!("[{name}]\n"));
        if let Some(n) = &m.name {
            out.push_str(&format!("name={n}\n"));
        }
        if let Some(c) = m.color {
            out.push_str(&format!("color={:02x}{:02x}{:02x}\n", c.r, c.g, c.b));
        }
        out.push_str(&format!("enabled={}\npolarity={}\nswap={}\neffects={}\n", u8::from(m.enabled), u8::from(m.polarity), u8::from(m.swap), u8::from(m.slots_enabled)));
        for (k, v) in [("volume", m.volume), ("pan", m.pan), ("separation", m.stereo_separation)] {
            if let Some(v) = v {
                out.push_str(&format!("{k}={v}\n"));
            }
        }
        let eq: Vec<String> = m.eq.iter().flatten().map(i32::to_string).collect();
        out.push_str(&format!("eq={}\n", eq.join(",")));
        for k in 0..10 {
            if let Some(p) = &m.slots[k] {
                out.push_str(&format!("slot{k}={},{},{},{}\n", u8::from(m.slot_on[k]), m.slot_mix[k], hex(&p.state), p.name));
            }
        }
    }
    out
}

/// The states in the file's text; malformed lines are skipped.
pub fn parse(text: &str) -> Vec<TrackState> {
    let mut out: Vec<TrackState> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push((name.to_string(), MixerTrack { routes: Vec::new(), ..MixerTrack::new(0) }));
            continue;
        }
        let (Some((_, m)), Some((k, v))) = (out.last_mut(), line.split_once('=')) else { continue };
        let flag = v == "1";
        match k {
            "name" => m.name = Some(v.to_string()),
            "color" if v.len() == 6 => {
                if let Ok(c) = u32::from_str_radix(v, 16) {
                    m.color = Some(Rgb::new((c >> 16) as u8, (c >> 8) as u8, c as u8));
                }
            }
            "enabled" => m.enabled = flag,
            "polarity" => m.polarity = flag,
            "swap" => m.swap = flag,
            "effects" => m.slots_enabled = flag,
            "volume" => m.volume = v.parse().ok().or(m.volume),
            "pan" => m.pan = v.parse().ok().or(m.pan),
            "separation" => m.stereo_separation = v.parse().ok().or(m.stereo_separation),
            "eq" => {
                let vals: Vec<i32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if vals.len() == 9 {
                    for (i, x) in vals.into_iter().enumerate() {
                        m.eq[i / 3][i % 3] = x;
                    }
                }
            }
            _ => {
                let Some(k) = k.strip_prefix("slot").and_then(|n| n.parse::<usize>().ok()).filter(|&n| n < 10) else { continue };
                let mut parts = v.splitn(4, ',');
                let (Some(on), Some(mix), Some(state), Some(name)) = (parts.next(), parts.next(), parts.next(), parts.next()) else { continue };
                let (Ok(mix), Some(state)) = (mix.parse::<f64>(), unhex(state)) else { continue };
                m.slot_on[k] = on == "1";
                m.slot_mix[k] = mix.clamp(0.0, 1.0);
                m.slots[k] = Some(SlotPlugin { name: name.to_string(), state });
            }
        }
    }
    out
}

#[cfg(test)]
thread_local!(static TEST_FILE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) });

/// The saved states (tests: a file of their own per test thread).
pub fn load() -> Vec<TrackState> {
    #[cfg(test)]
    {
        TEST_FILE.with(|f| parse(&f.borrow()))
    }
    #[cfg(not(test))]
    {
        os_paths::user_file("mixer_states.txt").and_then(|f| std::fs::read_to_string(f).ok()).map(|t| parse(&t)).unwrap_or_default()
    }
}

fn store(states: &[TrackState]) -> std::io::Result<()> {
    #[cfg(test)]
    {
        TEST_FILE.with(|f| *f.borrow_mut() = text(states));
        Ok(())
    }
    #[cfg(not(test))]
    {
        let dir = os_paths::user_dir().ok_or_else(|| std::io::Error::other("no user folder"))?;
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("mixer_states.txt"), text(states))
    }
}

/// The names of the saved states, in the order they were first saved.
pub fn names() -> Vec<String> {
    load().into_iter().map(|(n, _)| n).collect()
}

/// Save mixer track state as...: keeps track `n` under its name (replacing a state of that name). Hosted plugins
/// are kept with their current state, in the form a project file stores them.
pub fn save_track(app: &mut App, n: u16) -> Option<String> {
    let name = mx::name(&app.state, n);
    let mut proj = app.state.project.clone();
    crate::project_plugins::store_hosted(&mut proj, &app.plugins);
    let mut m = kept(proj.mixer.iter().find(|m| m.index == n)?);
    // Built-in effects keep their own state (they are not written as wrapped plugins).
    for k in 0..10u8 {
        let key = crate::plugins::fx_key(n, k);
        let Some(p) = m.slots[usize::from(k)].as_mut() else { continue };
        if p.name != os_flp::wrapper::WRAPPER && p.name != crate::plugins::EQ_FILE_NAME && app.plugins.effect_index(&p.name).is_some() {
            if let Some(st) = app.plugins.state(key) {
                p.state = st;
            }
        }
    }
    let mut all = load();
    match all.iter_mut().find(|(nm, _)| *nm == name) {
        Some(s) => s.1 = m,
        None => all.push((name.clone(), m)),
    }
    Some(match store(&all) {
        Ok(()) => format!("Saved the mixer track state \"{name}\""),
        Err(e) => format!("Mixer track state not saved: {e}"),
    })
}

/// Insert from state: a new track at `n` (before the track there, like Insert one) with the state's settings and
/// effects. Wrapped and project-file plugins are hosted by the plugin tick as for an opened project; built-in effects
/// are created from their kept state here.
pub fn insert(app: &mut App, n: u16, name: &str) -> Option<String> {
    let Some((_, st)) = load().into_iter().find(|(nm, _)| nm == name) else { return Some(format!("No mixer track state \"{name}\"")) };
    let s = &mut app.state;
    let t = mx::add_track(s, Some(n.max(1)));
    let fresh = s.project.mixer.iter().find(|m| m.index == t)?.clone();
    let m = s.project.mixer.iter_mut().find(|m| m.index == t)?;
    *m = MixerTrack { index: t, routes: fresh.routes, send_levels: Vec::new(), ..st.clone() };
    for (k, p) in st.slots.iter().enumerate() {
        let Some(p) = p else { continue };
        if p.name == os_flp::wrapper::WRAPPER || p.name == crate::plugins::EQ_FILE_NAME {
            continue;
        }
        if let Some(index) = app.plugins.effect_index(&p.name) {
            let state = (!p.state.is_empty()).then(|| p.state.clone());
            app.state.mixer.pending.push(FxReq::Create { track: t, slot: k as u8, index, state });
        }
    }
    app.state.piano.dirty = true;
    Some(format!("Inserted {} from its state", mx::name(&app.state, t)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer::MCmd;

    #[test]
    fn states_round_trip_through_the_file_text() {
        let mut m = MixerTrack::new(4);
        m.name = Some("Drum bus".into());
        m.color = Some(Rgb::new(0x12, 0xab, 0xff));
        m.volume = Some(0.65);
        m.pan = Some(-0.25);
        m.polarity = true;
        m.eq[1] = [600, 30000, 20000];
        m.slots[2] = Some(SlotPlugin { name: "Lemon Delay, v2".into(), state: vec![0, 1, 0xfe] });
        m.slot_mix[2] = 0.5;
        m.slot_on[2] = false;
        let t = text(&[("Drums".into(), kept(&m))]);
        let back = parse(&t);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].0, "Drums");
        assert_eq!(back[0].1, kept(&m), "{t}");
        assert!(parse("junk\n[A]\nvolume=x\nslot3=1,1,zz,Name\nslot12=1,1,,X\n")[0].1.slots.iter().all(Option::is_none));
    }

    /// Save mixer track state as... keeps the track; Insert from state makes a new track from it (menu items work).
    #[test]
    fn save_and_insert_from_state() {
        let mut a = App::new(None);
        a.state.project = crate::app::test_project();
        let menu = |a: &App| mx::track_menu(&a.state, 3, 0.0, 0.0);
        let from = |p: &crate::playlist::Popup| p.items.iter().find(|i| i.label == "Insert from state").unwrap().children.clone();
        assert!(!from(&menu(&a))[0].enabled, "no saved states yet");
        let file = menu(&a).items.iter().find(|i| i.label == "File").unwrap().children.clone();
        assert!(file.iter().all(|i| i.enabled), "Render and Save state work");
        {
            let m = a.state.project.mixer.iter_mut().find(|m| m.index == 3).unwrap();
            m.name = Some("Vox".into());
            m.volume = Some(0.5);
            m.eq[0][0] = -900;
        }
        let msg = mx::run(&mut a, MCmd::SaveState(3)).unwrap();
        assert!(msg.contains("Vox"), "{msg}");
        assert_eq!(names(), ["Vox"]);
        let items = from(&menu(&a));
        assert!(items[0].enabled && items[0].label == "Vox");
        let len = a.state.project.mixer.len();
        mx::run(&mut a, MCmd::InsertState(6, "Vox".into()));
        assert_eq!(a.state.project.mixer.len(), len + 1);
        let t = mx::track(&a.state, 6).unwrap();
        assert_eq!((t.name.as_deref(), t.volume, t.eq[0][0], t.routes.clone()), (Some("Vox"), Some(0.5), -900, vec![0]));
        assert_eq!(a.state.selected_mixer, 6);
        // Saving again under the same name replaces it.
        a.state.project.mixer.iter_mut().find(|m| m.index == 3).unwrap().volume = Some(0.7);
        mx::run(&mut a, MCmd::SaveState(3));
        assert_eq!(load().len(), 1);
        assert_eq!(load()[0].1.volume, Some(0.7));
    }
}
