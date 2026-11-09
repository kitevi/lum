//! The radio tray: one Stop/Play toggle, shown for as long as a station runner lives.
//!
//! The runner's main thread owns every tray object, while the supervisor runs on a
//! worker thread and only ever touches the shared [`Playback`] handle. Keeping all
//! menu work on one thread is a macOS requirement, and it matters more than usual
//! here: release builds use `panic = "abort"`, so a panic in tray code would take
//! playback down with it.
//!
//! The tray is a convenience, never a precondition. If it cannot be created (no
//! D-Bus session, no tray host, no window server) the failure is logged and the
//! station keeps playing. See ADR-0022.

use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result, anyhow};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use super::icon::{self, Ink};
use super::playback::Playback;

/// Everything that reaches the UI thread.
enum Msg {
    Menu(MenuEvent),
    WorkerDone,
}

/// The menu item names the action a click performs, not the current state.
fn toggle_label(station: &str, paused: bool) -> String {
    if paused {
        format!("Play {station}")
    } else {
        format!("Stop {station}")
    }
}

fn tooltip(station: &str, paused: bool) -> String {
    if paused {
        format!("lum radio: {station} (paused)")
    } else {
        format!("lum radio: {station}")
    }
}

/// macOS tints a template image for the menu bar, light or dark; everywhere else the
/// pixels are shown exactly as drawn.
const INK: Ink = if cfg!(target_os = "macos") {
    Ink::Template
} else {
    Ink::Colour
};

fn tray_image(paused: bool) -> Result<Icon> {
    Icon::from_rgba(icon::rgba(paused, INK), icon::SIZE, icon::SIZE)
        .map_err(|error| anyhow!("invalid tray icon: {error}"))
}

/// Puts the icon on the builder; on macOS as a template image, so the menu bar tints it.
#[cfg(target_os = "macos")]
fn with_icon(builder: TrayIconBuilder, icon: Icon) -> TrayIconBuilder {
    builder.with_icon_templated(icon)
}

#[cfg(not(target_os = "macos"))]
fn with_icon(builder: TrayIconBuilder, icon: Icon) -> TrayIconBuilder {
    builder.with_icon(icon)
}

/// Swaps the shown icon, with the same macOS template handling as [`with_icon`].
#[cfg(target_os = "macos")]
fn set_icon(tray: &TrayIcon, icon: Icon) -> tray_icon::Result<()> {
    tray.set_icon_templated(Some(icon))
}

#[cfg(not(target_os = "macos"))]
fn set_icon(tray: &TrayIcon, icon: Icon) -> tray_icon::Result<()> {
    tray.set_icon(Some(icon))
}

/// The tray icon and its single menu item. Only ever used on the UI thread.
struct Ui {
    tray: TrayIcon,
    toggle: MenuItem,
    station: String,
    playback: Arc<Playback>,
}

impl Ui {
    fn new(station: &str, playback: Arc<Playback>) -> Result<Self> {
        let paused = playback.is_paused();
        let toggle = MenuItem::new(toggle_label(station, paused), true, None);
        let menu = Menu::new();
        menu.append(&toggle)
            .map_err(|error| anyhow!("failed to build the tray menu: {error}"))?;
        let builder = TrayIconBuilder::new()
            // A stable id lets tray hosts remember the user's visibility choice.
            .with_id("lum-radio")
            .with_menu(Box::new(menu))
            .with_tooltip(tooltip(station, paused));
        let tray = with_icon(builder, tray_image(paused)?)
            .build()
            .context("failed to create the tray icon")?;
        Ok(Self {
            tray,
            toggle,
            station: station.to_owned(),
            playback,
        })
    }

    fn on_menu_event(&self, event: &MenuEvent) {
        if event.id != *self.toggle.id() {
            return;
        }
        let paused = self.playback.toggle();
        self.toggle.set_text(toggle_label(&self.station, paused));
        if let Err(error) = self.refresh(paused) {
            tracing::debug!(error = %error, "failed to refresh the tray after a toggle");
        }
    }

    fn refresh(&self, paused: bool) -> Result<()> {
        self.tray
            .set_tooltip(Some(tooltip(&self.station, paused)))
            .map_err(|error| anyhow!("{error}"))?;
        set_icon(&self.tray, tray_image(paused)?).map_err(|error| anyhow!("{error}"))
    }
}

fn build_ui(station: &str, playback: Arc<Playback>) -> Option<Ui> {
    match Ui::new(station, playback) {
        Ok(ui) => {
            tracing::info!(station, "radio tray ready");
            Some(ui)
        }
        Err(error) => {
            // `{:#}` keeps the whole cause chain, which is the part that says why.
            tracing::warn!("radio tray unavailable; playback continues without it: {error:#}");
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

/// Runs `work` on a worker thread while this thread hosts the tray, and returns
/// `work`'s result once it finishes. The tray disappears with the worker.
///
/// Linux and BSD: the StatusNotifierItem backend runs its own D-Bus thread, so
/// this thread only relays menu events.
#[cfg(not(any(windows, target_os = "macos")))]
pub(super) fn host(
    station: &str,
    playback: Arc<Playback>,
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

    let ui = build_ui(station, playback);
    for msg in rx {
        match msg {
            Msg::Menu(event) => {
                if let Some(ui) = &ui {
                    ui.on_menu_event(&event);
                }
            }
            Msg::WorkerDone => break,
        }
    }
    drop(ui);
    worker
        .join()
        .map_err(|_| anyhow!("radio runner thread panicked"))?
}

/// Runs `work` on a worker thread while this thread hosts the tray, and returns
/// `work`'s result once it finishes. The tray disappears with the worker.
///
/// Windows and macOS: the tray needs a native event loop on this thread, which
/// must be the main thread on macOS.
#[cfg(any(windows, target_os = "macos"))]
pub(super) fn host(
    station: &str,
    playback: Arc<Playback>,
    work: impl FnOnce() -> Result<()> + Send + 'static,
) -> Result<()> {
    use winit::application::ApplicationHandler;
    use winit::event::{StartCause, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    #[cfg(target_os = "macos")]
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
    use winit::window::WindowId;

    struct App<'a> {
        station: &'a str,
        playback: Arc<Playback>,
        ui: Option<Ui>,
    }

    impl ApplicationHandler<Msg> for App<'_> {
        // The icon is created once the loop is actually running, which macOS requires.
        fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
            if cause == StartCause::Init {
                self.ui = build_ui(self.station, Arc::clone(&self.playback));
                #[cfg(target_os = "macos")]
                wake_main_run_loop();
            }
        }

        fn resumed(&mut self, _: &ActiveEventLoop) {}

        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}

        fn user_event(&mut self, event_loop: &ActiveEventLoop, msg: Msg) {
            match msg {
                Msg::Menu(event) => {
                    if let Some(ui) = &self.ui {
                        ui.on_menu_event(&event);
                    }
                }
                Msg::WorkerDone => {
                    self.ui = None;
                    event_loop.exit();
                }
            }
        }
    }

    let mut builder = EventLoop::<Msg>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        // No Dock icon, and do not steal focus from the terminal that launched us.
        builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_activate_ignoring_other_apps(false);
    }
    let event_loop = match builder.build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            tracing::warn!(error = %error, "radio tray unavailable; playback continues without it");
            return work();
        }
    };

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(Msg::Menu(event));
    }));

    let done_proxy = event_loop.create_proxy();
    let worker = spawn_worker(work, move || {
        let _ = done_proxy.send_event(Msg::WorkerDone);
    });

    let mut event_loop = event_loop;
    let mut app = App {
        station,
        playback,
        ui: None,
    };
    if let Err(error) = event_loop.run_app_on_demand(&mut app) {
        // The tray is gone but the station is not: keep supervising until it ends.
        tracing::warn!(error = %error, "radio tray event loop failed; playback continues without it");
    }
    drop(app);
    worker
        .join()
        .map_err(|_| anyhow!("radio runner thread panicked"))?
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
    use std::time::Duration;

    #[test]
    fn toggle_label_names_the_action_a_click_performs() {
        assert_eq!(toggle_label("Drone Zone", false), "Stop Drone Zone");
        assert_eq!(toggle_label("Drone Zone", true), "Play Drone Zone");
    }

    #[test]
    fn tooltip_mentions_pause_only_while_paused() {
        assert_eq!(tooltip("Drone Zone", false), "lum radio: Drone Zone");
        assert_eq!(
            tooltip("Drone Zone", true),
            "lum radio: Drone Zone (paused)"
        );
    }

    #[test]
    fn the_tray_accepts_the_icon_in_both_states() {
        assert!(tray_image(false).is_ok());
        assert!(tray_image(true).is_ok());
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
