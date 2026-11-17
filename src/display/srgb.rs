//! `lum display srgb`: the sRGB color profile for monitors that aren't Retina,
//! unless one was picked for them. ColorSync keeps each display's profiles.
//! See `README.md`.

use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use anyhow::{Context, Result, bail, ensure};
use inquire::{Confirm, InquireError};
use objc2_color_sync::{
    ColorSyncDeviceCopyDeviceInfo, ColorSyncDeviceSetCustomProfiles, kColorSyncCustomProfiles,
    kColorSyncDeviceDefaultProfileID, kColorSyncDeviceProfileURL, kColorSyncDisplayDeviceClass,
    kColorSyncFactoryProfiles,
};
use objc2_core_foundation::{CFDictionary, CFNull, CFRetained, CFString, CFType, CFURL, CFUUID};
use objc2_core_graphics::CGDirectDisplayID;

use super::macos::Monitor;
use super::modes::{SRGB, SrgbStep, srgb_step};

// objc2-color-sync binds this too, but its binding panics when macOS returns
// NULL, as for a monitor unplugged a moment ago, and release builds abort on a
// panic.
unsafe extern "C-unwind" {
    fn CGDisplayCreateUUIDFromDisplayID(display: CGDirectDisplayID) -> Option<NonNull<CFUUID>>;
}

pub(super) fn run() -> Result<()> {
    // Listing the monitors also connects to the window server, without which
    // ColorSync asserts.
    let monitors = Monitor::managed()?;
    ensure!(
        !monitors.is_empty(),
        "lum display srgb is disabled: every connected screen is Retina, and lum leaves their color alone"
    );
    ensure!(
        Path::new(SRGB).exists(),
        "macOS has no sRGB profile at {SRGB}"
    );
    let mut failed = 0;
    for (monitor, label) in monitors.iter().zip(Monitor::labels(&monitors)) {
        match give_srgb(monitor.id, &label) {
            Ok(line) => println!("{label}{line}"),
            // Ctrl-C at a question stops the whole command.
            Err(error) if monitors.len() == 1 || interrupted(&error) => return Err(error),
            // One monitor failing doesn't stop the others.
            Err(error) => {
                eprintln!("{label}{error:#}");
                failed += 1;
            }
        }
    }
    ensure!(
        failed == 0,
        "{failed} of {} monitors did not get sRGB",
        monitors.len()
    );
    Ok(())
}

/// Gives a display sRGB once the user says yes, unless it has it already or has
/// a profile picked for it. `label` starts the question, as it starts the line.
fn give_srgb(display: CGDirectDisplayID, label: &str) -> Result<String> {
    // SAFETY: takes any display ID, and returns NULL for one it doesn't know.
    let uuid = unsafe { CGDisplayCreateUUIDFromDisplayID(display) }
        .context("macOS did not report the monitor's ColorSync ID")?;
    // SAFETY: the UUID was created for us, so releasing it is ours to do.
    let uuid = unsafe { CFRetained::from_raw(uuid) };
    let profiles = Profiles::read(&uuid)?;
    match srgb_step(profiles.custom.as_deref(), profiles.factory.as_deref()) {
        SrgbStep::AlreadySet => Ok("sRGB profile already set.".to_owned()),
        SrgbStep::Keep(picked) => {
            let name = picked.file_name().unwrap_or(picked.as_os_str());
            Ok(format!(
                "Keeps the profile picked for it, {}.",
                name.display()
            ))
        }
        SrgbStep::Set => {
            if !ask(label)? {
                return Ok("Keeps the profile macOS made for it.".to_owned());
            }
            profiles.set_srgb(&uuid)?;
            let now = Profiles::read(&uuid)?;
            ensure!(
                now.custom.as_deref() == Some(Path::new(SRGB)),
                "ColorSync did not keep the sRGB profile"
            );
            Ok("sRGB profile set.".to_owned())
        }
    }
}

/// Whether to give the monitor sRGB: no unless the user says yes.
fn ask(label: &str) -> Result<bool> {
    let question =
        format!("{label}Give it the sRGB profile, in place of the one macOS made for it?");
    match Confirm::new(&question).with_default(false).prompt() {
        Ok(yes) => Ok(yes),
        // Esc answers no.
        Err(InquireError::OperationCanceled) => Ok(false),
        Err(InquireError::NotTTY) => bail!(
            "lum display srgb asks before it changes a profile, and needs a terminal to ask in"
        ),
        Err(error) => Err(error.into()),
    }
}

/// Whether the user pressed Ctrl-C at a question.
fn interrupted(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<InquireError>(),
        Some(InquireError::OperationInterrupted)
    )
}

/// A display's default profile, as ColorSync keeps it.
struct Profiles {
    /// What ColorSync calls the profile, such as "1".
    id: CFRetained<CFString>,
    /// The file macOS made for the display.
    factory: Option<PathBuf>,
    /// The file picked in its place, if any.
    custom: Option<PathBuf>,
}

impl Profiles {
    fn read(display: &CFUUID) -> Result<Self> {
        // SAFETY: reads immutable ColorSync constants.
        let (class, factory_key, default_key, url_key, custom_key) = unsafe {
            (
                kColorSyncDisplayDeviceClass,
                kColorSyncFactoryProfiles,
                kColorSyncDeviceDefaultProfileID,
                kColorSyncDeviceProfileURL,
                kColorSyncCustomProfiles,
            )
        };
        // SAFETY: the class is ColorSync's own, and the UUID a display's.
        let info = unsafe { ColorSyncDeviceCopyDeviceInfo(class, display) }
            .context("ColorSync has no profiles for the monitor")?;
        let factory =
            get(&info, factory_key).context("ColorSync has no profiles for the monitor")?;
        // ColorSync's profile IDs are strings, such as "1".
        let id = get(&factory, default_key)
            .and_then(|id| id.downcast::<CFString>().ok())
            .context("ColorSync names no default profile for the monitor")?;
        let factory = get(&factory, &id)
            .and_then(|entry| get(&entry, url_key))
            .and_then(|url| file(&url));
        let custom = get(&info, custom_key)
            .and_then(|custom| get(&custom, &id))
            // Null puts back the profile macOS made.
            .filter(|entry| entry.downcast_ref::<CFNull>().is_none())
            .map(|entry| {
                file(&entry).context("ColorSync's custom profile for the monitor is not a file")
            })
            .transpose()?;
        Ok(Self {
            id,
            factory,
            custom,
        })
    }

    /// Makes sRGB the display's custom default profile, for the current user
    /// on this Mac.
    fn set_srgb(&self, display: &CFUUID) -> Result<()> {
        let url =
            CFURL::from_file_path(SRGB).context("could not make a URL of the sRGB profile")?;
        let profiles = CFDictionary::<CFString, CFURL>::from_slices(&[&*self.id], &[&*url]);
        // SAFETY: reads an immutable ColorSync constant.
        let class = unsafe { kColorSyncDisplayDeviceClass };
        // SAFETY: maps one of the display's profile IDs to a profile's URL, as
        // ColorSync expects.
        let set = unsafe { ColorSyncDeviceSetCustomProfiles(class, display, profiles.as_opaque()) };
        ensure!(set, "ColorSync did not take the sRGB profile");
        Ok(())
    }
}

/// The entry under `key`, when `dict` is a dictionary that has one.
fn get(dict: &CFType, key: &CFType) -> Option<CFRetained<CFType>> {
    let dict = dict.downcast_ref::<CFDictionary>()?;
    // SAFETY: CoreFoundation dictionaries hold CoreFoundation objects.
    unsafe { dict.cast_unchecked::<CFType, CFType>() }.get(key)
}

/// The file a profile URL points at.
fn file(url: &CFType) -> Option<PathBuf> {
    url.downcast_ref::<CFURL>()?.to_file_path()
}
