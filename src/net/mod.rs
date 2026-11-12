//! `lum net`: switch the wired connection between normal and game mode.
//!
//! Game mode forces the link to 100 Mb/s and shapes downloads just under that,
//! so bursts queue at the computer instead of being dropped upstream, and
//! downloads cannot fill the router's queue. See `README.md` and ADR-0024.

/// `println!` that drops the line when it cannot be written. `println!`
/// panics instead, and with the terminal gone that would end lum before it
/// switches back. Everything that switches game mode prints through this.
macro_rules! say {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

/// `eprintln!` that drops the line when it cannot be written, like `say!`.
macro_rules! say_err {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

mod desktop;
mod gfn;
mod iface;
mod linux;
mod macos;
mod probe;
mod root;
mod wrap;

use std::{
    fs,
    process::ExitStatus,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::cli::NetCommand;
use iface::Iface;
use wrap::Trap;

/// The link speed game mode forces, in Mb/s.
const GAME_SPEED: u32 = 100;
/// Downloads are shaped a little under the link speed, so the queue builds
/// here, where the shaper keeps it short, and not in the router.
const SHAPED_MBIT: u32 = 90;
/// How long to wait for the link to come back after a speed change.
const LINK_TIMEOUT: Duration = Duration::from_secs(15);

/// `LINK_TIMEOUT`, or the milliseconds in `$LUM_NET_LINK_TIMEOUT_MS`, a hook
/// for tests of a link that never comes back.
fn link_timeout() -> Duration {
    std::env::var("LUM_NET_LINK_TIMEOUT_MS")
        .ok()
        .and_then(|millis| millis.parse().ok())
        .map_or(LINK_TIMEOUT, Duration::from_millis)
}

pub fn run(command: NetCommand) -> Result<()> {
    let platform = platform()?;
    match command {
        NetCommand::Game { iface, command } => game(platform, iface.as_deref(), &command),
        NetCommand::Normal => normal(platform),
        NetCommand::Status => status(platform),
        NetCommand::Test { iface, seconds } => probe::run(platform, iface.as_deref(), seconds),
    }
}

/// What game mode needs from the operating system.
trait Platform {
    /// Every network interface, wired or not, with its current link.
    fn interfaces(&self) -> Result<Vec<Iface>>;
    /// The interface that carries the default route, if any.
    fn default_route(&self) -> Option<String>;
    /// Fails with an install hint when a tool game mode needs is missing.
    fn check_tools(&self) -> Result<()>;
    fn force_game_speed(&self, iface: &str) -> Result<()>;
    fn restore_auto_speed(&self, iface: &str) -> Result<()>;
    /// Starts shaping downloads on `iface`. Returns a token `unshape` needs.
    fn shape(&self, iface: &str) -> Result<Option<String>>;
    /// Removes download shaping. Best effort: part of it may already be gone.
    fn unshape(&self, iface: &str, token: Option<&str>);
    /// Whether the system still shows the changes `record` describes. False
    /// after a reboot, which undoes them all.
    fn is_applied(&self, record: &GameRecord) -> bool;
}

fn platform() -> Result<&'static dyn Platform> {
    if cfg!(target_os = "linux") {
        Ok(&linux::Linux)
    } else if cfg!(target_os = "macos") {
        Ok(&macos::MacOs)
    } else {
        bail!("lum net is only supported on Linux and macOS")
    }
}

/// What `lum net game` changed, kept in `net-game.json` so `lum net normal`
/// can undo it even from another terminal or after lum was killed.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GameRecord {
    iface: String,
    /// Finds the interface again if it was renamed or replugged.
    mac: String,
    started: String,
    /// macOS: the pf enable reference to release.
    #[serde(default)]
    pf_token: Option<String>,
    /// GNOME Shell extensions switched off for a game. GNOME saves that, so
    /// they stay off across a reboot until they are switched back on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hidden_extensions: Vec<String>,
}

fn game(platform: &dyn Platform, requested: Option<&str>, command: &[String]) -> Result<()> {
    if command.first().map(String::as_str) == Some(gfn::NAME) {
        ensure!(
            command.len() == 1,
            "'lum net game gfn' takes no arguments; put --iface before gfn"
        );
        return gfn::play(platform, requested);
    }
    if let Some(program) = command.first() {
        // Before anything changes: a mistyped name would otherwise cost a
        // switch on and straight back off.
        which::which(program)
            .with_context(|| format!("{program} not found; game mode was left as it was"))?;
    }

    let trap = if command.is_empty() {
        Trap::none()
    } else {
        Trap::install()?
    };
    let Some(record) = begin(platform, requested)? else {
        if command.is_empty() {
            return Ok(());
        }
        say!("It stays on after the command exits.");
        return finish_command(command, wrap::run(command, &trap), trap.fired());
    };
    if command.is_empty() {
        say!("Run 'lum net normal' to switch back.");
        return Ok(());
    }

    let outcome = {
        let _sudo = root::keep_alive();
        wrap::run(command, &trap)
    };
    say!("Switching game mode off.");
    switch_off(platform, &record)
        .context("game mode is still on; run 'lum net normal' to switch back")?;
    finish_command(command, outcome, trap.fired())
}

fn normal(platform: &dyn Platform) -> Result<()> {
    let Some(record) = load_record()? else {
        say!("Game mode is not on.");
        return Ok(());
    };
    switch_off(platform, &record)
}

fn status(platform: &dyn Platform) -> Result<()> {
    match load_record()? {
        Some(record) => {
            if platform.is_applied(&record) {
                say!(
                    "Game mode: on for {} since {} ({GAME_SPEED} Mb/s, downloads shaped to {SHAPED_MBIT} Mbit/s)",
                    record.iface,
                    record.started
                );
            } else {
                say!(
                    "Game mode: off (a record for {} from {} is left over; 'lum net normal' clears it)",
                    record.iface,
                    record.started
                );
            }
            for uuid in &record.hidden_extensions {
                say!("{} is off for the game.", desktop::name(uuid));
            }
        }
        None => say!("Game mode: off"),
    }

    let wired: Vec<Iface> = platform
        .interfaces()?
        .into_iter()
        .filter(|iface| iface.wired)
        .collect();
    if wired.is_empty() {
        say!("No wired Ethernet interface with a link.");
        return Ok(());
    }
    let routed = platform.default_route();
    let networks = sysinfo::Networks::new_with_refreshed_list();
    for iface in wired {
        let route = if routed.as_deref() == Some(iface.name.as_str()) {
            ", default route"
        } else {
            ""
        };
        let errors = networks
            .list()
            .get(&iface.name)
            .map(|data| {
                format!(
                    ", {} receive errors in {} packets since boot",
                    data.total_errors_on_received(),
                    data.total_packets_received()
                )
            })
            .unwrap_or_default();
        say!(
            "{}  {}  {}{route}{errors}",
            iface.name,
            iface.mac,
            describe_link(iface.speed, iface.link)
        );
    }
    Ok(())
}

/// Switches game mode on and returns its record, or `None` if it was on
/// already, in which case whoever switched it on also switches it off.
fn begin(platform: &dyn Platform, requested: Option<&str>) -> Result<Option<GameRecord>> {
    if let Some(record) = load_record()? {
        if platform.is_applied(&record) {
            if let Some(requested) = requested {
                ensure!(
                    requested == record.iface,
                    "game mode is already on for {}; run 'lum net normal' before using {requested}",
                    record.iface
                );
            }
            say!("Game mode is already on for {}.", record.iface);
            return Ok(None);
        }
        // A reboot undid the network changes, but a lum killed part-way
        // through switching on can leave the link forced with no shaper to
        // show for it. So the record is undone in full, as `normal` does.
        say!("Clearing what an earlier game mode left behind.");
        switch_off(platform, &record)?;
    }

    platform.check_tools()?;
    let iface = pick(platform, requested)?;
    root::authenticate()?;
    let mut record = GameRecord {
        iface: iface.name.clone(),
        mac: iface.mac.clone(),
        started: jiff::Zoned::now().strftime("%Y-%m-%d %H:%M").to_string(),
        pf_token: None,
        hidden_extensions: Vec::new(),
    };
    // Saved before anything changes, so `normal` can clean up a partial switch.
    save_record(&record)?;
    if let Err(error) = switch_on(platform, &mut record) {
        say_err!("Switching game mode on failed; switching back.");
        if let Err(revert) = switch_off(platform, &record) {
            say_err!("{revert:#}");
        }
        return Err(error);
    }
    Ok(Some(record))
}

fn switch_on(platform: &dyn Platform, record: &mut GameRecord) -> Result<()> {
    platform.force_game_speed(&record.iface)?;
    // Without the link at game speed there is no game mode: the error makes
    // `begin` switch back, so a shaper never sits on a faster link.
    match wait_for_link(platform, record, |speed| speed == GAME_SPEED) {
        Some(GAME_SPEED) => {}
        Some(speed) => bail!(
            "{} did not reach {GAME_SPEED} Mb/s: it is at {speed} Mb/s",
            record.iface
        ),
        None => bail!(
            "{} did not reach {GAME_SPEED} Mb/s: its link is not back",
            record.iface
        ),
    }
    // Shaping goes on after the link is back: on macOS a link change can
    // reload the firewall, which would drop rules loaded before it.
    record.pf_token = platform.shape(&record.iface)?;
    save_record(record)?;
    say!(
        "Game mode on: {} at {GAME_SPEED} Mb/s, downloads shaped to {SHAPED_MBIT} Mbit/s.",
        record.iface
    );
    Ok(())
}

/// Undoes `record`, finding its interface by MAC address in case it was
/// renamed, and forgets it. The desktop comes back first: it needs no root
/// and is what a person sees. Then shaping goes, so a failure part-way leaves
/// at worst a slower link, never a shaper on a gigabit link. If an extension
/// cannot be switched back on, the record keeps it so `normal` can retry.
fn switch_off(platform: &dyn Platform, record: &GameRecord) -> Result<()> {
    let mut record = record.clone();
    let shown = show_extensions(&mut record);
    save_record(&record)?;

    root::authenticate()?;
    let current = find(platform, &record);
    let name = current
        .as_ref()
        .map_or(record.iface.as_str(), |iface| iface.name.as_str());
    platform.unshape(name, record.pf_token.as_deref());
    if current.is_some() {
        platform.restore_auto_speed(name)?;
        let speed = wait_for_link(platform, &record, |speed| speed != GAME_SPEED);
        say!(
            "Game mode off: {name} {}, download shaping removed.",
            describe_link(speed, speed.is_some())
        );
    } else {
        say!(
            "Game mode off. {} is gone, so its speed resets when it is plugged back in.",
            record.iface
        );
    }
    shown?;
    remove_record()
}

/// Switches the extensions in `record` back on, forgetting each one as it
/// comes back. Stops at the first failure, which leaves the rest recorded.
fn show_extensions(record: &mut GameRecord) -> Result<()> {
    while let Some(uuid) = record.hidden_extensions.first().cloned() {
        desktop::enable(&uuid)?;
        say!("{} is back on.", desktop::name(&uuid));
        record.hidden_extensions.remove(0);
    }
    Ok(())
}

fn pick(platform: &dyn Platform, requested: Option<&str>) -> Result<Iface> {
    iface::pick(
        platform.interfaces()?,
        platform.default_route().as_deref(),
        requested,
    )
}

fn find(platform: &dyn Platform, record: &GameRecord) -> Option<Iface> {
    iface::find(platform.interfaces().ok()?, &record.iface, &record.mac)
}

/// Waits until the link reports a speed `accept` takes, or until
/// `LINK_TIMEOUT`, and returns the last speed seen.
fn wait_for_link(
    platform: &dyn Platform,
    record: &GameRecord,
    accept: impl Fn(u32) -> bool,
) -> Option<u32> {
    let deadline = Instant::now() + link_timeout();
    // Give the old link time to drop, so it is not mistaken for the new one.
    thread::sleep(Duration::from_millis(500));
    loop {
        let speed = find(platform, record)
            .filter(|iface| iface.link)
            .and_then(|iface| iface.speed);
        if speed.is_some_and(&accept) || Instant::now() >= deadline {
            return speed;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn describe_link(speed: Option<u32>, link: bool) -> String {
    match (link, speed) {
        (true, Some(speed)) => format!("at {speed} Mb/s"),
        (true, None) => "has a link".to_owned(),
        (false, _) => "has no link".to_owned(),
    }
}

fn finish_command(
    command: &[String],
    outcome: Result<Option<ExitStatus>>,
    interrupted: bool,
) -> Result<()> {
    match outcome? {
        Some(status) if !status.success() && !interrupted => {
            bail!("{} exited with {status}", command[0])
        }
        _ => Ok(()),
    }
}

/// Whether `text` has a line that is exactly `item`: `gnome-extensions list`
/// and `flatpak ps` print one item per line.
fn has_line(text: &str, item: &str) -> bool {
    text.lines().any(|line| line.trim() == item)
}

fn load_record() -> Result<Option<GameRecord>> {
    let path = crate::paths::net_game_state_file()?;
    if !path.exists() {
        return Ok(None);
    }
    let data =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let record = serde_json::from_str(&data)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(record))
}

fn save_record(record: &GameRecord) -> Result<()> {
    let path = crate::paths::net_game_state_file()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(record)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn remove_record() -> Result<()> {
    let path = crate::paths::net_game_state_file()?;
    match fs::remove_file(&path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(error).with_context(|| format!("failed to remove {}", path.display()))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_the_link_for_people() {
        assert_eq!(describe_link(Some(100), true), "at 100 Mb/s");
        assert_eq!(describe_link(None, true), "has a link");
        assert_eq!(describe_link(None, false), "has no link");
    }

    #[test]
    fn a_record_from_before_extensions_and_tokens_still_parses() {
        let record: GameRecord = serde_json::from_str(
            r#"{"iface":"enp0s31f6","mac":"28:00:af:6f:ff:cb","started":"2026-11-12 19:40"}"#,
        )
        .unwrap();
        assert_eq!(record.iface, "enp0s31f6");
        assert_eq!(record.pf_token, None);
        assert!(record.hidden_extensions.is_empty());
    }

    #[test]
    fn finds_exact_lines_only() {
        let text = "caffeine@patapon.info\ndash-to-panel@jderose9.github.com\n";
        assert!(has_line(text, "dash-to-panel@jderose9.github.com"));
        assert!(!has_line(text, "dash-to-panel"));
        assert!(!has_line("", "com.nvidia.geforcenow"));
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_command_is_reported_unless_it_was_interrupted() {
        let failed = std::process::Command::new("false").status().unwrap();
        let command = ["false".to_owned()];
        assert!(finish_command(&command, Ok(Some(failed)), false).is_err());
        assert!(finish_command(&command, Ok(Some(failed)), true).is_ok());
        assert!(finish_command(&command, Ok(None), true).is_ok());
    }
}
