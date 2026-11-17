//! macOS: `ifconfig` sets the media, and a dummynet pipe shapes downloads
//! through a rule in the `com.apple/lum` pf anchor. Nothing here edits
//! `/etc/pf.conf` or survives a reboot. Untested on real hardware so far.

use std::process::Command;

use anyhow::{Context, Result, ensure};

use super::{GameRecord, Platform, SHAPED_MBIT, iface, iface::Iface, root};

/// The stock `/etc/pf.conf` evaluates `dummynet-anchor "com.apple/*"`, so a
/// rule loaded here takes effect without touching the main ruleset, which
/// other software (VPN clients, for one) relies on.
const ANCHOR: &str = "com.apple/lum";
/// An arbitrary high pipe number, unlikely to clash with pipes other tools create.
const PIPE: &str = "64242";
/// dummynet has no fq_codel on macOS, so a short queue (in packets) stands in
/// for it: 50 full-size packets drain in under 7 ms at the shaped rate.
const QUEUE: &str = "50";

pub(super) struct MacOs;

impl Platform for MacOs {
    fn interfaces(&self) -> Result<Vec<Iface>> {
        Ok(iface::parse_ifconfig(&output("ifconfig", &["-a"])?))
    }

    fn default_route(&self) -> Option<String> {
        iface::macos_default_route(&output("route", &["-n", "get", "default"]).ok()?)
    }

    fn check_tools(&self) -> Result<()> {
        // ifconfig, route, pfctl and dnctl ship with macOS.
        Ok(())
    }

    fn force_game_speed(&self, iface: &str) -> Result<()> {
        ensure!(
            iface::supports_game_media(&output("ifconfig", &["-m", iface])?),
            "{iface} does not offer 100baseTX full-duplex"
        );
        // ifconfig instead of `networksetup -setMedia`, which saves the
        // setting in the network preferences and so would outlive a reboot.
        root::run(&[
            "ifconfig",
            iface,
            "media",
            "100baseTX",
            "mediaopt",
            "full-duplex",
        ])?;
        Ok(())
    }

    fn restore_auto_speed(&self, iface: &str) -> Result<()> {
        root::run(&["ifconfig", iface, "media", "autoselect"])?;
        Ok(())
    }

    fn shape(&self, iface: &str) -> Result<Option<String>> {
        let rate = format!("{SHAPED_MBIT}Mbit/s");
        root::run(&["dnctl", "pipe", PIPE, "config", "bw", &rate, "queue", QUEUE])?;
        root::run_with_input(
            &["pfctl", "-a", ANCHOR, "-f", "-"],
            Some(&dummynet_rule(iface)),
        )?;
        // -E takes a reference instead of switching pf on outright; releasing
        // it with -X turns pf off again only if nothing else holds one.
        let enabled = root::run(&["pfctl", "-E"])?;
        let token = iface::pf_token(&enabled);
        if token.is_none() {
            tracing::warn!(output = %enabled, "net: pfctl -E printed no token");
        }
        Ok(token)
    }

    fn unshape(&self, _iface: &str, token: Option<&str>) {
        root::run_quiet(&["pfctl", "-a", ANCHOR, "-f", "/dev/null"]);
        root::run_quiet(&["dnctl", "pipe", "delete", PIPE]);
        if let Some(token) = token {
            root::run_quiet(&["pfctl", "-X", token]);
        }
    }

    fn is_applied(&self, record: &GameRecord) -> bool {
        // Seeing the pipe needs root; a reboot or replug resets the media,
        // so media still set by hand means game mode is still on.
        self.interfaces().is_ok_and(|all| {
            all.iter()
                .any(|iface| iface.forced && iface.mac.eq_ignore_ascii_case(&record.mac))
        })
    }
}

fn dummynet_rule(iface: &str) -> String {
    format!("dummynet in quick on {iface} all pipe {PIPE}\n")
}

/// Runs a command that needs no root and returns its stdout.
fn output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    ensure!(
        output.status.success(),
        "{program} {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dummynet_rule_sends_incoming_traffic_through_the_pipe() {
        assert_eq!(
            dummynet_rule("en7"),
            "dummynet in quick on en7 all pipe 64242\n"
        );
    }
}
