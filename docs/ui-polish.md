# Floating windows and Settings

Drag the Settings title bar to move the window. Its position remains during the
session. The window stays inside the visible area when the main window shrinks.
The close button, Escape, tab selection, and page scrolling keep their actions.
Tabs show a hand cursor on hover. Their appearance changes only when selected.

Settings uses shared window drag handling and a flat frame without an outer
shadow. Classic Settings uses semibold system text, slanted tabs, and small section headers.
The General page places related controls in columns, following FL Studio's
General settings layout. Unavailable controls remain dimmed.

The visual reference was FL Studio 2025's General settings at 144 DPI, with a
1002 by 1350 pixel dialog. The Plugins tab remains Lemon Studio's plugin manager.
This change does not implement unavailable FL Studio settings.

The arrow beside the tabs opens panel options. **Show only one panel** closes
other panels when a panel opens. **Close all panels** folds the page.
Disable **Show only one panel** to use **Open all panels**.
**Go to panel** opens and scrolls to the selected panel.
Unavailable controls remain inactive, with reduced dimming for legibility.
FL Studio's saved PPI and note-name preferences are not imported.

Browser resizing and file context commands are described in
[Browser projects](browser-projects.md).

The browser retains FL Studio's eight-pixel resize gutter, with one device pixel
on each border. The toolbar bottom uses one logical pixel for its divider.
Settings page controls accept
input only inside the visible page. Floating titles use an ellipsis before window buttons.
Classic window titles use the system font, following the native FL captions.
Classic Settings dropdown arrows point down.
General Settings retains the scaling help button and shaded slider handles.
The default Piano roll split follows window height, leaving most space for notes.
Saved split positions and panel collapse behavior remain available.
Classic context menus use a neutral gray panel and regular black system text.
Window options arrows point down, following the native Playlist, Piano roll, and Mixer.
Classic Piano roll keys use smooth shading instead of visible color bands.
The compact keyboard scales black-key length with keyboard width.
The active Piano roll tool uses its native color and rounded outline.
Enabled Classic Mixer polarity and channel-swap switches use the theme highlight
with the same outline. Open Classic menu titles use a dark inset and white text.
Floating windows show a maximize square; maximized windows show the restore glyph.
The Control selector follows native menu sizing and selection styling.
Its popup paints above child windows and stays inside the main window.
Classic context menu hover uses dark gray with light text, matching native FL.
General Settings slider rails use two device pixels, with shaded oval handles.
Classic Mixer FX slots use horizontal separators and downward menu arrows,
following the captured FL Studio Wide layout.

Classic hover behavior follows the captured FL controls. Menu titles, Play, Stop,
window buttons, and Piano roll tools retain their idle appearance. Mixer faders,
knobs, mute switches, polarity buttons, channel-swap buttons, and effect slots do too.
Active tools, playing transport, and enabled Mixer buttons keep their active states.
Held Classic child-window caption buttons and the Settings close button show
an orange outline and glyph. Their actions run on release inside the button.
Releasing outside cancels the action.
Window buttons and tools show a hand cursor. Mixer faders show a vertical
adjustment cursor. Piano roll keys show a hand; the Draw grid shows a pencil.
Window edges and note ends keep their resize cursors. Lemon design retains its
hover highlights.
