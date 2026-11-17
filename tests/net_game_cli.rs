//! `lum net game` and `lum net normal` end to end, against a fake `sudo` and
//! `/sys/class/net`: every command lum runs is logged instead of executed, so
//! nothing on the real machine changes.

#![cfg(target_os = "linux")]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{ChildStderr, ChildStdout, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use assert_cmd::Command;
use predicates::str::contains;
use shared_child::{SharedChild, unix::SharedChildExt};
use tempfile::TempDir;

/// What a `lum net game` on eth0 leaves behind until it is switched off.
const RECORD: &str = r#"{"iface":"eth0","mac":"02:00:00:00:00:01","started":"2026-11-12 20:00"}"#;

struct Fake {
    dir: TempDir,
}

impl Fake {
    fn new() -> Self {
        let fake = Self {
            dir: TempDir::new().unwrap(),
        };
        let bin = fake.path("bin");
        fs::create_dir_all(&bin).unwrap();
        // The fake sudo moves the fake link speed the way ethtool would. Once
        // a test creates `log.no-terminal` it fails, as the real one does
        // when there is no terminal left to ask for the password.
        fake.script(
            "sudo",
            r#"echo "sudo $*" >> "$LUM_FAKE_LOG"
[ -e "$LUM_FAKE_LOG.no-terminal" ] && exit 1
case "$*" in
  "ethtool -s eth0 speed 100 "*) echo 100 > "$LUM_NET_SYSFS/eth0/speed" ;;
  "ethtool -s eth0 autoneg on") echo 1000 > "$LUM_NET_SYSFS/eth0/speed" ;;
esac"#,
        );
        for tool in ["ethtool", "tc", "ip"] {
            fake.script(tool, "exit 0");
        }

        let eth0 = fake.path("sys/eth0");
        fs::create_dir_all(eth0.join("device")).unwrap();
        for (file, value) in [
            ("type", "1"),
            ("carrier", "1"),
            ("speed", "1000"),
            ("address", "02:00:00:00:00:01"),
        ] {
            fs::write(eth0.join(file), format!("{value}\n")).unwrap();
        }
        fake
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn script(&self, name: &str, body: &str) {
        let path = self.path("bin").join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn envs(&self) -> [(&'static str, PathBuf); 5] {
        let path = format!(
            "{}:{}",
            self.path("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        [
            ("PATH", path.into()),
            ("XDG_STATE_HOME", self.path("state")),
            ("LUM_NET_SYSFS", self.path("sys")),
            ("LUM_FAKE_LOG", self.path("log")),
            // No icon on the desk of whoever runs the tests.
            ("LUM_NO_STATUS_TRAY", "1".into()),
        ]
    }

    fn lum(&self, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("lum").unwrap();
        cmd.envs(self.envs()).args(args);
        cmd
    }

    /// Starts lum and returns once `running`, the line the wrapped command
    /// logs, shows that the command is running.
    fn start(&self, args: &[&str], running: &str) -> Running {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_lum"));
        command
            .envs(self.envs())
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let lum = SharedChild::spawn(&mut command).unwrap();
        let terminal = (lum.take_stdout().unwrap(), lum.take_stderr().unwrap());
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.log().iter().any(|line| line == running) {
            assert!(
                Instant::now() < deadline,
                "{running} never ran: {:?}",
                self.log()
            );
            thread::sleep(Duration::from_millis(50));
        }
        Running { lum, terminal }
    }

    fn log(&self) -> Vec<String> {
        fs::read_to_string(self.path("log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn record(&self) -> PathBuf {
        self.path("state/lum/net-game.json")
    }

    /// Leaves `record` behind, as a `lum net game` that never switched back.
    fn leave_record(&self, record: &str) {
        fs::create_dir_all(self.path("state/lum")).unwrap();
        fs::write(self.record(), record).unwrap();
    }

    fn set_speed(&self, speed: u32) {
        fs::write(self.path("sys/eth0/speed"), format!("{speed}\n")).unwrap();
    }
}

/// A lum that is wrapping a running command.
struct Running {
    lum: SharedChild,
    terminal: (ChildStdout, ChildStderr),
}

impl Running {
    /// What closing the terminal window does to lum: its output has nowhere
    /// to go any more, and it gets SIGHUP.
    fn lose_the_terminal(self) -> ExitStatus {
        drop(self.terminal);
        self.lum.send_signal(signal_hook::consts::SIGHUP).unwrap();
        self.lum.wait().unwrap()
    }
}

const SWITCH_ON: [&str; 8] = [
    "sudo -v",
    "sudo ethtool -s eth0 speed 100 duplex full autoneg on advertise 0x008",
    "sudo modprobe ifb numifbs=0",
    "sudo ip link add name lumifb0 type ifb",
    "sudo ip link set lumifb0 up",
    "sudo tc qdisc add dev eth0 handle ffff: ingress",
    "sudo tc filter add dev eth0 parent ffff: matchall action mirred egress redirect dev lumifb0",
    "sudo tc qdisc add dev lumifb0 root cake bandwidth 90mbit besteffort ingress",
];

const SWITCH_OFF: [&str; 4] = [
    "sudo -v",
    "sudo tc qdisc del dev eth0 ingress",
    "sudo ip link del lumifb0",
    "sudo ethtool -s eth0 autoneg on",
];

#[test]
fn the_status_tray_leaves_at_once_when_nothing_is_on() {
    let fake = Fake::new();
    // No record, so no icon to wait for a tray host to show.
    fake.lum(&["__status_tray"])
        .timeout(Duration::from_secs(20))
        .assert()
        .success();
}

#[test]
fn game_keeps_its_record_when_sudo_refuses_after_the_terminal_is_gone() {
    let fake = Fake::new();
    fake.script("game", "echo game >> \"$LUM_FAKE_LOG\"\nexec sleep 30");
    let running = fake.start(&["net", "game", "game"], "game");
    fs::write(fake.path("log.no-terminal"), "").unwrap();
    let status = running.lose_the_terminal();
    // An error, because the network could not be switched back, not a panic.
    assert_eq!(status.code(), Some(1), "{status}");

    let expected = [&SWITCH_ON[..], &["game", "sudo -v"]].concat();
    assert_eq!(fake.log(), expected);
    // The record still describes the network, for `lum net normal`.
    let record = fs::read_to_string(fake.record()).unwrap();
    assert!(record.contains("eth0"), "{record}");
}

#[test]
fn game_switches_back_after_its_terminal_is_gone() {
    let fake = Fake::new();
    fake.script("game", "echo game >> \"$LUM_FAKE_LOG\"\nexec sleep 30");
    let status = fake
        .start(&["net", "game", "game"], "game")
        .lose_the_terminal();
    assert!(status.success(), "{status}");

    let expected = [&SWITCH_ON[..], &["game"], &SWITCH_OFF[..]].concat();
    assert_eq!(fake.log(), expected);
    assert!(!fake.record().exists());
}

#[test]
fn game_checks_the_command_before_switching() {
    let fake = Fake::new();
    fake.lum(&["net", "game", "no-such-game"])
        .assert()
        .failure()
        .stderr(contains("no-such-game not found"));
    assert!(fake.log().is_empty());
    assert!(!fake.record().exists());
}

#[test]
fn game_switches_back_when_the_link_misses_game_speed() {
    let fake = Fake::new();
    // This sudo leaves the link at 1000 Mb/s whatever ethtool is told.
    fake.script("sudo", r#"echo "sudo $*" >> "$LUM_FAKE_LOG""#);
    fake.lum(&["net", "game"])
        .env("LUM_NET_LINK_TIMEOUT_MS", "1")
        .assert()
        .failure()
        .stderr(contains("eth0 did not reach 100 Mb/s"));

    // The speed change, no shaping, and straight back.
    let expected = [&SWITCH_ON[..2], &SWITCH_OFF[..]].concat();
    assert_eq!(fake.log(), expected);
    assert!(!fake.record().exists());
}

#[test]
fn game_undoes_a_half_finished_switch_before_switching_on() {
    let fake = Fake::new();
    // lum was killed after forcing the speed and before shaping: without
    // lumifb0 the record looks stale, but the link is still forced.
    fake.set_speed(100);
    fake.leave_record(RECORD);

    fake.lum(&["net", "game"])
        .assert()
        .success()
        .stdout(contains("Game mode on: eth0 at 100 Mb/s"));

    let expected = [&SWITCH_OFF[..], &SWITCH_ON[..]].concat();
    assert_eq!(fake.log(), expected);
}

#[test]
fn game_refuses_another_interface_while_it_is_on() {
    let fake = Fake::new();
    fs::create_dir_all(fake.path("sys/lumifb0")).unwrap();
    fake.leave_record(RECORD);

    fake.lum(&["net", "game", "--iface", "eth1"])
        .assert()
        .failure()
        .stderr(contains("game mode is already on for eth0"));
    fake.lum(&["net", "game", "--iface", "eth0"])
        .assert()
        .success()
        .stdout(contains("Game mode is already on for eth0."));
    assert!(fake.log().is_empty());
}

#[test]
fn normal_switches_back_on_what_a_killed_session_left_off() {
    let fake = Fake::new();
    fake.set_speed(100);
    fake.leave_record(RECORD);

    fake.lum(&["net", "normal"])
        .assert()
        .success()
        .stdout(contains("Game mode off: eth0 at 1000 Mb/s"));

    assert_eq!(fake.log(), SWITCH_OFF);
    assert!(!fake.record().exists());
}

#[test]
fn normal_ignores_a_bridge_with_the_same_address() {
    let fake = Fake::new();
    // A bridge over eth0 takes its address, and sorts before it.
    let br0 = fake.path("sys/br0");
    fs::create_dir_all(&br0).unwrap();
    for (file, value) in [
        ("type", "1"),
        ("carrier", "1"),
        ("address", "02:00:00:00:00:01"),
    ] {
        fs::write(br0.join(file), format!("{value}\n")).unwrap();
    }
    fake.set_speed(100);
    fake.leave_record(RECORD);

    fake.lum(&["net", "normal"])
        .assert()
        .success()
        .stdout(contains("Game mode off: eth0 at 1000 Mb/s"));

    assert_eq!(fake.log(), SWITCH_OFF);
    assert!(!fake.record().exists());
}
