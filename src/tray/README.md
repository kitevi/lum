# Tray

lum's tray icon, shared by `lum radio` and, on macOS, `lum display`. This file covers how it is hosted, built and drawn. What the radio's items do is in `src/radio/README.md`, and the display's lines in `src/display/README.md`. Why it works this way is in ADR-0022.

## Hosts

- While a station is loaded, its runner hosts the tray (`host`): the radio's menu, and on macOS the display's lines above it.
- Otherwise, on macOS, `lum display off` starts the display tray (`host_display`, behind the hidden `__display_tray` command): a monitor icon whose menu holds only the display's lines. It hides while a station's tray shows them and exits once HiDPI is back on everywhere.
- On macOS both run their watch every 2 seconds (`WATCH_EVERY`); the monitors' modes are read at most every 10 seconds (`MODES_EVERY` in `src/display/tray.rs`). The station's tray asks for the display's lines; the display tray asks for the lines and whether a station is loaded. Nothing passes between the processes. The display tray doesn't create its icon until it has something to show, so it never flashes up only to hide.
- The host's main thread owns the tray and every menu object. A station's supervisor runs on a worker thread and only touches `Playback`; see `src/radio/README.md`.
- Linux uses tray-icon's `ksni` backend: StatusNotifierItem over D-Bus, pure Rust, no GTK. It needs a tray host. KDE, Xfce and most bars have one; stock GNOME needs the AppIndicator extension. Without a host the icon is simply invisible. Do not switch to tray-icon's default backend: it links GTK 3, and then every lum command would need GTK 3 installed just to start.
- Windows and macOS run a `winit` event loop on the main thread (`mod.rs`). macOS uses the Accessory activation policy, so there is no Dock icon.

## Menu

- The display's lines are disabled items at the top, one for each monitor with HiDPI off, worded as `lum display` prints them. A separator divides them from the radio's items.
- Lines are inserted and removed in place, and only macOS has them: muda carries every change to a `Menu` over to the menu on screen.
- Clicking the icon itself is not a portable gesture across tray hosts, which is why everything is a menu item.

## Icon

- The icon is drawn in code (`icon.rs`), so there is no image asset to ship: a small radio while a station is loaded, a monitor while HiDPI is off, and with both, a smaller radio over the monitor's bottom-right corner, the way SF Symbols draws its badges. The monitor around the radio is cut away with a one-unit gap, and it drops its stand, which the radio would cut in two. It is always one square icon.
- The radio's antenna is up while playing and folded away while paused, so the state shows in the silhouette and not only in the colour (a template has no colour, and green against grey is a weak cue for colour-blind users). The monitor has no states: it is there exactly while HiDPI is off. A playing radio is green and nothing else is, so green means playing.
- The icon has to read on any panel, light or dark, because the host paints it over a panel we cannot see. The colour ink uses mid-tone fills, and a test holds them above WCAG's 3:1 contrast floor for graphics against real dark and light panels. The speaker, the knobs and the screen are holes, so they take the panel's own colour, and a faint white rim around the outside keeps the icon visible on mid-tone panels while vanishing on light ones. macOS gets a black template image instead, which the system tints for the menu bar (`with_icon_templated`); that is why the state also changes the shape and, in the template, the opacity. A template has no colour, so while the radio plays macOS gets the menu bar ink (`Ink::MenuBar`): lum asks the icon's button for its effective appearance, which follows the wallpaper as well as the system setting, and draws the monitor black or white as the template would be, with the radio in Apple's accessible system green for that appearance (`#248a3d` light, `#30d158` dark). The watch asks again every 2 seconds and redraws on a change.
- The geometry sits on a 16-unit grid so the main edges stay crisp at the common 16 px tray size. There is no preview tool in the repo: when you change the drawing, composite `icon::rgba` over a dark and a light panel at 16, 24 and 32 px and look at it; thin shapes that look fine at 64 px turn to mush at 16.
