//! The tray icon, drawn in code so there is no image asset to ship or decode.
//!
//! One little radio, built from a few signed-distance shapes on a 16-unit grid (one unit is
//! one pixel at the common 16 px tray size, so the main edges sit on whole units and stay
//! crisp there). Its antenna is up while playing and folded away while paused, so the state
//! shows in the silhouette and not only in the colour.
//!
//! Tray hosts paint the icon over a panel we cannot see, light or dark, so nothing here
//! depends on the panel:
//!
//! - The colour ink uses mid-tone fills, with enough contrast against both dark and light
//!   panels (see the tests), and the speaker and knobs are holes that take the panel's own
//!   colour. A faint white rim around the outside keeps the icon visible on mid-tone panels
//!   and vanishes on light ones.
//! - macOS draws only the alpha channel of a template image and tints it for the menu bar,
//!   so the template ink is plain black and the system does the rest.

pub(super) const SIZE: u32 = 64;
/// Subsamples per pixel axis; edges are anti-aliased by averaging them.
const SUPERSAMPLE: u32 = 8;
const GRID: f32 = 16.0;
/// How far the folded radio is lifted, in grid units. Playing, the antenna balances the body
/// about the grid centre. Folded, the body alone sits 2.5 units low, so it is lifted to match.
const FOLDED_LIFT: f32 = 2.5;

/// Fill colours, picked for contrast against both dark and light panels.
const PLAYING: [u8; 3] = [0x1a, 0x8f, 0x4d];
const PAUSED: [u8; 3] = [0x78, 0x7f, 0x8b];
/// Width in grid units, and opacity, of the white rim around the colour ink's outline.
const RIM_WIDTH: f32 = 0.6;
const RIM_ALPHA: f32 = 0.35;
/// A paused template is drawn fainter, like a disabled menu bar item.
const PAUSED_TEMPLATE_ALPHA: f32 = 0.6;

/// How the icon is inked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ink {
    /// Real colours, for hosts that paint the pixels as they are.
    Colour,
    /// Black with alpha, for macOS to tint for the menu bar.
    Template,
}

type Point = (f32, f32);

fn circle((x, y): Point, (cx, cy): Point, radius: f32) -> f32 {
    (x - cx).hypot(y - cy) - radius
}

/// Axis-aligned rounded rectangle from `min` to `max`.
fn rounded_box((x, y): Point, (x0, y0): Point, (x1, y1): Point, radius: f32) -> f32 {
    let (hx, hy) = ((x1 - x0) / 2.0 - radius, (y1 - y0) / 2.0 - radius);
    let (dx, dy) = (
        (x - (x0 + x1) / 2.0).abs() - hx,
        (y - (y0 + y1) / 2.0).abs() - hy,
    );
    dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius
}

/// A line segment with round ends, `half_width` either side of it.
fn capsule((x, y): Point, (ax, ay): Point, (bx, by): Point, half_width: f32) -> f32 {
    let (px, py, dx, dy) = (x - ax, y - ay, bx - ax, by - ay);
    let t = ((px * dx + py * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (px - dx * t).hypot(py - dy * t) - half_width
}

/// Signed distances at `p`, negative inside: `(radio, silhouette)`. The silhouette is the
/// radio with its holes filled in, which is what the rim follows.
fn radio(p: Point, antenna: bool) -> (f32, f32) {
    // Sampling lower moves the shape up, so the folded radio is drawn FOLDED_LIFT higher.
    let p = if antenna { p } else { (p.0, p.1 + FOLDED_LIFT) };
    let body = rounded_box(p, (1.0, 6.0), (15.0, 15.0), 2.0);
    let silhouette = if antenna {
        body.min(capsule(p, (10.5, 6.5), (5.5, 2.2), 1.0))
    } else {
        body
    };
    let speaker = circle(p, (6.0, 10.5), 3.0);
    let dial = rounded_box(p, (10.0, 8.0), (14.0, 9.0), 0.5);
    let knob = circle(p, (12.0, 12.0), 1.5);
    let holes = speaker.min(dial).min(knob);
    (silhouette.max(-holes), silhouette)
}

/// RGBA pixels (straight alpha) of the radio in the given state, `SIZE` x `SIZE`.
pub(super) fn rgba(paused: bool, ink: Ink) -> Vec<u8> {
    let samples = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    let unit = GRID / SIZE as f32;
    let (fill, strength) = match (ink, paused) {
        (Ink::Colour, false) => (PLAYING, 1.0),
        (Ink::Colour, true) => (PAUSED, 1.0),
        (Ink::Template, false) => ([0; 3], 1.0),
        (Ink::Template, true) => ([0; 3], PAUSED_TEMPLATE_ALPHA),
    };
    let rim = if ink == Ink::Colour { RIM_ALPHA } else { 0.0 };

    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (mut inside, mut on_rim) = (0.0_f32, 0.0_f32);
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let at = |pixel: u32, sub: u32| {
                        (pixel as f32 + (sub as f32 + 0.5) / SUPERSAMPLE as f32) * unit
                    };
                    let (distance, silhouette) = radio((at(x, sx), at(y, sy)), !paused);
                    if distance <= 0.0 {
                        inside += 1.0;
                    } else if silhouette > 0.0 && silhouette <= RIM_WIDTH {
                        on_rim += 1.0;
                    }
                }
            }
            // The fill and the white rim never share a sample, so mixing their coverage
            // weighted by opacity gives the straight-alpha colour of the pixel.
            let (fill_alpha, rim_alpha) = (inside / samples * strength, on_rim / samples * rim);
            let alpha = fill_alpha + rim_alpha;
            for channel in fill {
                let mixed = if alpha > 0.0 {
                    (f32::from(channel) * fill_alpha + 255.0 * rim_alpha) / alpha
                } else {
                    0.0
                };
                rgba.push(mixed.round() as u8);
            }
            rgba.push((alpha * 255.0).round() as u8);
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATES: [bool; 2] = [false, true];
    const INKS: [Ink; 2] = [Ink::Colour, Ink::Template];

    /// The straight-alpha pixel covering grid point `(x, y)`, in the 16-unit grid the radio is
    /// drawn on: `(6.0, 10.5)` is the speaker's centre.
    fn pixel(rgba: &[u8], (x, y): (f32, f32)) -> [u8; 4] {
        let unit = GRID / SIZE as f32;
        let i = (((y / unit) as u32 * SIZE + (x / unit) as u32) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    fn alphas(rgba: &[u8]) -> Vec<u8> {
        rgba.as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect()
    }

    /// WCAG relative luminance.
    fn luminance([r, g, b]: [u8; 3]) -> f32 {
        let linear = |channel: u8| {
            let c = f32::from(channel) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn every_state_and_ink_is_a_full_square_rgba_buffer() {
        for paused in STATES {
            for ink in INKS {
                assert_eq!(rgba(paused, ink).len(), (SIZE * SIZE * 4) as usize);
            }
        }
    }

    #[test]
    fn the_icon_has_no_background_plate() {
        for paused in STATES {
            for ink in INKS {
                let icon = rgba(paused, ink);
                for corner in [(0.0, 0.0), (15.9, 0.0), (0.0, 15.9), (15.9, 15.9)] {
                    assert_eq!(pixel(&icon, corner)[3], 0, "{ink:?} paused={paused}");
                }
            }
        }
    }

    #[test]
    fn the_antenna_is_up_only_while_playing() {
        // The antenna's tip: above the lifted body, so empty once it is folded away.
        let on_the_antenna = (5.5, 2.2);
        for ink in INKS {
            assert_eq!(pixel(&rgba(false, ink), on_the_antenna)[3], 255);
            assert_eq!(pixel(&rgba(true, ink), on_the_antenna)[3], 0);
        }
    }

    #[test]
    fn state_shows_in_the_silhouette_and_not_only_in_the_colour() {
        // Colour alone is a poor cue (colour-blind users, and a template has no colour).
        for ink in INKS {
            assert_ne!(
                alphas(&rgba(false, ink)),
                alphas(&rgba(true, ink)),
                "{ink:?}"
            );
        }
    }

    #[test]
    fn the_speaker_is_a_hole_that_takes_the_panels_colour() {
        // The speaker's centre, lifted with the folded radio.
        for (paused, centre) in [(false, (6.0, 10.5)), (true, (6.0, 10.5 - FOLDED_LIFT))] {
            for ink in INKS {
                assert_eq!(
                    pixel(&rgba(paused, ink), centre)[3],
                    0,
                    "{ink:?} paused={paused}"
                );
            }
        }
    }

    #[test]
    fn the_stopped_silhouette_is_vertically_centred() {
        let icon = rgba(true, Ink::Template);
        let alpha = |x: u32, y: u32| icon[((y * SIZE + x) * 4 + 3) as usize];
        let inked: Vec<u32> = (0..SIZE)
            .filter(|&y| (0..SIZE).any(|x| alpha(x, y) >= 128))
            .collect();
        let (top, bottom) = (inked[0], inked[inked.len() - 1]);
        let unit = GRID / SIZE as f32;
        let centre = (top + bottom + 1) as f32 / 2.0 * unit;
        assert!(
            (centre - GRID / 2.0).abs() <= 0.05,
            "stopped icon is centred at {centre}, not {}",
            GRID / 2.0
        );
    }

    #[test]
    fn the_body_is_solid_in_the_palette_colour() {
        let solid_body = (2.0, 10.5);
        assert_eq!(
            pixel(&rgba(false, Ink::Colour), solid_body),
            [PLAYING[0], PLAYING[1], PLAYING[2], 255]
        );
        assert_eq!(
            pixel(&rgba(true, Ink::Colour), solid_body),
            [PAUSED[0], PAUSED[1], PAUSED[2], 255]
        );
    }

    #[test]
    fn the_colour_rim_is_white_and_stays_outside_the_radio() {
        // Just left of the body's edge, which sits at x = 1.0.
        let beside_the_body = (0.8, 10.5);
        let [r, g, b, a] = pixel(&rgba(false, Ink::Colour), beside_the_body);
        assert_eq!([r, g, b], [255, 255, 255]);
        assert_eq!(a, (RIM_ALPHA * 255.0).round() as u8);
        assert_eq!(pixel(&rgba(false, Ink::Template), beside_the_body)[3], 0);
    }

    #[test]
    fn template_ink_is_black_and_a_paused_template_is_fainter() {
        for paused in STATES {
            let icon = rgba(paused, Ink::Template);
            for p in icon.as_chunks::<4>().0.iter().filter(|p| p[3] > 0) {
                assert_eq!(
                    p[..3],
                    [0, 0, 0],
                    "macOS tints the alpha shape; the colour is ignored"
                );
            }
        }
        let strongest = |paused| *alphas(&rgba(paused, Ink::Template)).iter().max().unwrap();
        assert_eq!(strongest(false), 255);
        assert_eq!(
            strongest(true),
            (PAUSED_TEMPLATE_ALPHA * 255.0).round() as u8
        );
    }

    /// The requirement behind the palette: the icon must read on dark and on light panels.
    /// 3:1 is WCAG's floor for graphical objects.
    #[test]
    fn colour_fills_read_on_dark_and_light_panels() {
        let panels = [
            ("black (GNOME)", [0x00, 0x00, 0x00]),
            ("Windows dark", [0x20, 0x20, 0x20]),
            ("Breeze dark", [0x23, 0x26, 0x29]),
            ("Catppuccin Mocha", [0x1e, 0x1e, 0x2e]),
            ("Windows light", [0xf3, 0xf3, 0xf3]),
            ("Breeze light", [0xef, 0xf0, 0xf1]),
            ("white", [0xff, 0xff, 0xff]),
        ];
        for (state, fill) in [("playing", PLAYING), ("paused", PAUSED)] {
            for (panel, colour) in panels {
                let ratio = contrast(fill, colour);
                assert!(ratio >= 3.0, "{state} fill is {ratio:.1}:1 on {panel}");
            }
        }
    }
}
