//! Choosing display modes, the HiDPI file's sizes and the monitors' labels,
//! apart from CoreGraphics so it is tested on every platform.

// Only the macOS build calls these outside tests.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

/// What lum reads from a display mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Mode {
    /// The size things look like, in points.
    pub(crate) size: (u32, u32),
    /// The pixels drawn for it: twice the size each way for HiDPI.
    pub(crate) pixels: (u32, u32),
    pub(crate) hz: u32,
    /// IOKit flags the mode as drawing the panel's own pixels.
    pub(crate) native: bool,
    /// IOKit flags it as the mode macOS picks for the monitor by itself.
    pub(crate) default: bool,
}

impl Mode {
    pub(crate) fn hidpi(&self) -> bool {
        self.pixels.0 > self.size.0
    }
}

impl AsRef<Mode> for Mode {
    fn as_ref(&self) -> &Mode {
        self
    }
}

/// A Retina monitor is one macOS runs at HiDPI by itself, so its text is sharp
/// already and lum leaves it alone.
pub(crate) fn retina<M: AsRef<Mode>>(modes: &[M]) -> bool {
    modes
        .iter()
        .map(|mode| mode.as_ref())
        .any(|mode| mode.default && mode.hidpi())
}

/// The panel's own resolution: what its native modes draw, else its biggest
/// plain mode.
pub(crate) fn native_size<M: AsRef<Mode>>(modes: &[M]) -> Option<(u32, u32)> {
    let flagged = modes.iter().any(|mode| mode.as_ref().native);
    modes
        .iter()
        .map(|mode| mode.as_ref())
        .filter(|mode| if flagged { mode.native } else { !mode.hidpi() })
        .map(|mode| mode.pixels)
        .max_by_key(|&(width, height)| width * height)
}

/// `on`: the biggest HiDPI size no bigger than the panel, at the panel's shape
/// and the fastest refresh rate. That is the panel's own size where macOS
/// offers it at 2x, else the next size down. The half size macOS offers by
/// itself makes everything twice as big, so it doesn't count.
pub(crate) fn hidpi_mode<M: AsRef<Mode>>(modes: &[M], native: (u32, u32)) -> Option<&M> {
    modes
        .iter()
        .filter(|mode| {
            let mode = mode.as_ref();
            mode.hidpi()
                && mode.size.0 <= native.0
                && mode.size.0 > native.0 / 2
                && same_shape(mode.size, native)
        })
        .max_by_key(|mode| (mode.as_ref().size.0, mode.as_ref().hz))
}

/// `off`: the panel's own resolution without HiDPI, at the fastest refresh rate.
pub(crate) fn plain_mode<M: AsRef<Mode>>(modes: &[M], native: (u32, u32)) -> Option<&M> {
    modes
        .iter()
        .filter(|mode| !mode.as_ref().hidpi() && mode.as_ref().pixels == native)
        .max_by_key(|mode| mode.as_ref().hz)
}

/// How much bigger everything looks at a size than at the panel's own
/// resolution without HiDPI, such as "everything is 0.6% bigger than at
/// 2560x1440". None when it is the same to a tenth of a percent.
pub(crate) fn size_difference(
    (width, _): (u32, u32),
    (native_width, native_height): (u32, u32),
) -> Option<String> {
    let percent = (f64::from(native_width) / f64::from(width) - 1.0) * 100.0;
    let shown = format!("{:.1}", percent.abs());
    if shown == "0.0" {
        return None;
    }
    let way = if percent > 0.0 { "bigger" } else { "smaller" };
    Some(format!(
        "everything is {shown}% {way} than at {native_width}x{native_height}"
    ))
}

/// Whether a size has the panel's aspect ratio, give or take a rounded point.
fn same_shape((width, height): (u32, u32), (native_width, native_height): (u32, u32)) -> bool {
    (width * native_height).abs_diff(height * native_width) <= native_width
}

/// The sizes the HiDPI file offers: the panel's width down to half of it in
/// 16-point steps, each at the panel's aspect ratio. Crisp writes the same
/// list. macOS leaves out the ones the GPU can't drive.
pub(crate) fn ladder(native_width: u32, native_height: u32) -> Vec<(u32, u32)> {
    (native_width / 2..=native_width)
        .rev()
        .step_by(16)
        .map(|width| {
            let height = (width * native_height + native_width / 2) / native_width;
            (width, height)
        })
        .collect()
}

/// One `scale-resolutions` entry of the HiDPI file: the size drawn at 2x,
/// width then height, as big-endian 32-bit numbers.
pub(crate) fn scale_entry((width, height): (u32, u32)) -> Vec<u8> {
    [(width * 2).to_be_bytes(), (height * 2).to_be_bytes()].concat()
}

/// What starts each monitor's line: nothing with one monitor, else its name,
/// numbered when two share it, or its place in the list when it has none.
pub(crate) fn labels(names: &[Option<&str>]) -> Vec<String> {
    if names.len() < 2 {
        return vec![String::new(); names.len()];
    }
    let count = |names: &[Option<&str>], name: &str| {
        names.iter().filter(|other| **other == Some(name)).count()
    };
    names
        .iter()
        .enumerate()
        .map(|(index, name)| match name {
            None => format!("Monitor {}: ", index + 1),
            Some(name) if count(names, name) > 1 => {
                format!("{name} ({}): ", count(&names[..=index], name))
            }
            Some(name) => format!("{name}: "),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(width: u32, height: u32, hz: u32) -> Mode {
        Mode {
            size: (width, height),
            pixels: (width, height),
            hz,
            native: false,
            default: false,
        }
    }

    fn hidpi(width: u32, height: u32, hz: u32) -> Mode {
        Mode {
            pixels: (width * 2, height * 2),
            ..plain(width, height, hz)
        }
    }

    fn native(mode: Mode) -> Mode {
        Mode {
            native: true,
            ..mode
        }
    }

    fn by_default(mode: Mode) -> Mode {
        Mode {
            default: true,
            ..mode
        }
    }

    /// Modes an M1 offers a 2560x1440 monitor with the HiDPI file loaded.
    fn monitor_1440p() -> Vec<Mode> {
        vec![
            native(plain(2560, 1440, 60)),
            by_default(native(plain(2560, 1440, 144))),
            hidpi(2544, 1431, 60),
            hidpi(2544, 1431, 144),
            hidpi(2304, 1296, 144),
            hidpi(1920, 1080, 144),
            native(hidpi(1280, 720, 144)),
            plain(1920, 1080, 144),
            // Not the panel's shape, so not a candidate for `on`.
            hidpi(2560, 1600, 60),
        ]
    }

    /// Modes a 1920x1080 monitor offers with the HiDPI file loaded, where macOS
    /// leaves out its own size at 2x as it does for 2560x1440.
    fn monitor_1080p() -> Vec<Mode> {
        vec![
            native(plain(1920, 1080, 60)),
            native(plain(1920, 1080, 144)),
            hidpi(1904, 1071, 60),
            hidpi(1904, 1071, 144),
            hidpi(1600, 900, 144),
            native(hidpi(960, 540, 144)),
            plain(1280, 720, 144),
        ]
    }

    #[test]
    fn native_size_is_what_the_native_modes_draw() {
        assert_eq!(native_size(&monitor_1440p()), Some((2560, 1440)));
        assert_eq!(native_size(&monitor_1080p()), Some((1920, 1080)));
    }

    #[test]
    fn native_size_falls_back_to_the_biggest_plain_mode() {
        let modes = [
            plain(1280, 720, 60),
            plain(1920, 1080, 60),
            hidpi(1920, 1080, 60),
        ];
        assert_eq!(native_size(&modes), Some((1920, 1080)));
    }

    #[test]
    fn on_picks_the_hidpi_size_next_to_a_1440p_panels_own() {
        let modes = monitor_1440p();
        assert_eq!(
            hidpi_mode(&modes, (2560, 1440)),
            Some(&hidpi(2544, 1431, 144))
        );
    }

    #[test]
    fn on_picks_the_hidpi_size_next_to_a_1080p_panels_own() {
        let modes = monitor_1080p();
        assert_eq!(
            hidpi_mode(&modes, (1920, 1080)),
            Some(&hidpi(1904, 1071, 144))
        );
    }

    #[test]
    fn on_takes_the_panels_own_size_when_macos_offers_it_at_2x() {
        let mut modes = monitor_1080p();
        modes.push(hidpi(1920, 1080, 120));
        assert_eq!(
            hidpi_mode(&modes, (1920, 1080)),
            Some(&hidpi(1920, 1080, 120))
        );
    }

    #[test]
    fn on_finds_nothing_before_the_hidpi_file_is_loaded() {
        let modes = [
            by_default(native(plain(2560, 1440, 144))),
            native(hidpi(1280, 720, 144)),
        ];
        assert_eq!(hidpi_mode(&modes, (2560, 1440)), None);
    }

    #[test]
    fn off_picks_the_panels_own_resolution_at_the_fastest_rate() {
        let modes = monitor_1440p();
        assert_eq!(
            plain_mode(&modes, (2560, 1440)),
            Some(&by_default(native(plain(2560, 1440, 144))))
        );
        let modes = monitor_1080p();
        assert_eq!(
            plain_mode(&modes, (1920, 1080)),
            Some(&native(plain(1920, 1080, 144)))
        );
    }

    #[test]
    fn size_difference_is_how_much_bigger_everything_looks_than_without_hidpi() {
        let at_1440p = "everything is 0.6% bigger than at 2560x1440";
        assert_eq!(
            size_difference((2544, 1431), (2560, 1440)).as_deref(),
            Some(at_1440p)
        );
        let at_1080p = "everything is 0.8% bigger than at 1920x1080";
        assert_eq!(
            size_difference((1904, 1071), (1920, 1080)).as_deref(),
            Some(at_1080p)
        );
        let smaller = "everything is 14.9% smaller than at 2560x1440";
        assert_eq!(
            size_difference((3008, 1692), (2560, 1440)).as_deref(),
            Some(smaller)
        );
        assert_eq!(size_difference((2560, 1440), (2560, 1440)), None);
    }

    #[test]
    fn ladder_runs_from_the_panels_width_to_half_in_16_point_steps() {
        let sizes = ladder(2560, 1440);
        assert_eq!(sizes.len(), 81);
        assert_eq!(sizes[..2], [(2560, 1440), (2544, 1431)]);
        assert_eq!(sizes[80], (1280, 720));
        assert_eq!(ladder(1920, 1080)[..2], [(1920, 1080), (1904, 1071)]);
    }

    #[test]
    fn ladder_rounds_heights_the_aspect_ratio_does_not_divide() {
        // 3424 * 1440 / 3440 = 1433.3
        assert_eq!(ladder(3440, 1440)[1], (3424, 1433));
    }

    #[test]
    fn scale_entry_is_the_2x_size_as_big_endian_numbers() {
        assert_eq!(
            scale_entry((2544, 1431)),
            [0, 0, 0x13, 0xe0, 0, 0, 0x0b, 0x2e]
        );
    }

    /// Modes a 4K monitor offers, which macOS runs at HiDPI by itself.
    fn monitor_4k() -> Vec<Mode> {
        vec![
            native(plain(3840, 2160, 60)),
            by_default(native(hidpi(1920, 1080, 60))),
            hidpi(2560, 1440, 60),
            plain(1920, 1080, 60),
        ]
    }

    #[test]
    fn retina_is_a_monitor_macos_runs_at_hidpi_by_itself() {
        assert!(retina(&monitor_4k()));
        assert!(
            !retina(&monitor_1440p()),
            "its default mode is 1x, HiDPI file or not"
        );
        assert!(!retina(&monitor_1080p()));
    }

    #[test]
    fn one_monitor_gets_no_label() {
        assert_eq!(labels(&[Some("MP275Q E14")]), [""]);
    }

    #[test]
    fn several_monitors_go_by_name_numbered_when_two_share_one() {
        let names = [
            Some("DELL P2723D"),
            Some("MP275Q E14"),
            None,
            Some("DELL P2723D"),
        ];
        assert_eq!(
            labels(&names),
            [
                "DELL P2723D (1): ",
                "MP275Q E14: ",
                "Monitor 3: ",
                "DELL P2723D (2): "
            ]
        );
    }
}
