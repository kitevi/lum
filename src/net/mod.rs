//! `lum net`: switch shaping of the wired connection on and off.
//!
//! Shaping forces the link to 100 Mb/s and shapes downloads just under that,
//! so bursts queue at the computer instead of being dropped upstream, and
//! downloads cannot fill the router's queue. See `README.md` and ADR-0024.

/// `println!` that drops the line when it cannot be written. `println!`
/// panics instead, and with the terminal gone that would end lum before it
/// switches back. Everything that switches shaping prints through this.
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

mod iface;
mod linux;
mod macos;
mod probe;
mod root;
mod wrap;

use std::{
    ffi::OsStr,
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

/// The link speed shaping forces, in Mb/s.
const FORCED_SPEED: u32 = 100;
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
        NetCommand::On { interface, command } => on(platform, interface.as_deref(), &command),
        NetCommand::Off => off(platform),
        NetCommand::Status => status(platform),
        NetCommand::Test { interface, seconds } => {
            probe::run(platform, interface.as_deref(), seconds.0)
        }
    }
}

/// What shaping needs from the operating system.
trait Platform {
    /// Every network interface, wired or not, with its current link.
    fn interfaces(&self) -> Result<Vec<Iface>>;
    /// The interface that carries the default route, if any.
    fn default_route(&self) -> Option<String>;
    /// Fails with an install hint when a tool shaping needs is missing.
    fn check_tools(&self) -> Result<()>;
    fn force_speed(&self, iface: &str) -> Result<()>;
    fn restore_auto_speed(&self, iface: &str) -> Result<()>;
    /// Starts shaping downloads on `iface`. Returns a token `unshape` needs.
    fn shape(&self, iface: &str) -> Result<Option<String>>;
    /// Removes download shaping. Best effort: part of it may already be gone.
    fn unshape(&self, iface: &str, token: Option<&str>);
    /// Whether the system still shows the changes `record` describes. False
    /// after a reboot, which undoes them all.
    fn is_applied(&self, record: &ShapingRecord) -> bool;
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

/// The wired interfaces' names, for completing `--interface`. None where lum net
/// is unsupported or the interfaces cannot be read.
pub(crate) fn wired_interfaces() -> Vec<String> {
    platform()
        .and_then(|platform| platform.interfaces())
        .unwrap_or_default()
        .into_iter()
        .filter(|iface| iface.wired)
        .map(|iface| iface.name)
        .collect()
}

/// What `lum net on` changed, kept in `net-shaping.json` so `lum net off`
/// can undo it even from another terminal or after lum was killed.
#[derive(Debug, Serialize, Deserialize)]
struct ShapingRecord {
    iface: String,
    /// Finds the interface again if it was renamed or replugged.
    mac: String,
    started: String,
    /// macOS: the pf enable reference to release.
    #[serde(default)]
    pf_token: Option<String>,
}

fn on(platform: &dyn Platform, requested: Option<&str>, command: &[String]) -> Result<()> {
    if let Some(program) = command.first() {
        // Before anything changes: a mistyped name would otherwise cost a
        // switch on and straight back off.
        which::which(program)
            .with_context(|| format!("{program} not found; shaping was left as it was"))?;
    }

    let trap = if command.is_empty() {
        Trap::none()
    } else {
        Trap::install()?
    };
    let began = begin(platform, requested)?;
    // Shaping is on now, newly or already: a tray icon says so until it is off again.
    crate::tray::status::show();
    let Some(record) = began else {
        if command.is_empty() {
            return Ok(());
        }
        say!("It stays on after the command exits.");
        return finish_command(command, wrap::run(command, &trap), trap.fired());
    };
    if command.is_empty() {
        say!("Run `lum net off` to switch back.");
        return Ok(());
    }

    let outcome = {
        let _sudo = root::keep_alive();
        wrap::run(command, &trap)
    };
    say!("Switching shaping off.");
    switch_off(platform, &record)
        .context("shaping is still on; run `lum net off` to switch back")?;
    finish_command(command, outcome, trap.fired())
}

fn off(platform: &dyn Platform) -> Result<()> {
    let Some(record) = load_record()? else {
        say!("Shaping is not on.");
        return Ok(());
    };
    switch_off(platform, &record)
}

fn status(platform: &dyn Platform) -> Result<()> {
    match load_record()? {
        Some(record) => {
            if platform.is_applied(&record) {
                say!(
                    "Shaping: on for {} since {} ({FORCED_SPEED} Mb/s, downloads shaped to {SHAPED_MBIT} Mbit/s)",
                    record.iface,
                    record.started
                );
            } else {
                say!(
                    "Shaping: off (a record for {} from {} is left over; `lum net off` clears it)",
                    record.iface,
                    record.started
                );
            }
        }
        None => say!("Shaping: off"),
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
    let networks = sysinfo::Networks::new_with_refreshed_list().ok();
    for iface in wired {
        let route = if routed.as_deref() == Some(iface.name.as_str()) {
            ", default route"
        } else {
            ""
        };
        let errors = networks
            .as_ref()
            .and_then(|networks| networks.get(OsStr::new(&iface.name)))
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

/// Switches shaping on and returns its record, or `None` if it was on
/// already, in which case whoever switched it on also switches it off.
fn begin(platform: &dyn Platform, requested: Option<&str>) -> Result<Option<ShapingRecord>> {
    if let Some(record) = load_record()? {
        if platform.is_applied(&record) {
            if let Some(requested) = requested {
                ensure!(
                    requested == record.iface,
                    "shaping is already on for {}; run `lum net off` before using {requested}",
                    record.iface
                );
            }
            say!("Shaping is already on for {}.", record.iface);
            return Ok(None);
        }
        // A reboot undid the network changes, but a lum killed part-way
        // through switching on can leave the link forced with no shaper to
        // show for it. So the record is undone in full, as `off` does.
        say!("Clearing what earlier shaping left behind.");
        switch_off(platform, &record)?;
    }

    platform.check_tools()?;
    let iface = pick(platform, requested)?;
    root::authenticate()?;
    let mut record = ShapingRecord {
        iface: iface.name.clone(),
        mac: iface.mac.clone(),
        started: jiff::Zoned::now().strftime("%Y-%m-%d %H:%M").to_string(),
        pf_token: None,
    };
    // Saved before anything changes, so `off` can clean up a partial switch.
    save_record(&record)?;
    if let Err(error) = switch_on(platform, &mut record) {
        say_err!("Switching shaping on failed; switching back.");
        if let Err(revert) = switch_off(platform, &record) {
            say_err!("{revert:#}");
        }
        return Err(error);
    }
    Ok(Some(record))
}

fn switch_on(platform: &dyn Platform, record: &mut ShapingRecord) -> Result<()> {
    platform.force_speed(&record.iface)?;
    // Without the link at the forced speed there is no download shaping: the error makes
    // `begin` switch back, so a shaper never sits on a faster link.
    match wait_for_link(platform, record, |speed| speed == FORCED_SPEED) {
        Some(FORCED_SPEED) => {}
        Some(speed) => bail!(
            "{} did not reach {FORCED_SPEED} Mb/s: it is at {speed} Mb/s",
            record.iface
        ),
        None => bail!(
            "{} did not reach {FORCED_SPEED} Mb/s: its link is not back",
            record.iface
        ),
    }
    // Download shaping goes on after the link is back: on macOS a link change can
    // reload the firewall, which would drop rules loaded before it.
    record.pf_token = platform.shape(&record.iface)?;
    save_record(record)?;
    say!(
        "Shaping on: {} at {FORCED_SPEED} Mb/s, downloads shaped to {SHAPED_MBIT} Mbit/s.",
        record.iface
    );
    Ok(())
}

/// Undoes `record`, finding its interface by MAC address in case it was
/// renamed, and forgets it. Download shaping goes first, so a failure part-way leaves
/// at worst a slower link, never a shaper on a gigabit link.
fn switch_off(platform: &dyn Platform, record: &ShapingRecord) -> Result<()> {
    root::authenticate()?;
    let current = find(platform, record);
    let name = current
        .as_ref()
        .map_or(record.iface.as_str(), |iface| iface.name.as_str());
    platform.unshape(name, record.pf_token.as_deref());
    if current.is_some() {
        platform.restore_auto_speed(name)?;
        let speed = wait_for_link(platform, record, |speed| speed != FORCED_SPEED);
        say!(
            "Shaping off: {name} {}, download shaping removed.",
            describe_link(speed, speed.is_some())
        );
    } else {
        say!(
            "Shaping off. {} is gone, so its speed resets when it is plugged back in.",
            record.iface
        );
    }
    remove_record()
}

fn pick(platform: &dyn Platform, requested: Option<&str>) -> Result<Iface> {
    iface::pick(
        platform.interfaces()?,
        platform.default_route().as_deref(),
        requested,
    )
}

fn find(platform: &dyn Platform, record: &ShapingRecord) -> Option<Iface> {
    iface::find(platform.interfaces().ok()?, &record.iface, &record.mac)
}

/// Waits until the link reports a speed `accept` takes, or until
/// `LINK_TIMEOUT`, and returns the last speed seen.
fn wait_for_link(
    platform: &dyn Platform,
    record: &ShapingRecord,
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

/// Whether `lum net on` has left a record: cheap to find out, unlike [`tray_line`]. Shaping
/// may still be off, as after a reboot.
pub(crate) fn shaping_recorded() -> bool {
    crate::paths::net_shaping_state_file().is_ok_and(|path| path.exists())
}

/// The status tray's line for shaping, or `None` when it is off, as it is when the record is
/// left over from before a reboot.
pub(crate) fn tray_line() -> Option<String> {
    let platform = platform().ok()?;
    let record = load_record().ok()??;
    platform.is_applied(&record).then(|| shaping_line(&record))
}

fn shaping_line(record: &ShapingRecord) -> String {
    format!("Shaping on for {} since {}", record.iface, record.started)
}

fn load_record() -> Result<Option<ShapingRecord>> {
    let path = crate::paths::net_shaping_state_file()?;
    if !path.exists() {
        return Ok(None);
    }
    let data =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let record = serde_json::from_str(&data)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(record))
}

fn save_record(record: &ShapingRecord) -> Result<()> {
    let path = crate::paths::net_shaping_state_file()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(record)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn remove_record() -> Result<()> {
    let path = crate::paths::net_shaping_state_file()?;
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
    fn the_tray_line_names_the_interface_and_when_shaping_began() {
        let record = ShapingRecord {
            iface: "eth0".to_owned(),
            mac: "02:00:00:00:00:01".to_owned(),
            started: "2026-11-12 20:00".to_owned(),
            pf_token: None,
        };
        assert_eq!(
            shaping_line(&record),
            "Shaping on for eth0 since 2026-11-12 20:00"
        );
    }

    #[test]
    fn a_record_without_a_pf_token_still_parses() {
        let record: ShapingRecord = serde_json::from_str(
            r#"{"iface":"enp0s31f6","mac":"28:00:af:6f:ff:cb","started":"2026-11-12 19:40"}"#,
        )
        .unwrap();
        assert_eq!(record.iface, "enp0s31f6");
        assert_eq!(record.pf_token, None);
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
