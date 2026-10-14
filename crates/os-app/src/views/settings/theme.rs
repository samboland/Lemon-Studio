//! Settings > Theme: the preset tiles ((Default) = the Lemon design, Classic, theme files), adjustments, options, theme
//! import.
// spec: ui/settings.md "Theme", ui/lemon-design.md §1, ui/flstheme.md, ui/theme-presets.md

use os_theme::Design;
use os_ui::Color;

use super::{g, Group, Row, SAct};
use crate::app::{App, State};

/// The checks and buttons of this page that work (`SAct::Check` / `SAct::Button` labels, handled in `act`).
pub const LIVE: &[&str] = &["Load theme...", "Reset"];

/// The Theme page's groups (spec §4).
pub fn page(s: &State) -> Vec<Group> {
    use Row::*;
    let st = &s.settings;
    // (Default) = the Lemon design, then the Classic look, then theme files (spec: ui/lemon-design.md §1).
    use os_theme::design::lemon as brand;
    let mut tiles = vec![("(Default)".to_string(), [brand::DARK, brand::MAIN, brand::HIGHLIGHT]), ("Classic".to_string(), swatch(&os_theme::Palette::default()))];
    for (name, text) in &st.themes {
        tiles.push((name.clone(), swatch(&os_theme::fl::palette(&os_theme::FlTheme::parse(text)))));
    }
    vec![
        g("Presets", vec![Presets(tiles, st.preset)]),
        g("Adjustments", vec![ThemeKnobs]),
        g("Options", vec![Check("Light mode", true), Check("Audio and automation clips use note colors", false)]),
        g("", vec![Buttons(vec![("Load theme...", 110.0), ("Reset", 64.0), ("Save preset...", 100.0)])]),
    ]
}

/// Three colours that represent a palette on its preset tile: surface, highlight, selection.
fn swatch(p: &os_theme::Palette) -> [Color; 3] {
    [p.s(0x2b2d30), p.h(0xffaf65), p.a(0x9ad04a)]
}

/// This page's working controls: the preset tiles, Reset and Load theme.
pub(crate) fn act(app: &mut App, a: &SAct) -> bool {
    let st = &mut app.state.settings;
    match *a {
        // Tiles (spec: ui/lemon-design.md §1): 0 = (Default) = the Lemon design, 1 = the Classic look without a
        // theme file, then the theme files (they colour the Classic design).
        SAct::Preset(0) => {
            app.set_design(Design::Lemon, true);
            app.state.message = Some("Theme: default (Lemon)".into());
        }
        SAct::Preset(1) => {
            app.theme = os_theme::Theme::default();
            os_theme::set_current(&app.theme.colors);
            crate::app::save_design(Design::Classic);
            if let Some(f) = os_paths::user_file("current.flstheme") {
                let _ = std::fs::remove_file(f);
            }
            app.sync_preset();
            app.state.message = Some("Theme: Classic".into());
        }
        SAct::Preset(i) => {
            if let Some((name, text)) = st.themes.get(i - 2).cloned() {
                st.preset = i;
                let fl = os_theme::FlTheme::parse(&text);
                app.theme = os_theme::Theme::new(&name, os_theme::fl::palette(&fl), os_theme::Adjust::default());
                os_theme::set_current(&app.theme.colors);
                crate::app::save_design(Design::Classic);
                // Remembered like an imported theme (loaded at the next start).
                if let Some(d) = os_paths::user_dir() {
                    let _ = std::fs::create_dir_all(&d);
                    let _ = std::fs::write(d.join("current.flstheme"), &text);
                }
                app.state.message = Some(format!("Theme: {name}"));
            }
        }
        SAct::Button("Reset") => {
            act(app, &SAct::Preset(0));
            app.invalidate();
        }
        SAct::Button("Load theme...") => app.theme_dialog(),
        _ => return false,
    }
    true
}

impl App {
    /// The Theme preset tile shown as chosen (spec: ui/lemon-design.md §1): (Default) for Lemon, Classic for the plain
    /// Classic look, else the theme file picked from the tiles (none when the theme came from elsewhere).
    pub(crate) fn sync_preset(&mut self) {
        let st = &mut self.state.settings;
        st.preset = if self.theme.colors.style.lemon() {
            0
        } else if self.theme.colors == os_theme::Palette::default() {
            1
        } else if st.preset >= 2 {
            st.preset
        } else {
            usize::MAX
        };
    }

    /// Theme import (spec: ui/flstheme.md): pick a .flstheme, starting in FL's theme folder when installed.
    pub(crate) fn theme_dialog(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("Theme", &["flstheme", "txt"]).set_title("Load theme");
        let fl = std::path::Path::new(r"C:\Program Files\Image-Line\FL Studio 2026\Artwork\Themes");
        if fl.is_dir() {
            dlg = dlg.set_directory(fl);
        }
        if let Some(p) = crate::dialog::modal(|| dlg.pick_file()) {
            self.load_theme_file(&p);
        }
    }
}
