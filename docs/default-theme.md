# Default theme

With no saved design or theme, Lemon Studio looks for FL Studio's current theme:

`Documents/Image-Line/FL Studio/Settings/Themes/__current.flstheme`

On Windows, Documents follows the current Windows location, including OneDrive.
When a usable theme is found, Lemon Studio starts in Classic design and imports
its colours through the existing FL theme importer. Otherwise it starts in Lemon design.

A saved design takes precedence. In Classic design, a saved `theme.txt` or
`current.flstheme` takes precedence over FL Studio's current theme.
Automatic detection reads FL's file without copying or changing it.
Until a local theme is saved, restarting Classic design reads FL's current theme again.
