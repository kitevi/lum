//! Finding the wired interface. Names are read fresh on every run and never
//! trusted across runs: a USB adapter or a reinstall can rename them.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail, ensure};

/// A network interface as game mode sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Iface {
    pub name: String,
    pub mac: String,
    /// Wired Ethernet: not Wi-Fi, a bridge, a VPN or another virtual device.
    pub wired: bool,
    pub link: bool,
    /// The current link speed in Mb/s, while there is a link.
    pub speed: Option<u32>,
    /// macOS: the media was set by hand instead of `autoselect`.
    pub forced: bool,
}

/// Picks the interface game mode works on: the one asked for, else the only
/// wired interface with a link, else the one of those carrying the default route.
pub(super) fn pick(
    all: Vec<Iface>,
    routed: Option<&str>,
    requested: Option<&str>,
) -> Result<Iface> {
    if let Some(name) = requested {
        let iface = all
            .into_iter()
            .find(|iface| iface.name == name)
            .with_context(|| format!("there is no interface named {name}"))?;
        ensure!(
            iface.wired,
            "{name} is not a wired Ethernet interface with a link"
        );
        ensure!(iface.link, "{name} has no link; is the cable plugged in?");
        return Ok(iface);
    }
    let mut linked: Vec<Iface> = all
        .into_iter()
        .filter(|iface| iface.wired && iface.link)
        .collect();
    if linked.len() > 1 {
        if let Some(index) = linked
            .iter()
            .position(|iface| Some(iface.name.as_str()) == routed)
        {
            return Ok(linked.swap_remove(index));
        }
        let names: Vec<&str> = linked.iter().map(|iface| iface.name.as_str()).collect();
        bail!(
            "several wired interfaces have a link ({}); choose one with --iface",
            names.join(", ")
        );
    }
    linked
        .pop()
        .context("no wired Ethernet interface has a link; game mode is for wired connections only")
}

/// Finds a recorded interface again by its MAC address, whatever it is called
/// now. Only wired interfaces count: a bridge, bond or VLAN on top of one has
/// its address and can sort first. Where two wired interfaces share an
/// address, as the two halves of a bond do, the recorded name decides.
pub(super) fn find(all: Vec<Iface>, name: &str, mac: &str) -> Option<Iface> {
    all.into_iter()
        .filter(|iface| {
            iface.wired
                && if mac.is_empty() {
                    iface.name == name
                } else {
                    iface.mac.eq_ignore_ascii_case(mac)
                }
        })
        .max_by_key(|iface| iface.name == name)
}

/// Linux: reads `/sys/class/net` (or a test copy of it).
pub(super) fn read_sysfs(root: &Path) -> Result<Vec<Iface>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let read = |file: &str| {
            fs::read_to_string(path.join(file))
                .ok()
                .map(|value| value.trim().to_owned())
        };
        // Only physical devices have `device`; Wi-Fi also has `wireless` or
        // `phy80211`; type 1 is Ethernet framing (ARPHRD_ETHER).
        let wired = path.join("device").exists()
            && !path.join("wireless").exists()
            && !path.join("phy80211").exists()
            && read("type").as_deref() == Some("1");
        // Reading `carrier` fails while the interface is down: no link.
        let link = read("carrier").as_deref() == Some("1");
        let speed = read("speed")
            .and_then(|speed| speed.parse::<i64>().ok())
            .filter(|speed| link && *speed > 0)
            .and_then(|speed| u32::try_from(speed).ok());
        found.push(Iface {
            name: name.to_owned(),
            mac: read("address").unwrap_or_default(),
            wired,
            link,
            speed,
            forced: false,
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// Linux: the default-route interface from `/proc/net/route`, lowest metric first.
pub(super) fn linux_default_route(table: &str) -> Option<String> {
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let default = fields.len() > 7 && fields[1] == "00000000" && fields[7] == "00000000";
            default.then(|| (fields[6].parse::<u32>().unwrap_or(u32::MAX), fields[0]))
        })
        .min()
        .map(|(_, name)| name.to_owned())
}

/// macOS: parses `ifconfig -a`. An interface counts as wired when its media
/// names an Ethernet type such as `1000baseT`; Wi-Fi only says `autoselect`.
pub(super) fn parse_ifconfig(text: &str) -> Vec<Iface> {
    let mut found: Vec<Iface> = Vec::new();
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) {
            if let Some((name, _)) = line.split_once(':') {
                found.push(Iface {
                    name: name.to_owned(),
                    mac: String::new(),
                    wired: false,
                    link: false,
                    speed: None,
                    forced: false,
                });
            }
            continue;
        }
        let Some(iface) = found.last_mut() else {
            continue;
        };
        let line = line.trim();
        if let Some(mac) = line.strip_prefix("ether ") {
            iface.mac = mac.trim().to_owned();
        } else if let Some(media) = line.strip_prefix("media:") {
            iface.forced = !media.trim().starts_with("autoselect");
            iface.speed = media_speed(media);
            iface.wired = iface.speed.is_some();
        } else if let Some(status) = line.strip_prefix("status:") {
            iface.link = status.trim() == "active";
        }
    }
    for iface in &mut found {
        if !iface.link {
            iface.speed = None;
        }
    }
    found
}

/// The speed in a macOS media description: `autoselect (1000baseT <full-duplex>)`
/// gives 1000, `100baseTX <full-duplex>` 100, `autoselect (10Gbase-T)` 10000.
fn media_speed(media: &str) -> Option<u32> {
    let media = media.trim();
    let active = match (media.find('('), media.find(')')) {
        (Some(open), Some(close)) if open < close => &media[open + 1..close],
        _ => media,
    };
    let subtype = active.split_whitespace().next()?.to_ascii_lowercase();
    let rate = &subtype[..subtype.find("base")?];
    match rate.strip_suffix('g') {
        Some(gigabits) => gigabits.parse::<u32>().ok()?.checked_mul(1000),
        None => rate.parse().ok(),
    }
}

/// macOS: whether `ifconfig -m <iface>` lists 100 Mb/s full duplex.
pub(super) fn supports_game_media(text: &str) -> bool {
    text.lines().any(|line| {
        line.trim()
            .starts_with("media 100baseTX mediaopt full-duplex")
    })
}

/// macOS: the interface in `route -n get default`.
pub(super) fn macos_default_route(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("interface:"))
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

/// macOS: the reference `pfctl -E` prints as `Token : 12345`.
pub(super) fn pf_token(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("Token"))
        .and_then(|rest| rest.split_once(':'))
        .map(|(_, token)| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str, wired: bool, link: bool) -> Iface {
        Iface {
            name: name.to_owned(),
            mac: format!("{name}-mac"),
            wired,
            link,
            speed: link.then_some(1000),
            forced: false,
        }
    }

    #[test]
    fn picks_the_only_wired_interface_with_a_link() {
        let all = vec![
            iface("docker0", false, true),
            iface("enp0s31f6", true, true),
            iface("enx001122334455", true, false),
            iface("wlan0", false, true),
        ];
        assert_eq!(pick(all, Some("wlan0"), None).unwrap().name, "enp0s31f6");
    }

    #[test]
    fn several_linked_interfaces_fall_back_to_the_default_route() {
        let all = vec![iface("eth0", true, true), iface("eth1", true, true)];
        assert_eq!(pick(all.clone(), Some("eth1"), None).unwrap().name, "eth1");
        let error = pick(all, None, None).unwrap_err().to_string();
        assert!(error.contains("eth0, eth1"), "{error}");
        assert!(error.contains("--iface"), "{error}");
    }

    #[test]
    fn without_a_wired_link_there_is_nothing_to_pick() {
        let all = vec![iface("wlan0", false, true), iface("eth0", true, false)];
        let error = pick(all, Some("wlan0"), None).unwrap_err().to_string();
        assert!(error.contains("wired"), "{error}");
    }

    #[test]
    fn a_requested_interface_must_be_wired_and_linked() {
        let all = vec![iface("wlan0", false, true), iface("eth0", true, false)];
        assert!(pick(all.clone(), None, Some("eth0")).is_err());
        assert!(pick(all.clone(), None, Some("wlan0")).is_err());
        assert!(pick(all.clone(), None, Some("nope")).is_err());
        let all = vec![iface("eth0", true, true), iface("eth1", true, true)];
        assert_eq!(pick(all, None, Some("eth1")).unwrap().name, "eth1");
    }

    fn with_mac(name: &str, wired: bool, mac: &str) -> Iface {
        Iface {
            mac: mac.to_owned(),
            ..iface(name, wired, true)
        }
    }

    #[test]
    fn finds_the_wired_interface_among_those_sharing_its_address() {
        // A bridge over the interface takes its address and sorts first.
        let all = vec![
            with_mac("br0", false, "aa:bb"),
            with_mac("enp3s0", true, "AA:BB"),
            with_mac("enp3s0.10", false, "aa:bb"),
        ];
        assert_eq!(find(all, "enp3s0", "aa:bb").unwrap().name, "enp3s0");
        // Unplugged: the bridge that is left with the address is not it.
        let left = vec![with_mac("br0", false, "aa:bb")];
        assert_eq!(find(left, "enx001122334455", "aa:bb"), None);
    }

    #[test]
    fn finds_a_renamed_interface_by_its_address() {
        let all = vec![
            with_mac("eth0", true, "cc:dd"),
            with_mac("eth1", true, "aa:bb"),
        ];
        assert_eq!(find(all.clone(), "eth0", "aa:bb").unwrap().name, "eth1");
        // Without a recorded address there is only the name to go by.
        assert_eq!(find(all, "eth0", "").unwrap().name, "eth0");
    }

    #[test]
    fn the_recorded_name_decides_between_wired_interfaces_sharing_an_address() {
        // The two halves of a bond.
        let all = vec![
            with_mac("enp3s0", true, "aa:bb"),
            with_mac("enp4s0", true, "aa:bb"),
        ];
        assert_eq!(find(all.clone(), "enp3s0", "aa:bb").unwrap().name, "enp3s0");
        assert_eq!(find(all, "enp4s0", "aa:bb").unwrap().name, "enp4s0");
    }

    fn fake_iface(root: &Path, name: &str, files: &[(&str, &str)], dirs: &[&str]) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        for (file, content) in files {
            fs::write(dir.join(file), content).unwrap();
        }
        for sub in dirs {
            fs::create_dir_all(dir.join(sub)).unwrap();
        }
    }

    #[test]
    fn reads_sysfs_and_tells_wired_from_wireless_and_virtual() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let ether = [("type", "1\n"), ("carrier", "1\n"), ("speed", "100\n")];
        fake_iface(
            root,
            "enp0s31f6",
            &[
                ether[0],
                ether[1],
                ether[2],
                ("address", "28:00:af:6f:ff:cb\n"),
            ],
            &["device"],
        );
        fake_iface(root, "wlan0", &ether, &["device", "phy80211"]);
        fake_iface(root, "docker0", &ether, &[]);
        fake_iface(root, "lo", &[("type", "772\n"), ("carrier", "1\n")], &[]);
        // A cable-less port: the kernel reports speed -1.
        fake_iface(
            root,
            "eth1",
            &[("type", "1\n"), ("carrier", "0\n"), ("speed", "-1\n")],
            &["device"],
        );

        let found = read_sysfs(root).unwrap();
        let get = |name: &str| found.iter().find(|iface| iface.name == name).unwrap();
        assert_eq!(
            *get("enp0s31f6"),
            Iface {
                name: "enp0s31f6".into(),
                mac: "28:00:af:6f:ff:cb".into(),
                wired: true,
                link: true,
                speed: Some(100),
                forced: false,
            }
        );
        assert!(!get("wlan0").wired);
        assert!(!get("docker0").wired);
        assert!(!get("lo").wired);
        assert!(get("eth1").wired && !get("eth1").link);
        assert_eq!(get("eth1").speed, None);
    }

    #[test]
    fn linux_default_route_takes_the_lowest_metric() {
        let table = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
            wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
            enp0s31f6\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
            enp0s31f6\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";
        assert_eq!(linux_default_route(table).as_deref(), Some("enp0s31f6"));
        assert_eq!(linux_default_route("Iface\tDestination\n"), None);
    }

    const IFCONFIG: &str = "\
lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384
\toptions=1203<RXCSUM,TXCSUM,TXSTATUS,SW_TIMESTAMP>
\tinet 127.0.0.1 netmask 0xff000000
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
\toptions=6460<TSO4,TSO6,CHANNEL_IO,PARTIAL_CSUM,ZEROINVERT_CSUM>
\tether 3c:22:fb:12:34:56
\tinet 192.168.1.20 netmask 0xffffff00 broadcast 192.168.1.255
\tmedia: autoselect
\tstatus: active
en7: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
\tether a0:ce:c8:01:02:03
\tmedia: autoselect (1000baseT <full-duplex,flow-control>)
\tstatus: active
en8: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
\tether a0:ce:c8:0a:0b:0c
\tmedia: 100baseTX <full-duplex>
\tstatus: active
en9: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
\tether a0:ce:c8:0d:0e:0f
\tmedia: autoselect (none)
\tstatus: inactive
bridge0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
\tether 36:1c:2d:00:00:00
\tmedia: <unknown type>
\tstatus: inactive
utun0: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1380
";

    #[test]
    fn parses_ifconfig_and_finds_wired_links() {
        let found = parse_ifconfig(IFCONFIG);
        let names: Vec<&str> = found.iter().map(|iface| iface.name.as_str()).collect();
        assert_eq!(
            names,
            ["lo0", "en0", "en7", "en8", "en9", "bridge0", "utun0"]
        );
        let get = |name: &str| found.iter().find(|iface| iface.name == name).unwrap();

        // Wi-Fi: active, but its media names no Ethernet type.
        assert!(!get("en0").wired && get("en0").link);
        assert_eq!(
            *get("en7"),
            Iface {
                name: "en7".into(),
                mac: "a0:ce:c8:01:02:03".into(),
                wired: true,
                link: true,
                speed: Some(1000),
                forced: false,
            }
        );
        assert!(get("en8").forced);
        assert_eq!(get("en8").speed, Some(100));
        assert!(!get("en9").wired && !get("en9").link);
        assert!(!get("bridge0").wired);

        // Two wired links and the default route on Wi-Fi: lum must not guess.
        assert!(pick(found.clone(), Some("en0"), None).is_err());
        assert_eq!(pick(found, Some("en7"), None).unwrap().name, "en7");
    }

    #[test]
    fn reads_speeds_from_macos_media() {
        assert_eq!(
            media_speed(" autoselect (1000baseT <full-duplex>)"),
            Some(1000)
        );
        assert_eq!(media_speed(" 100baseTX <full-duplex>"), Some(100));
        assert_eq!(
            media_speed(" autoselect (2500Base-T <full-duplex>)"),
            Some(2500)
        );
        assert_eq!(
            media_speed(" autoselect (10Gbase-T <full-duplex>)"),
            Some(10_000)
        );
        assert_eq!(
            media_speed(" autoselect (10baseT/UTP <half-duplex>)"),
            Some(10)
        );
        assert_eq!(media_speed(" autoselect"), None);
        assert_eq!(media_speed(" autoselect (none)"), None);
        assert_eq!(media_speed(" <unknown type>"), None);
    }

    #[test]
    fn checks_macos_supported_media() {
        let usb = "en7: flags=8863<UP> mtu 1500\n\
            \tsupported media:\n\
            \t\tmedia autoselect\n\
            \t\tmedia 100baseTX mediaopt full-duplex,flow-control\n\
            \t\tmedia 1000baseT mediaopt full-duplex\n";
        assert!(supports_game_media(usb));
        assert!(!supports_game_media(
            "\tsupported media:\n\t\tmedia autoselect\n"
        ));
    }

    #[test]
    fn reads_the_macos_default_route() {
        let text = "   route to: default\ndestination: default\n    gateway: 192.168.1.1\n  interface: en7\n";
        assert_eq!(macos_default_route(text).as_deref(), Some("en7"));
        assert_eq!(
            macos_default_route("route: writing to routing socket: not in table\n"),
            None
        );
    }

    #[test]
    fn reads_the_pf_enable_token() {
        let text = "No ALTQ support in kernel\nALTQ related functions disabled\npf enabled\nToken : 13254978341234\n";
        assert_eq!(pf_token(text).as_deref(), Some("13254978341234"));
        assert_eq!(pf_token("pf enabled\n"), None);
    }
}
