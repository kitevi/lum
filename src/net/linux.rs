//! Linux: ethtool sets the link speed, and CAKE shapes downloads on an IFB
//! device that the wired interface's incoming traffic is redirected through.
//! None of it survives a reboot.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};

use super::{GAME_SPEED, GameRecord, Platform, SHAPED_MBIT, iface, iface::Iface, root};

const SYSFS: &str = "/sys/class/net";
/// A fixed name: deriving it from the interface (`ifb4<iface>`) breaks on USB
/// adapters, whose 15-character `enx<mac>` names leave no room under IFNAMSIZ.
const IFB: &str = "lumifb0";

pub(super) struct Linux;

impl Platform for Linux {
    fn interfaces(&self) -> Result<Vec<Iface>> {
        iface::read_sysfs(&sysfs())
    }

    fn default_route(&self) -> Option<String> {
        iface::linux_default_route(&fs::read_to_string("/proc/net/route").ok()?)
    }

    fn check_tools(&self) -> Result<()> {
        for (tool, package) in [
            ("ethtool", "ethtool"),
            ("tc", "iproute2"),
            ("ip", "iproute2"),
        ] {
            which::which(tool)
                .with_context(|| format!("{tool} not found; install the {package} package"))?;
        }
        Ok(())
    }

    fn force_game_speed(&self, iface: &str) -> Result<()> {
        let speed = GAME_SPEED.to_string();
        // 0x008 advertises 100baseT/Full only, so autonegotiation still runs
        // (forcing the speed with autoneg off can leave a duplex mismatch).
        root::run(&[
            "ethtool",
            "-s",
            iface,
            "speed",
            &speed,
            "duplex",
            "full",
            "autoneg",
            "on",
            "advertise",
            "0x008",
        ])?;
        Ok(())
    }

    fn restore_auto_speed(&self, iface: &str) -> Result<()> {
        // Autonegotiation without a speed advertises every supported mode again.
        root::run(&["ethtool", "-s", iface, "autoneg", "on"])?;
        Ok(())
    }

    fn shape(&self, iface: &str) -> Result<Option<String>> {
        // Without numifbs=0, loading the module also creates ifb0 and ifb1.
        root::run_quiet(&["modprobe", "ifb", "numifbs=0"]);
        for args in shaping_commands(iface) {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            root::run(&args)?;
        }
        Ok(None)
    }

    fn unshape(&self, iface: &str, _token: Option<&str>) {
        root::run_quiet(&["tc", "qdisc", "del", "dev", iface, "ingress"]);
        root::run_quiet(&["ip", "link", "del", IFB]);
    }

    fn is_applied(&self, _record: &GameRecord) -> bool {
        sysfs().join(IFB).exists()
    }
}

/// `/sys/class/net`, or the fake one in `$LUM_NET_SYSFS` that tests use.
fn sysfs() -> PathBuf {
    std::env::var_os("LUM_NET_SYSFS").map_or_else(|| PathBuf::from(SYSFS), PathBuf::from)
}

/// Redirects everything `iface` receives through the IFB device, whose
/// outgoing queue is CAKE at `SHAPED_MBIT`. `ingress` makes CAKE count the
/// packets it drops, which suits shaping traffic that has already arrived.
fn shaping_commands(iface: &str) -> Vec<Vec<String>> {
    let rate = format!("{SHAPED_MBIT}mbit");
    [
        vec!["ip", "link", "add", "name", IFB, "type", "ifb"],
        vec!["ip", "link", "set", IFB, "up"],
        vec![
            "tc", "qdisc", "add", "dev", iface, "handle", "ffff:", "ingress",
        ],
        vec![
            "tc", "filter", "add", "dev", iface, "parent", "ffff:", "matchall", "action", "mirred",
            "egress", "redirect", "dev", IFB,
        ],
        vec![
            "tc",
            "qdisc",
            "add",
            "dev",
            IFB,
            "root",
            "cake",
            "bandwidth",
            &rate,
            "besteffort",
            "ingress",
        ],
    ]
    .into_iter()
    .map(|args| args.into_iter().map(str::to_owned).collect())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ifb_name_fits_the_kernel_limit() {
        // IFNAMSIZ is 16 including the trailing NUL.
        assert!(IFB.len() <= 15);
    }

    #[test]
    fn shaping_redirects_the_interface_into_cake_on_the_ifb() {
        let commands: Vec<String> = shaping_commands("enx001122334455")
            .iter()
            .map(|args| args.join(" "))
            .collect();
        assert_eq!(
            commands,
            [
                "ip link add name lumifb0 type ifb",
                "ip link set lumifb0 up",
                "tc qdisc add dev enx001122334455 handle ffff: ingress",
                "tc filter add dev enx001122334455 parent ffff: matchall action mirred egress redirect dev lumifb0",
                "tc qdisc add dev lumifb0 root cake bandwidth 90mbit besteffort ingress",
            ]
        );
    }
}
