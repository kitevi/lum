//! `lum net test`: latency idle and while downloading (bufferbloat), and the
//! receive errors the interface counts meanwhile.

use std::{
    io::Read as _,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, ensure};

use super::{Platform, describe_link, load_record, pick};

/// Latency is the time a TCP handshake takes. Unlike ping it needs no root on
/// macOS, whose ping refuses intervals under a second for normal users.
const PROBE_TARGET: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443);
const PROBE_INTERVAL: Duration = Duration::from_millis(250);
/// A probe without an answer by then counts as lost.
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const IDLE: Duration = Duration::from_secs(5);
/// Downloads ramp up before the loaded probes start.
const WARMUP: Duration = Duration::from_secs(2);
/// Large public test files on separate hosts, so one slow server does not
/// leave the link half loaded.
const LOAD_URLS: [&str; 3] = [
    "https://fsn1-speed.hetzner.com/10GB.bin",
    "https://nbg1-speed.hetzner.com/10GB.bin",
    "https://proof.ovh.net/files/10Gb.dat",
];

pub(super) fn run(platform: &dyn Platform, requested: Option<&str>, seconds: u64) -> Result<()> {
    ensure!(
        (5..=120).contains(&seconds),
        "--seconds must be between 5 and 120"
    );
    let iface = pick(platform, requested)?;
    let game = load_record()?.is_some_and(|record| platform.is_applied(&record));
    println!(
        "{} ({}) {}, game mode {}",
        iface.name,
        iface.mac,
        describe_link(iface.speed, iface.link),
        if game { "on" } else { "off" }
    );

    println!("Measuring idle latency for {}s...", IDLE.as_secs());
    let idle = latency(Instant::now() + IDLE);

    println!("Downloading for {seconds}s...");
    let before = Counters::read(&iface.name);
    let window = Duration::from_secs(seconds);
    let deadline = Instant::now() + window;
    let (downloaded, loaded) = thread::scope(|scope| {
        let workers: Vec<_> = LOAD_URLS
            .iter()
            .map(|url| scope.spawn(move || download_until(url, deadline)))
            .collect();
        thread::sleep(WARMUP);
        let loaded = latency(deadline);
        let downloaded: u64 = workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or(0))
            .sum();
        (downloaded, loaded)
    });
    let after = Counters::read(&iface.name);
    ensure!(
        downloaded > 0,
        "no download server answered, so the link was never loaded; try again later"
    );

    println!();
    println!("Idle        {}", idle.summary());
    println!("Under load  {}", loaded.summary());
    println!(
        "Download    {:.0} Mb/s",
        megabits_per_second(downloaded, window)
    );
    let errors = before.zip(after).map(|(before, after)| {
        (
            after.errors.saturating_sub(before.errors),
            after.packets.saturating_sub(before.packets),
        )
    });
    if let Some((errors, packets)) = errors {
        println!(
            "Rx errors   {errors} in {packets} packets received ({:.3}%)",
            percent(errors, packets)
        );
    }

    println!();
    if let (Some(idle), Some(loaded)) = (idle.average_ms(), loaded.average_ms()) {
        let added = loaded - idle;
        let level = bufferbloat(added);
        let hint = if level != "low" && !game {
            " 'lum net game' shapes downloads to keep it low."
        } else {
            ""
        };
        println!("Bufferbloat: {level}, {added:+.1} ms under load.{hint}");
    }
    if let Some((errors, packets)) = errors {
        println!("Receive errors: {}.", errors_verdict(errors, packets));
    }
    if cfg!(target_os = "macos") {
        println!("For Apple's own measurement, run: networkQuality -s");
    }
    Ok(())
}

#[derive(Debug, Default)]
struct Latency {
    samples: Vec<Duration>,
    lost: usize,
}

impl Latency {
    fn average_ms(&self) -> Option<f64> {
        let total: f64 = self.samples.iter().map(Duration::as_secs_f64).sum();
        (!self.samples.is_empty()).then(|| total * 1000.0 / self.samples.len() as f64)
    }

    fn worst_ms(&self) -> Option<f64> {
        self.samples
            .iter()
            .max()
            .map(|worst| worst.as_secs_f64() * 1000.0)
    }

    fn summary(&self) -> String {
        let probes = self.samples.len() + self.lost;
        match (self.average_ms(), self.worst_ms()) {
            (Some(average), Some(worst)) => format!(
                "{average:.1} ms average, {worst:.1} ms worst, {} of {probes} probes lost",
                self.lost
            ),
            _ => format!("no answers, {} of {probes} probes lost", self.lost),
        }
    }
}

/// Probes every `PROBE_INTERVAL` until `until`.
fn latency(until: Instant) -> Latency {
    let mut result = Latency::default();
    while Instant::now() < until {
        let start = Instant::now();
        match TcpStream::connect_timeout(&PROBE_TARGET, PROBE_TIMEOUT) {
            Ok(_) => result.samples.push(start.elapsed()),
            Err(_) => result.lost += 1,
        }
        if let Some(rest) = PROBE_INTERVAL.checked_sub(start.elapsed()) {
            thread::sleep(rest);
        }
    }
    result
}

/// Downloads `url` until `deadline` and returns the bytes received. The
/// deadline bounds the whole request, so a server that never answers or
/// stalls ends with the others instead of holding up the result. A server
/// that fails only lowers the load; the others carry on.
fn download_until(url: &str, deadline: Instant) -> u64 {
    let request = crate::github::agent()
        .get(url)
        .config()
        .timeout_global(Some(deadline.saturating_duration_since(Instant::now())))
        .build();
    let mut reader = match request.call() {
        Ok(response) => response.into_body().into_reader(),
        Err(error) => {
            tracing::warn!(url, error = %error, "net: load download failed");
            return 0;
        }
    };
    let mut buffer = vec![0; 64 * 1024];
    let mut total = 0;
    while Instant::now() < deadline {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => total += read as u64,
            // Reading into the deadline is how a download normally ends.
            Err(_) if Instant::now() >= deadline => break,
            Err(error) => {
                tracing::warn!(url, error = %error, "net: load download stopped");
                break;
            }
        }
    }
    total
}

/// The rate of `bytes` received over the `window` the downloads were given,
/// not over the time until the last of them returned.
fn megabits_per_second(bytes: u64, window: Duration) -> f64 {
    bytes as f64 * 8.0 / window.as_secs_f64() / 1_000_000.0
}

#[derive(Debug, Clone, Copy)]
struct Counters {
    packets: u64,
    errors: u64,
}

impl Counters {
    fn read(iface: &str) -> Option<Self> {
        let networks = sysinfo::Networks::new_with_refreshed_list();
        let data = networks.list().get(iface)?;
        Some(Self {
            packets: data.total_packets_received(),
            errors: data.total_errors_on_received(),
        })
    }
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

/// How much latency a full download adds: under 30 ms is barely noticeable
/// in a game stream, over 100 ms is visible lag.
fn bufferbloat(added_ms: f64) -> &'static str {
    if added_ms < 30.0 {
        "low"
    } else if added_ms < 100.0 {
        "moderate"
    } else {
        "high"
    }
}

/// Receive errors are mostly packets damaged on the wire (CRC errors); some
/// drivers count each one twice. Game streaming copes with well under 0.1%:
/// GeForce NOW only warns from 1% loss.
fn errors_verdict(errors: u64, packets: u64) -> &'static str {
    if errors == 0 {
        "none"
    } else if percent(errors, packets) < 0.1 {
        "a few, harmless for gaming"
    } else {
        "too many; check the cable, its plugs and the router port"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarises_latency_samples() {
        let latency = Latency {
            samples: vec![Duration::from_millis(10), Duration::from_millis(20)],
            lost: 1,
        };
        assert_eq!(latency.average_ms(), Some(15.0));
        assert_eq!(latency.worst_ms(), Some(20.0));
        assert_eq!(
            latency.summary(),
            "15.0 ms average, 20.0 ms worst, 1 of 3 probes lost"
        );
        let silent = Latency {
            samples: vec![],
            lost: 4,
        };
        assert_eq!(silent.average_ms(), None);
        assert_eq!(silent.summary(), "no answers, 4 of 4 probes lost");
    }

    #[test]
    fn a_silent_server_does_not_outlast_the_deadline() {
        // Connections are accepted by the kernel, but nothing ever answers.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/10GB.bin", listener.local_addr().unwrap());
        let started = Instant::now();
        assert_eq!(
            download_until(&url, started + Duration::from_millis(500)),
            0
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn rates_downloads_over_the_window() {
        assert_eq!(
            megabits_per_second(25_000_000, Duration::from_secs(2)),
            100.0
        );
    }

    #[test]
    fn grades_bufferbloat_by_added_latency() {
        assert_eq!(bufferbloat(1.5), "low");
        assert_eq!(bufferbloat(32.0), "moderate");
        assert_eq!(bufferbloat(245.0), "high");
    }

    #[test]
    fn grades_receive_errors_by_rate() {
        assert_eq!(errors_verdict(0, 121_509), "none");
        assert_eq!(errors_verdict(118, 1_165_689), "a few, harmless for gaming");
        assert_eq!(
            errors_verdict(118, 26_071),
            "too many; check the cable, its plugs and the router port"
        );
        assert_eq!(percent(1, 0), 0.0);
    }
}
