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
Classic Mixer FX slots use horizontal separators and downward menu arrows,
following the captured FL Studio Wide layout.
