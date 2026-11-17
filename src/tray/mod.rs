//! lum's tray icon: the radio's controls while a station is loaded, and the
//! status lines: the monitors `lum display` has HiDPI off on, and `lum net game`
//! while it is on. A station runner hosts it while a station is loaded;
//! otherwise the status tray does (ADR-0022, ADR-0033). See `README.md`.
//!
//! The host's main thread owns every tray object, while a station's supervisor
//! runs on a worker thread and only ever touches the shared [`Playback`] handle.
//! Keeping all menu work on one thread is a macOS requirement, and it matters
//! more than usual here: release builds use `panic = "abort"`, so a panic in tray
//! code would take playback down with it.
//!
//! The tray is a convenience, never a precondition. If it cannot be created (no
//! D-Bus session, no tray host, no window server) the failure is logged and the
//! station keeps playing. See ADR-0022.

mod icon;
pub(crate) mod status;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::radio::playback::Playback;
use crate::radio::stations::{self, Station};
use icon::{Ink, Look};

/// Everything that reaches the UI thread.
enum Msg {
    Menu(MenuEvent),
    WorkerDone,
}

/// The station whose runner hosts the tray.
pub(crate) struct Radio {
    pub(crate) station: Station,
    pub(crate) playback: Arc<Playback>,
}

/// What the tray says besides the radio: one inert menu line for each monitor
/// `lum display` has HiDPI off on, and one while `lum net game` is on.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Status {
    pub(crate) display: Vec<String>,
    pub(crate) game: Option<String>,
}

impl Status {
    pub(crate) fn is_empty(&self) -> bool {
        self.display.is_empty() && self.game.is_none()
    }

    /// The menu lines, in order.
    fn lines(&self) -> impl Iterator<Item = &String> {
        self.display.iter().chain(&self.game)
    }
}

/// What the status tray's watch reports.
pub(crate) struct Shown {
    pub(crate) status: Status,
    /// False hides the icon, while a station's tray shows the same lines.
    pub(crate) visible: bool,
}

/// What a menu click asks of the runner beyond the pause toggle. Every one of them,
/// once carried out, ends this runner.
pub(crate) enum Request {
    Switch(&'static Station),
    Random,
    Quit,
}

/// The menu item names the action a click performs, not the current state.
fn toggle_label(station: &str, paused: bool) -> String {
    if paused {
        format!("Play {station}")
    } else {
        format!("Stop {station}")
    }
}

/// A line for the station, when one is loaded, one for the display, when it has
/// HiDPI off, and one for game mode, when it is on.
fn tooltip(station: Option<(&str, bool)>, display: bool, game: bool) -> String {
    let station = station.map(|(name, paused)| {
        if paused {
            format!("lum radio: {name} (paused)")
        } else {
            format!("lum radio: {name}")
        }
    });
    let display = display.then(|| "lum display: HiDPI off".to_owned());
    let game = game.then(|| "lum net: game mode on".to_owned());
    station
        .into_iter()
        .chain(display)
        .chain(game)
        .collect::<Vec<_>>()
        .join("\n")
}

/// macOS tints a template image for the menu bar, light or dark, but a template has no
/// colour, so while the radio plays or the screen is lit lum draws the tint itself for the menu
/// bar's appearance (`dark`, unknown until the icon exists). Everywhere else the pixels are shown
/// as drawn.
fn ink(look: Look, dark: Option<bool>) -> Ink {
    match dark {
        _ if !cfg!(target_os = "macos") => Ink::Colour,
        Some(dark) if look.radio == Some(false) || look.game => Ink::MenuBar { dark },
        _ => Ink::Template,
    }
}

/// The tray's image and its ink.
fn tray_image(look: Look, dark: Option<bool>) -> Result<(Icon, Ink)> {
    let ink = ink(look, dark);
    let icon = Icon::from_rgba(icon::rgba(look, ink), icon::SIZE, icon::SIZE)
        .map_err(|error| anyhow!("invalid tray icon: {error}"))?;
    Ok((icon, ink))
}

/// Puts the icon on the builder, as a template image when it is one.
fn with_icon(builder: TrayIconBuilder, (icon, ink): (Icon, Ink)) -> TrayIconBuilder {
    match ink {
        Ink::Template => builder.with_icon_templated(icon),
        Ink::Colour | Ink::MenuBar { .. } => builder.with_icon(icon),
    }
}

/// Swaps the shown icon, with the same template handling as [`with_icon`].
fn set_icon(tray: &TrayIcon, (icon, ink): (Icon, Ink)) -> tray_icon::Result<()> {
    match ink {
        Ink::Template => tray.set_icon_templated(Some(icon)),
        Ink::Colour | Ink::MenuBar { .. } => tray.set_icon(Some(icon)),
    }
}

/// Whether the menu bar the icon sits in is dark. It follows the wallpaper as well as the
/// system's appearance, so it is asked of the icon's own button.
#[cfg(target_os = "macos")]
fn menu_bar_dark(tray: &TrayIcon) -> Option<bool> {
    use objc2_app_kit::{
        NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSAppearanceNameVibrantDark, NSAppearanceNameVibrantLight,
    };
    use objc2_foundation::NSArray;

    let mtm = objc2::MainThreadMarker::new()?;
    let button = tray.ns_status_item()?.button(mtm)?;
    // SAFETY: the appearance names are constants AppKit exports.
    let (aqua, dark, vibrant_light, vibrant_dark) = unsafe {
        (
            NSAppearanceNameAqua,
            NSAppearanceNameDarkAqua,
            NSAppearanceNameVibrantLight,
            NSAppearanceNameVibrantDark,
        )
    };
    let names = NSArray::from_slice(&[aqua, dark, vibrant_light, vibrant_dark]);
    let best = button
        .effectiveAppearance()
        .bestMatchFromAppearancesWithNames(&names)?;
    Some(&*best == dark || &*best == vibrant_dark)
}

#[cfg(not(target_os = "macos"))]
fn menu_bar_dark(_: &TrayIcon) -> Option<bool> {
    None
}

/// The icon and tooltip for the station, when one is loaded, and the status lines.
fn face(
    radio: Option<&RadioMenu>,
    status: &Status,
    dark: Option<bool>,
) -> Result<((Icon, Ink), String)> {
    let station = radio.map(|radio| (radio.station, radio.playback.is_paused()));
    let display = !status.display.is_empty();
    let game = status.game.is_some();
    let look = Look {
        radio: station.map(|(_, paused)| paused),
        // The status tray keeps its monitor while it hides without lines.
        monitor: display || radio.is_none(),
        game,
    };
    Ok((tray_image(look, dark)?, tooltip(station, display, game)))
}

/// The radio's part of the menu: the Stop/Play toggle, the station list, a
/// random station and Quit.
struct RadioMenu {
    toggle: MenuItem,
    switches: Vec<(MenuId, &'static Station)>,
    random: MenuId,
    quit: MenuId,
    station: &'static str,
    playback: Arc<Playback>,
}

impl RadioMenu {
    fn new(radio: &Radio, menu: &Menu) -> Result<Self> {
        let Radio { station, playback } = radio;
        let toggle = MenuItem::new(toggle_label(station.name, playback.is_paused()), true, None);

        // The current station is listed too, checked and inert, so the list reads
        // the same whichever station is playing.
        let others = Submenu::new("Stations", true);
        let mut switches = Vec::new();
        for other in stations::all() {
            let appended = if other.code == station.code {
                others.append(&CheckMenuItem::new(other.name, false, true, None))
            } else {
                let item = MenuItem::new(other.name, true, None);
                switches.push((item.id().clone(), other));
                others.append(&item)
            };
            appended.map_err(menu_error)?;
        }
        let random = MenuItem::new("Random station", true, None);
        let quit = MenuItem::new("Quit radio", true, None);
        menu.append_items(&[
            &toggle,
            &PredefinedMenuItem::separator(),
            &others,
            &random,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(menu_error)?;
        Ok(Self {
            toggle,
            switches,
            random: random.into_id(),
            quit: quit.into_id(),
            station: station.name,
            playback: Arc::clone(playback),
        })
    }
}

/// The tray icon and its menu. Only ever used on the UI thread.
struct Ui {
    tray: TrayIcon,
    /// The tray's menu: muda carries every change over to the menu on screen.
    menu: Menu,
    radio: Option<RadioMenu>,
    /// The status lines at the top of the menu, and the separator under them
    /// when the radio's items follow.
    items: Vec<Box<dyn IsMenuItem>>,
    status: Status,
    visible: bool,
    /// Whether the menu bar is dark, as last drawn for; see [`ink`].
    dark: Option<bool>,
}

impl Ui {
    fn new(radio: Option<&Radio>, shown: Shown) -> Result<Self> {
        let menu = Menu::new();
        let radio = radio
            .map(|radio| RadioMenu::new(radio, &menu))
            .transpose()?;
        let (image, tip) = face(radio.as_ref(), &shown.status, None)?;
        let builder = TrayIconBuilder::new()
            // A stable id lets tray hosts remember the user's visibility choice.
            .with_id(if radio.is_some() {
                "lum-radio"
            } else {
                "lum-status"
            })
            .with_menu(Box::new(menu.clone()))
            .with_tooltip(tip);
        let tray = with_icon(builder, image)
            .build()
            .context("failed to create the tray icon")?;
        let mut ui = Self {
            tray,
            menu,
            radio,
            items: Vec::new(),
            status: Status::default(),
            visible: true,
            dark: None,
        };
        ui.dark = menu_bar_dark(&ui.tray);
        ui.set_lines(shown.status)?;
        ui.refresh()?;
        ui.set_visible(shown.visible)?;
        Ok(ui)
    }

    /// Brings the tray up to date with the watch.
    fn show(&mut self, shown: Shown) -> Result<()> {
        let dark = menu_bar_dark(&self.tray);
        let lines_changed = shown.status != self.status;
        if lines_changed {
            self.set_lines(shown.status)?;
        }
        if lines_changed || dark != self.dark {
            self.dark = dark;
            self.refresh()?;
        }
        self.set_visible(shown.visible)
    }

    /// Puts an inert item for each status line at the top of the menu.
    fn set_lines(&mut self, status: Status) -> Result<()> {
        for item in self.items.drain(..) {
            self.menu.remove(&*item).map_err(menu_error)?;
        }
        let mut items: Vec<Box<dyn IsMenuItem>> = status
            .lines()
            .map(|line| Box::new(MenuItem::new(line, false, None)) as Box<dyn IsMenuItem>)
            .collect();
        if self.radio.is_some() && !items.is_empty() {
            items.push(Box::new(PredefinedMenuItem::separator()));
        }
        let shown: Vec<&dyn IsMenuItem> = items.iter().map(|item| &**item).collect();
        self.menu.insert_items(&shown, 0).map_err(menu_error)?;
        self.items = items;
        self.status = status;
        Ok(())
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        if visible != self.visible {
            self.tray
                .set_visible(visible)
                .map_err(|error| anyhow!("{error}"))?;
            self.visible = visible;
        }
        Ok(())
    }

    /// Handles the pause toggle here; any other click comes back as a [`Request`]
    /// for the runner.
    fn on_menu_event(&self, event: &MenuEvent) -> Option<Request> {
        let radio = self.radio.as_ref()?;
        if event.id == *radio.toggle.id() {
            let paused = radio.playback.toggle();
            radio.toggle.set_text(toggle_label(radio.station, paused));
            if let Err(error) = self.refresh() {
                tracing::debug!(error = %error, "failed to refresh the tray after a toggle");
            }
            None
        } else if event.id == radio.quit {
            Some(Request::Quit)
        } else if event.id == radio.random {
            Some(Request::Random)
        } else {
            let (_, station) = radio.switches.iter().find(|(id, _)| *id == event.id)?;
            Some(Request::Switch(station))
        }
    }

    /// Redraws the icon and the tooltip.
    fn refresh(&self) -> Result<()> {
        let (image, tip) = face(self.radio.as_ref(), &self.status, self.dark)?;
        self.tray
            .set_tooltip(Some(tip))
            .map_err(|error| anyhow!("{error}"))?;
        set_icon(&self.tray, image).map_err(|error| anyhow!("{error}"))
    }
}

fn menu_error(error: tray_icon::menu::Error) -> anyhow::Error {
    anyhow!("failed to build the tray menu: {error}")
}

fn build_ui(radio: Option<&Radio>, shown: Shown) -> Option<Ui> {
    match Ui::new(radio, shown) {
        Ok(ui) => {
            let station = radio.map_or("none", |radio| radio.station.name);
            tracing::info!(station, "tray ready");
            Some(ui)
        }
        Err(error) => {
            // `{:#}` keeps the whole cause chain, which is the part that says why.
            tracing::warn!("tray unavailable: {error:#}");
            None
        }
    }
}

/// Runs `work` on its own thread and calls `done` when it ends, even by panic, so
/// the UI thread can never be left waiting on a worker that is already gone.
fn spawn_worker(
    work: impl FnOnce() -> Result<()> + Send + 'static,
    done: impl FnOnce() + Send + 'static,
) -> thread::JoinHandle<Result<()>> {
    struct OnExit<F: FnOnce()>(Option<F>);

    impl<F: FnOnce()> Drop for OnExit<F> {
        fn drop(&mut self) {
            if let Some(done) = self.0.take() {
                done();
            }
        }
    }

    thread::spawn(move || {
        let _done = OnExit(Some(done));
        work()
    })
}

/// How often the watch runs.
const WATCH_EVERY: Duration = Duration::from_secs(2);

/// Hosts the tray on this thread, relaying its menu events, and returns true when a menu
/// request ended it. It ends too, returning false, when the worker is done.
///
/// `watch` runs first and then every [`WATCH_EVERY`]; None ends the tray, which is how the
/// status tray leaves. A tray that is not visible is taken down and put up again when it is:
/// the StatusNotifierItem backend can only mark an icon passive, which a tray host may ignore.
#[cfg(not(any(windows, target_os = "macos")))]
fn serve(
    radio: Option<&Radio>,
    rx: &std::sync::mpsc::Receiver<Msg>,
    on_request: impl Fn(Request) -> bool,
    mut watch: impl FnMut() -> Option<Shown>,
) -> bool {
    use std::sync::mpsc::RecvTimeoutError;

    let mut ui: Option<Ui> = None;
    // A station's tray that could not be built is not tried again.
    let mut gave_up = false;
    loop {
        if !gave_up {
            let Some(shown) = watch() else {
                return true;
            };
            if !shown.visible {
                ui = None;
            } else if let Some(ui) = &mut ui {
                if let Err(error) = ui.show(shown) {
                    tracing::debug!(error = %error, "failed to update the tray");
                }
            } else {
                ui = build_ui(radio, shown);
                if ui.is_none() {
                    // The status tray has nothing to do without its icon, while a station
                    // plays on without it.
                    if radio.is_none() {
                        return true;
                    }
                    gave_up = true;
                }
            }
        }
        let msg = if gave_up {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(WATCH_EVERY)
        };
        match msg {
            Ok(Msg::Menu(event)) => {
                let request = ui.as_ref().and_then(|ui| ui.on_menu_event(&event));
                if request.is_some_and(&on_request) {
                    return true;
                }
            }
            Ok(Msg::WorkerDone) | Err(RecvTimeoutError::Disconnected) => return false,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Runs `work` on a worker thread while this thread hosts the tray, and returns
/// `work`'s result once it finishes. The tray disappears with the worker.
///
/// `on_request` runs on this thread for every menu click other than the pause
/// toggle. When it returns true the runner is done: the tray is removed and this
/// returns at once, without waiting for `work`, which is left parked on a halted
/// [`Playback`] until the process exits.
///
/// `status` gives the status lines. It runs on this thread when the tray starts
/// and every 2 seconds after.
///
/// Linux and BSD: the StatusNotifierItem backend runs its own D-Bus thread, so
/// this thread only relays menu events.
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn host(
    radio: Radio,
    on_request: impl Fn(Request) -> bool,
    mut status: impl FnMut() -> Status,
    work: impl FnOnce() -> Result<()> + Send + 'static,
) -> Result<()> {
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();
    let menu_events = tx.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_events.send(Msg::Menu(event));
    }));

    // Playback first: the tray is an extra and must never delay or gate it.
    let worker = spawn_worker(work, move || {
        let _ = tx.send(Msg::WorkerDone);
    });

    let watch = move || {
        Some(Shown {
            status: status(),
            visible: true,
        })
    };
    if serve(Some(&radio), &rx, on_request, watch) {
        return Ok(());
    }
    worker
        .join()
        .map_err(|_| anyhow!("radio runner thread panicked"))?
}

/// Hosts the status tray, with no station, until `watch` returns None. `watch`
/// runs on this thread when the tray starts and every 2 seconds after.
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn host_status(watch: impl FnMut() -> Option<Shown>) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = tx.send(Msg::Menu(event));
    }));
    serve(None, &rx, |_| false, watch);
    Ok(())
}

/// Runs `work` on a worker thread while this thread hosts the tray, and returns
/// `work`'s result once it finishes. The tray disappears with the worker.
///
/// `on_request` runs on this thread for every menu click other than the pause
/// toggle. When it returns true the runner is done: the tray is removed and this
/// returns at once, without waiting for `work`, which is left parked on a halted
/// [`Playback`] until the process exits.
///
/// `status` gives the status lines. It runs on this thread when the tray starts,
/// and every 2 seconds on macOS, the one platform that has them.
///
/// Windows and macOS: the tray needs a native event loop on this thread, which
/// must be the main thread on macOS.
#[cfg(any(windows, target_os = "macos"))]
pub(crate) fn host(
    radio: Radio,
    on_request: impl Fn(Request) -> bool,
    mut status: impl FnMut() -> Status,
    work: impl FnOnce() -> Result<()> + Send + 'static,
) -> Result<()> {
    let event_loop = match native::event_loop() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            tracing::warn!(error = %error, "radio tray unavailable; playback continues without it");
            return work();
        }
    };
    let done_proxy = event_loop.create_proxy();
    let worker = spawn_worker(work, move || {
        let _ = done_proxy.send_event(Msg::WorkerDone);
    });
    let watch = move || {
        Some(Shown {
            status: status(),
            visible: true,
        })
    };
    if native::run(event_loop, Some(radio), on_request, watch) {
        return Ok(());
    }
    worker
        .join()
        .map_err(|_| anyhow!("radio runner thread panicked"))?
}

/// Hosts the status tray, with no station, until `watch` returns None. `watch`
/// runs on this thread when the tray starts and, on macOS, every 2 seconds after.
#[cfg(any(windows, target_os = "macos"))]
pub(crate) fn host_status(watch: impl FnMut() -> Option<Shown>) -> Result<()> {
    let event_loop = native::event_loop().context("could not start the status tray")?;
    native::run(event_loop, None, |_| false, watch);
    Ok(())
}

/// The native event loop the tray needs on Windows and macOS.
#[cfg(any(windows, target_os = "macos"))]
mod native {
    use std::time::Instant;

    use tray_icon::menu::MenuEvent;
    use winit::application::ApplicationHandler;
    use winit::error::EventLoopError;
    use winit::event::{StartCause, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    #[cfg(target_os = "macos")]
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    use winit::window::WindowId;

    use super::{Msg, Radio, Request, Shown, Ui, build_ui};

    pub(super) fn event_loop() -> Result<EventLoop<Msg>, EventLoopError> {
        let mut builder = EventLoop::<Msg>::with_user_event();
        #[cfg(target_os = "macos")]
        {
            // No Dock icon, and do not steal focus from the terminal that launched us.
            builder
                .with_activation_policy(ActivationPolicy::Accessory)
                .with_activate_ignoring_other_apps(false);
        }
        builder.build()
    }

    /// Runs the tray until the worker finishes, or until a request or the watch
    /// ends it, which is when this returns true: the worker must not be waited for.
    pub(super) fn run(
        mut event_loop: EventLoop<Msg>,
        radio: Option<Radio>,
        on_request: impl Fn(Request) -> bool,
        watch: impl FnMut() -> Option<Shown>,
    ) -> bool {
        let menu_proxy = event_loop.create_proxy();
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = menu_proxy.send_event(Msg::Menu(event));
        }));
        let mut app = App {
            radio,
            on_request,
            watch,
            ui: None,
            left: false,
        };
        if let Err(error) = event_loop.run_app_on_demand(&mut app) {
            // The tray is gone but the station is not: keep supervising until it ends.
            tracing::warn!(error = %error, "tray event loop failed; playback continues without it");
        }
        app.left
    }

    struct App<R, W> {
        radio: Option<Radio>,
        on_request: R,
        watch: W,
        ui: Option<Ui>,
        /// A request or the watch ended the tray.
        left: bool,
    }

    impl<R, W> App<R, W> {
        fn leave(&mut self, event_loop: &ActiveEventLoop) {
            self.left = true;
            self.ui = None;
            event_loop.exit();
        }
    }

    impl<R: Fn(Request) -> bool, W: FnMut() -> Option<Shown>> ApplicationHandler<Msg> for App<R, W> {
        // The icon is created once the loop is actually running, which macOS requires.
        fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
            match cause {
                StartCause::Init => {}
                // A station whose tray failed has nothing left to watch for.
                StartCause::ResumeTimeReached { .. }
                    if self.ui.is_some() || self.radio.is_none() => {}
                _ => return,
            }
            let Some(shown) = (self.watch)() else {
                return self.leave(event_loop);
            };
            if let Some(ui) = &mut self.ui {
                if let Err(error) = ui.show(shown) {
                    tracing::debug!(error = %error, "failed to update the tray");
                }
            } else if shown.visible {
                // The status tray waits until it has something to show, so it
                // never flashes up only to hide.
                self.ui = build_ui(self.radio.as_ref(), shown);
                if self.ui.is_none() {
                    // The status tray has nothing to do without its icon, while a
                    // station plays on without it.
                    if self.radio.is_none() {
                        self.leave(event_loop);
                    }
                    return;
                }
                #[cfg(target_os = "macos")]
                super::wake_main_run_loop();
            }
            // Only macOS has status lines that change.
            if cfg!(target_os = "macos") {
                let next = Instant::now() + super::WATCH_EVERY;
                event_loop.set_control_flow(ControlFlow::WaitUntil(next));
            }
        }

        fn resumed(&mut self, _: &ActiveEventLoop) {}

        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}

        fn user_event(&mut self, event_loop: &ActiveEventLoop, msg: Msg) {
            match msg {
                Msg::Menu(event) => {
                    let request = self.ui.as_ref().and_then(|ui| ui.on_menu_event(&event));
                    if request.is_some_and(&self.on_request) {
                        self.leave(event_loop);
                    }
                }
                Msg::WorkerDone => {
                    self.ui = None;
                    event_loop.exit();
                }
            }
        }
    }
}

/// tray-icon's own examples nudge the main run loop after creating the status
/// item; without it the icon may not show up until the next event.
#[cfg(target_os = "macos")]
fn wake_main_run_loop() {
    use objc2_core_foundation::CFRunLoop;

    if let Some(main) = CFRunLoop::main() {
        CFRunLoop::wake_up(&main);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_label_names_the_action_a_click_performs() {
        assert_eq!(toggle_label("Drone Zone", false), "Stop Drone Zone");
        assert_eq!(toggle_label("Drone Zone", true), "Play Drone Zone");
    }

    #[test]
    fn tooltip_has_a_line_each_for_the_station_the_display_and_game_mode() {
        let playing = Some(("Drone Zone", false));
        assert_eq!(tooltip(playing, false, false), "lum radio: Drone Zone");
        assert_eq!(
            tooltip(Some(("Drone Zone", true)), false, false),
            "lum radio: Drone Zone (paused)"
        );
        assert_eq!(tooltip(None, true, false), "lum display: HiDPI off");
        assert_eq!(tooltip(None, false, true), "lum net: game mode on");
        assert_eq!(
            tooltip(playing, true, true),
            "lum radio: Drone Zone\nlum display: HiDPI off\nlum net: game mode on"
        );
    }

    #[test]
    fn status_lines_are_the_display_lines_then_game_mode() {
        let status = Status {
            display: vec!["DELL U2723QE".to_owned(), "LG 27GL850".to_owned()],
            game: Some("Game mode on for eth0".to_owned()),
        };
        let lines: Vec<&str> = status.lines().map(String::as_str).collect();
        assert_eq!(
            lines,
            ["DELL U2723QE", "LG 27GL850", "Game mode on for eth0"]
        );
        assert!(!status.is_empty());
        assert!(Status::default().is_empty());
        assert!(
            !Status {
                game: Some("on".to_owned()),
                ..Status::default()
            }
            .is_empty()
        );
    }

    #[test]
    fn macos_draws_its_own_tint_for_a_playing_radio_and_a_lit_screen() {
        let look = |radio, game| Look {
            radio,
            monitor: false,
            game,
        };
        let menu_bar = cfg!(target_os = "macos");
        for (look, own_tint) in [
            (look(Some(false), false), menu_bar),
            (look(None, true), menu_bar),
            (look(Some(true), true), menu_bar),
            // A paused radio alone is a template, which macOS tints itself.
            (look(Some(true), false), false),
        ] {
            let ink = ink(look, Some(false));
            assert_eq!(matches!(ink, Ink::MenuBar { .. }), own_tint, "{look:?}");
        }
    }

    #[test]
    fn the_tray_accepts_the_icon_in_every_look() {
        for radio in [None, Some(false), Some(true)] {
            for monitor in [false, true] {
                for game in [false, true] {
                    if radio.is_some() || monitor || game {
                        for dark in [None, Some(false), Some(true)] {
                            let look = Look {
                                radio,
                                monitor,
                                game,
                            };
                            assert!(tray_image(look, dark).is_ok(), "{look:?} {dark:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn worker_reports_done_and_hands_back_its_result() {
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = spawn_worker(
            || Err(anyhow!("stream gave up")),
            move || {
                let _ = tx.send(());
            },
        );

        assert!(rx.recv_timeout(Duration::from_secs(10)).is_ok());
        let error = worker
            .join()
            .expect("no panic")
            .expect_err("the work failed");
        assert_eq!(error.to_string(), "stream gave up");
    }

    #[test]
    fn worker_reports_done_even_when_it_panics() {
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = spawn_worker(
            || -> Result<()> { panic!("simulated supervisor panic") },
            move || {
                let _ = tx.send(());
            },
        );

        assert!(
            rx.recv_timeout(Duration::from_secs(10)).is_ok(),
            "the UI thread must still be told, or it would wait forever"
        );
        assert!(worker.join().is_err());
    }
}
