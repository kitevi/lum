//! `lum net test`: latency on the wired connection idle and under load, and
//! its receive errors. See `README.md`.

mod iface;
mod probe;

use anyhow::{Result, ensure};

use crate::cli::NetCommand;

pub fn run(command: NetCommand) -> Result<()> {
    ensure!(
        cfg!(any(target_os = "linux", target_os = "macos")),
        "lum net is only supported on Linux and macOS"
    );
    match command {
        NetCommand::Test { iface, seconds } => probe::run(iface.as_deref(), seconds),
    }
}
