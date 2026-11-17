//! The macOS side of `lum display`. CoreGraphics lists and switches display
//! modes, and an override file under `/Library/Displays` gives a monitor the
//! HiDPI sizes macOS leaves out. See `README.md`.

use std::path::Path;
use std::process::Command;
use std::ptr;

use anyhow::{Context, Result, ensure};
use objc2_core_foundation::{CFArray, CFBoolean, CFDictionary, CFRetained};
use objc2_core_graphics::{
    CGBeginDisplayConfiguration, CGCancelDisplayConfiguration, CGCompleteDisplayConfiguration,
    CGConfigureDisplayWithDisplayMode, CGConfigureOption, CGDirectDisplayID,
    CGDisplayCopyAllDisplayModes, CGDisplayCopyDisplayMode, CGDisplayIsBuiltin, CGDisplayMode,
    CGDisplayModelNumber, CGDisplayVendorNumber, CGError, CGGetOnlineDisplayList,
    kCGDisplayShowDuplicateLowResolutionModes,
};
use plist::{Dictionary, Value};

use super::modes::{
    Mode, hidpi_mode, ladder, native_size, plain_mode, scale_entry, size_difference,
};
use crate::cli::HidpiState;

/// Where macOS reads the display overrides an admin adds.
const OVERRIDES: &str = "/Library/Displays/Contents/Resources/Overrides";
/// Where macOS keeps the overrides it ships.
const SYSTEM_OVERRIDES: &str = "/System/Library/Displays/Contents/Resources/Overrides";
const SCALE_RESOLUTIONS: &str = "scale-resolutions";
/// IOKit's `kDisplayModeNativeFlag`: the mode draws the panel's own pixels.
const NATIVE_FLAG: u32 = 0x0200_0000;
const RELOAD: &str =
    "Restart the Mac, or unplug the monitor and plug it back in, then run `lum display on` again.";

pub(super) fn run(state: Option<HidpiState>) -> Result<()> {
    let monitors = Monitor::external()?;
    ensure!(
        !monitors.is_empty(),
        "no external monitor is connected; built-in screens are HiDPI already"
    );
    for (index, monitor) in monitors.iter().enumerate() {
        let label = match monitors.len() {
            1 => String::new(),
            _ => format!("Monitor {}: ", index + 1),
        };
        let line = match state {
            None => monitor.describe(),
            Some(HidpiState::On) => monitor.turn_on()?,
            Some(HidpiState::Off) => monitor.turn_off()?,
        };
        println!("{label}{line}");
    }
    Ok(())
}

struct Monitor {
    id: CGDirectDisplayID,
    modes: Vec<CgMode>,
    /// The panel's own resolution.
    native: (u32, u32),
}

impl Monitor {
    /// Every external monitor; built-in screens are HiDPI already.
    fn external() -> Result<Vec<Self>> {
        let mut ids = [0; 16];
        let mut count = 0;
        // SAFETY: `ids` has room for as many IDs as we ask for, and `count` is a u32.
        let error =
            unsafe { CGGetOnlineDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
        check(error, "list the displays")?;
        ids.into_iter()
            .take(count as usize)
            .filter(|&id| !CGDisplayIsBuiltin(id))
            .map(Self::new)
            .collect()
    }

    fn new(id: CGDirectDisplayID) -> Result<Self> {
        let modes = CgMode::all(id);
        let native =
            native_size(&modes).context("macOS did not report the monitor's resolution")?;
        Ok(Self { id, modes, native })
    }

    /// Switches to the HiDPI mode for good, so it survives restarts.
    fn turn_on(&self) -> Result<String> {
        match hidpi_mode(&self.modes, self.native) {
            Some(mode) => self.switch(mode, CGConfigureOption::Permanently),
            None => self.install(),
        }
    }

    /// Switches to the plain mode until the user logs out or restarts, when
    /// macOS goes back to the mode saved for good.
    fn turn_off(&self) -> Result<String> {
        let mode = plain_mode(&self.modes, self.native)
            .context("macOS does not offer the monitor's own resolution")?;
        let line = self.switch(mode, CGConfigureOption::ForSession)?;
        Ok(format!("{line} It stays off until you log out or restart."))
    }

    /// Switches to `mode` for as long as `lasts` says, unless the monitor is
    /// in it already.
    fn switch(&self, mode: &CgMode, lasts: CGConfigureOption) -> Result<String> {
        if CgMode::current(self.id).is_none_or(|now| now.mode != mode.mode) {
            apply(self.id, &mode.raw, lasts)?;
            let now = CgMode::current(self.id).context("macOS did not report the new mode")?;
            ensure!(
                (now.mode.size, now.mode.pixels) == (mode.mode.size, mode.mode.pixels),
                "macOS did not switch to {}x{}",
                mode.mode.size.0,
                mode.mode.size.1
            );
        }
        Ok(self.describe())
    }

    fn describe(&self) -> String {
        let Some(now) = CgMode::current(self.id) else {
            return "macOS did not report the current mode.".to_owned();
        };
        let Mode {
            size: (width, height),
            hz,
            ..
        } = now.mode;
        let rate = if hz > 0 {
            format!(" at {hz} Hz")
        } else {
            String::new()
        };
        let difference = size_difference(now.mode.size, self.native)
            .map(|difference| format!("; {difference}"))
            .unwrap_or_default();
        if now.mode.hidpi() {
            format!("HiDPI on: looks like {width}x{height}{rate}{difference}.")
        } else {
            format!("HiDPI off: {width}x{height}{rate}{difference}.")
        }
    }

    /// Installs the file that gives the monitor HiDPI sizes near its own
    /// resolution, which macOS reads when the monitor connects.
    fn install(&self) -> Result<String> {
        let (width, height) = self.native;
        let (vendor, product) = (
            CGDisplayVendorNumber(self.id),
            CGDisplayModelNumber(self.id),
        );
        let file = format!("DisplayVendorID-{vendor:x}/DisplayProductID-{product:x}");
        ensure!(
            !Path::new(SYSTEM_OVERRIDES).join(&file).exists(),
            "macOS ships its own settings for this monitor, with no HiDPI size close to {width}x{height}"
        );
        let path = Path::new(OVERRIDES).join(&file);
        let mut settings = read_override(&path)?.unwrap_or_default();
        if settings.contains_key(SCALE_RESOLUTIONS) {
            return Ok(format!(
                "HiDPI sizes are installed, but macOS has not loaded them yet. {RELOAD}"
            ));
        }
        settings.insert("DisplayVendorID".to_owned(), vendor.into());
        settings.insert("DisplayProductID".to_owned(), product.into());
        let sizes = ladder(width, height)
            .into_iter()
            .map(|size| Value::Data(scale_entry(size)))
            .collect();
        settings.insert(SCALE_RESOLUTIONS.to_owned(), Value::Array(sizes));
        write_override(&path, settings)?;
        Ok(format!(
            "Installed HiDPI sizes for this {width}x{height} monitor. {RELOAD}"
        ))
    }
}

/// A CoreGraphics display mode, and what lum reads from it.
struct CgMode {
    mode: Mode,
    raw: CFRetained<CGDisplayMode>,
}

impl AsRef<Mode> for CgMode {
    fn as_ref(&self) -> &Mode {
        &self.mode
    }
}

impl CgMode {
    fn new(raw: CFRetained<CGDisplayMode>) -> Self {
        let mode = Some(&*raw);
        let read = Mode {
            size: (
                CGDisplayMode::width(mode) as u32,
                CGDisplayMode::height(mode) as u32,
            ),
            pixels: (
                CGDisplayMode::pixel_width(mode) as u32,
                CGDisplayMode::pixel_height(mode) as u32,
            ),
            hz: CGDisplayMode::refresh_rate(mode).round() as u32,
            native: (CGDisplayMode::io_flags(mode) & NATIVE_FLAG) != 0,
        };
        Self { mode: read, raw }
    }

    /// Every mode the desktop can use, HiDPI ones included.
    fn all(display: CGDirectDisplayID) -> Vec<Self> {
        // SAFETY: reads an immutable CoreGraphics constant.
        let key = unsafe { kCGDisplayShowDuplicateLowResolutionModes };
        // HiDPI modes that share a size with a plain mode only show up with this option.
        let options = CFDictionary::from_slices(&[key], &[CFBoolean::new(true)]);
        // SAFETY: the option maps a CFString to a CFBoolean, as CoreGraphics expects.
        let Some(modes) =
            (unsafe { CGDisplayCopyAllDisplayModes(display, Some(options.as_opaque())) })
        else {
            return Vec::new();
        };
        // SAFETY: CoreGraphics documents the array as holding display modes.
        let modes = unsafe { CFRetained::cast_unchecked::<CFArray<CGDisplayMode>>(modes) };
        modes
            .iter()
            .filter(|mode| CGDisplayMode::is_usable_for_desktop_gui(Some(mode)))
            .map(Self::new)
            .collect()
    }

    fn current(display: CGDirectDisplayID) -> Option<Self> {
        CGDisplayCopyDisplayMode(display).map(Self::new)
    }
}

/// Switches a display to a mode for as long as `lasts` says.
fn apply(display: CGDirectDisplayID, mode: &CGDisplayMode, lasts: CGConfigureOption) -> Result<()> {
    let mut config = ptr::null_mut();
    // SAFETY: `config` is a valid place for the new configuration.
    check(
        unsafe { CGBeginDisplayConfiguration(&mut config) },
        "start a display change",
    )?;
    // SAFETY: `config` was just begun, and is completed or cancelled once.
    unsafe {
        let error = CGConfigureDisplayWithDisplayMode(config, display, Some(mode), None);
        if error != CGError::Success {
            CGCancelDisplayConfiguration(config);
            return check(error, "pick that mode");
        }
        check(
            CGCompleteDisplayConfiguration(config, lasts),
            "switch the display",
        )
    }
}

fn check(error: CGError, action: &str) -> Result<()> {
    ensure!(
        error == CGError::Success,
        "macOS could not {action} (CoreGraphics error {})",
        error.0
    );
    Ok(())
}

fn read_override(path: &Path) -> Result<Option<Dictionary>> {
    if !path
        .try_exists()
        .with_context(|| format!("could not check {}", path.display()))?
    {
        return Ok(None);
    }
    Value::from_file(path)
        .with_context(|| format!("could not read {}", path.display()))?
        .into_dictionary()
        .map(Some)
        .with_context(|| format!("{} is not a display override file", path.display()))
}

/// Override files belong to root, so this stages the file and lets
/// `sudo install` put it in place.
fn write_override(path: &Path, settings: Dictionary) -> Result<()> {
    let staged = tempfile::NamedTempFile::new().context("could not create a temporary file")?;
    Value::Dictionary(settings)
        .to_file_xml(staged.path())
        .context("could not write the HiDPI file")?;
    let folder = path
        .parent()
        .context("override files sit in a vendor folder")?;
    println!("Installing HiDPI sizes needs admin rights, so sudo may ask for your password.");
    sudo("install -d -m 755 -o root -g wheel", &[folder])?;
    sudo("install -m 644 -o root -g wheel", &[staged.path(), path])
}

fn sudo(command: &str, paths: &[&Path]) -> Result<()> {
    let status = Command::new("/usr/bin/sudo")
        .args(command.split(' '))
        .args(paths)
        .status()
        .context("could not run sudo")?;
    ensure!(status.success(), "`sudo {command}` failed");
    Ok(())
}
