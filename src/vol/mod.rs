use anyhow::Result;

use crate::cli::VolCommand;

/// Returns the OS-specific default reset volume (0–100).
pub(crate) const fn default_volume() -> u8 {
    #[cfg(target_os = "macos")]
    {
        17
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        25
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        25
    }
}

pub fn run(command: VolCommand) -> Result<()> {
    match command {
        VolCommand::Status => {
            let device = volumecontrol::AudioDevice::from_default()?;
            println!("Volume: {}", device.get_vol()?);
            Ok(())
        }
        VolCommand::Set { level } => set(u8::try_from(level.0)?, false),
        VolCommand::Reset => set(default_volume(), true),
    }
}

fn set(target: u8, is_default: bool) -> Result<()> {
    let device = volumecontrol::AudioDevice::from_default()?;
    let previous = device.get_vol().ok();

    device.set_vol(target)?;

    match (is_default, previous) {
        (true, Some(prev)) => println!("Volume set to {} (default, was {})", target, prev),
        (true, None) => println!("Volume set to {} (default)", target),
        (false, Some(prev)) => println!("Volume set to {} (was {})", target, prev),
        (false, None) => println!("Volume set to {}", target),
    }

    Ok(())
}
