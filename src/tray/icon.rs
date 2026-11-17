//! The tray icon, drawn in code so there is no image asset to ship or decode.
//!
//! A little radio while a station is loaded, and a monitor while `lum display` has HiDPI
//! off. With both, a smaller radio sits over the monitor's bottom-right corner, cut out of
//! it with a gap the way SF Symbols draws its badges, so it stays one square icon. While
//! `lum net game` is on, the monitor's screen is lit: filled in blue, where it is a hole
//! otherwise, with the radio over it or without. Filled against hollow is a difference that
//! survives 16 px, which a picture inside the screen would not. Each shape is built from a
//! few signed-distance shapes on a 16-unit grid (one unit is one pixel at the common 16 px
//! tray size, so the main edges sit on whole units and stay crisp there). The radio's
//! antenna is up while playing and folded away while paused, so the state shows in the
//! silhouette and not only in the colour.
//!
//! Tray hosts paint the icon over a panel we cannot see, light or dark, so nothing here
//! depends on the panel:
//!
//! - The colour ink uses mid-tone fills, with enough contrast against both dark and light
//!   panels (see the tests), and the speaker, the knobs and an unlit screen are holes that
//!   take the panel's own colour. A faint white rim around the outside keeps the icon visible on mid-tone panels
//!   and vanishes on light ones.
//! - macOS draws only the alpha channel of a template image and tints it for the menu bar,
//!   so the template ink is plain black and the system does the rest. A template has no
//!   colour, so while the radio plays, or the screen is lit, macOS gets the menu bar ink
//!   instead: what the system would tint the template to, with the radio green and the lit
//!   screen blue.

/// The icon's width and height in pixels.
pub(super) const SIZE: u32 = 64;
/// Subsamples per pixel axis; edges are anti-aliased by averaging them.
const SUPERSAMPLE: u32 = 8;
const GRID: f32 = 16.0;
/// The radio over the monitor is drawn at this scale, with the bottom-right corner of its
/// body at `BADGE_CORNER` on both axes, so it covers the monitor's corner.
const BADGE_SCALE: f32 = 0.625;
const BADGE_CORNER: f32 = 15.25;
/// Grid units of the monitor cut away around the radio over it.
const BADGE_GAP: f32 = 1.0;
/// How far the folded radio is lifted, in grid units. Playing, the antenna balances the body
/// about the grid centre. Folded, the body alone sits 2.5 units low, so it is lifted to match.
const FOLDED_LIFT: f32 = 2.5;

/// Fill colours, picked for contrast against both dark and light panels. The monitor takes
/// the paused grey, so the green of a playing radio and the blue of a lit screen are the only
/// colours in the icon.
const PLAYING: [u8; 3] = [0x1a, 0x8f, 0x4d];
const PAUSED: [u8; 3] = [0x78, 0x7f, 0x8b];
/// The lit screen of game mode. Blue, so that it never reads as the green of a playing radio.
const GAME: [u8; 3] = [0x3b, 0x7d, 0xdd];
/// Apple's system green in its accessible variants, for a light and a dark menu bar.
const MENU_BAR_GREEN: [u8; 3] = [0x24, 0x8a, 0x3d];
const MENU_BAR_GREEN_DARK: [u8; 3] = [0x30, 0xd1, 0x58];
/// Apple's system blue, for a light and a dark menu bar.
const MENU_BAR_BLUE: [u8; 3] = [0x00, 0x7a, 0xff];
const MENU_BAR_BLUE_DARK: [u8; 3] = [0x0a, 0x84, 0xff];
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
    /// What macOS would tint a template to on a light or a dark menu bar, black or white,
    /// with a playing radio in Apple's green and a lit screen in Apple's blue. A template
    /// can't hold colour, so macOS gets this while the radio plays or the screen is lit.
    MenuBar { dark: bool },
}

/// What the icon shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Look {
    /// `Some(paused)` while a station is loaded.
    pub(super) radio: Option<bool>,
    /// `lum display` has HiDPI off.
    pub(super) monitor: bool,
    /// `lum net game` is on: the monitor is drawn, HiDPI off or not, with its screen lit.
    pub(super) game: bool,
}

/// What covers one sample of the icon.
enum Hit {
    Monitor,
    /// The lit screen of game mode.
    Screen,
    Radio,
    Rim,
    Empty,
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
/// radio with its holes filled in, which is what the rim and the badge's gap follow.
fn radio(p: Point, antenna: bool) -> (f32, f32) {
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

/// Signed distances at `p`, negative inside: `(monitor, silhouette, screen)`. A frame around
/// the screen, which is a hole, on a stand, centred on the grid like the playing radio. The
/// screen's own distance is what game mode lights.
fn monitor(p: Point) -> (f32, f32, f32) {
    let frame = rounded_box(p, (1.0, 1.75), (15.0, 11.75), 1.5);
    let neck = rounded_box(p, (7.0, 11.0), (9.0, 13.0), 0.0);
    let foot = rounded_box(p, (4.0, 12.75), (12.0, 14.25), 0.75);
    let silhouette = frame.min(neck).min(foot);
    let screen = rounded_box(p, (2.5, 3.25), (13.5, 10.25), 0.5);
    (silhouette.max(-screen), silhouette, screen)
}

/// Signed distances at `p`, negative inside: `(monitor, silhouette, screen)`. The monitor
/// under the radio: no stand, which the radio would cut in two, and the screen filling the
/// grid.
fn badged_monitor(p: Point) -> (f32, f32, f32) {
    let silhouette = rounded_box(p, (0.5, 1.5), (15.5, 13.0), 1.75);
    let screen = rounded_box(p, (2.0, 3.0), (14.0, 11.5), 0.5);
    (silhouette.max(-screen), silhouette, screen)
}

/// What covers grid point `p`.
fn hit(look: Look, p: Point) -> Hit {
    const FAR: f32 = f32::INFINITY;
    // Game mode lights the monitor's screen, so it draws the monitor whether HiDPI is off or not.
    let with_monitor = look.monitor || look.game;
    let radio = match (look.radio, with_monitor) {
        (None, _) => (FAR, FAR),
        // Alone, the folded radio is drawn FOLDED_LIFT higher (sampling lower moves the shape
        // up): playing, the antenna balances the body about the grid centre, and folded the
        // body alone would sit low.
        (Some(paused), false) => {
            let lift = if paused { FOLDED_LIFT } else { 0.0 };
            radio((p.0, p.1 + lift), !paused)
        }
        // Over the monitor, scaled into its bottom-right corner, where the body stays put
        // whether the antenna is up or not.
        (Some(paused), true) => {
            let offset = BADGE_CORNER - 15.0 * BADGE_SCALE;
            let (r, s) = radio(
                ((p.0 - offset) / BADGE_SCALE, (p.1 - offset) / BADGE_SCALE),
                !paused,
            );
            (r * BADGE_SCALE, s * BADGE_SCALE)
        }
    };
    let monitor = if !with_monitor {
        (FAR, FAR, FAR)
    } else if look.radio.is_some() && radio.1 <= BADGE_GAP {
        // Cut away around the radio.
        (FAR, -FAR, FAR)
    } else if look.radio.is_some() {
        badged_monitor(p)
    } else {
        monitor(p)
    };
    let on_rim = |silhouette: f32| silhouette > 0.0 && silhouette <= RIM_WIDTH;
    if radio.0 <= 0.0 {
        Hit::Radio
    } else if monitor.0 <= 0.0 {
        Hit::Monitor
    } else if look.game && monitor.2 <= 0.0 {
        Hit::Screen
    } else if on_rim(radio.1) || on_rim(monitor.1) {
        Hit::Rim
    } else {
        Hit::Empty
    }
}

/// RGBA pixels (straight alpha) of the icon, `SIZE` x `SIZE`.
pub(super) fn rgba(look: Look, ink: Ink) -> Vec<u8> {
    let samples = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    let unit = GRID / SIZE as f32;
    let paused = look.radio == Some(true);
    let (monitor_fill, radio_fill, screen_fill) = match ink {
        Ink::Colour => (PAUSED, if paused { PAUSED } else { PLAYING }, GAME),
        Ink::Template => ([0; 3], [0; 3], [0; 3]),
        Ink::MenuBar { dark } => {
            let (tint, green, blue) = if dark {
                ([255; 3], MENU_BAR_GREEN_DARK, MENU_BAR_BLUE_DARK)
            } else {
                ([0; 3], MENU_BAR_GREEN, MENU_BAR_BLUE)
            };
            (tint, if paused { tint } else { green }, blue)
        }
    };
    let radio_strength = if paused && ink != Ink::Colour {
        PAUSED_TEMPLATE_ALPHA
    } else {
        1.0
    };
    let rim = if ink == Ink::Colour { RIM_ALPHA } else { 0.0 };

    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            // Opacity-weighted sums of the colour, and the opacity.
            let (mut sum, mut alpha) = ([0.0_f32; 3], 0.0_f32);
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let at = |pixel: u32, sub: u32| {
                        (pixel as f32 + (sub as f32 + 0.5) / SUPERSAMPLE as f32) * unit
                    };
                    let (colour, opacity) = match hit(look, (at(x, sx), at(y, sy))) {
                        Hit::Monitor => (monitor_fill, 1.0),
                        Hit::Screen => (screen_fill, 1.0),
                        Hit::Radio => (radio_fill, radio_strength),
                        Hit::Rim => ([255; 3], rim),
                        Hit::Empty => continue,
                    };
                    for (total, channel) in sum.iter_mut().zip(colour) {
                        *total += f32::from(channel) * opacity / samples;
                    }
                    alpha += opacity / samples;
                }
            }
            for total in sum {
                rgba.push(if alpha > 0.0 {
                    (total / alpha).round() as u8
                } else {
                    0
                });
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
    const INKS: [Ink; 4] = [
        Ink::Colour,
        Ink::Template,
        Ink::MenuBar { dark: false },
        Ink::MenuBar { dark: true },
    ];
    /// The middle of the lit screen on its own, where the unlit one is a hole.
    const SCREEN_CENTRE: (f32, f32) = (8.0, 6.75);

    /// Every look the tray can show: no radio, a paused one or a playing one, each with and without
    /// the monitor and the lit screen, except the look that draws nothing.
    fn looks() -> Vec<Look> {
        let mut looks = Vec::new();
        for radio in [None, Some(true), Some(false)] {
            for monitor in STATES {
                for game in STATES {
                    if radio.is_some() || monitor || game {
                        looks.push(Look {
                            radio,
                            monitor,
                            game,
                        });
                    }
                }
            }
        }
        looks
    }

    fn radio_icon(paused: bool, ink: Ink) -> Vec<u8> {
        rgba(
            Look {
                radio: Some(paused),
                monitor: false,
                game: false,
            },
            ink,
        )
    }

    fn monitor_icon(ink: Ink) -> Vec<u8> {
        rgba(
            Look {
                radio: None,
                monitor: true,
                game: false,
            },
            ink,
        )
    }

    fn both_icon(paused: bool, ink: Ink) -> Vec<u8> {
        rgba(
            Look {
                radio: Some(paused),
                monitor: true,
                game: false,
            },
            ink,
        )
    }

    /// Game mode: the lit screen, alone or under a radio.
    fn game_icon(radio: Option<bool>, ink: Ink) -> Vec<u8> {
        rgba(
            Look {
                radio,
                monitor: false,
                game: true,
            },
            ink,
        )
    }

    /// A grid point in the badge's own radio grid, mapped onto the monitor's.
    fn on_badge((x, y): (f32, f32)) -> (f32, f32) {
        let offset = BADGE_CORNER - 15.0 * BADGE_SCALE;
        (offset + x * BADGE_SCALE, offset + y * BADGE_SCALE)
    }

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

    /// Where the middle of a drawing's inked rows sits, in grid units from the top.
    fn vertical_centre(drawing: &[u8]) -> f32 {
        let alpha = |x: u32, y: u32| drawing[((y * SIZE + x) * 4 + 3) as usize];
        let inked: Vec<u32> = (0..SIZE)
            .filter(|&y| (0..SIZE).any(|x| alpha(x, y) >= 128))
            .collect();
        let (top, bottom) = (inked[0], inked[inked.len() - 1]);
        (top + bottom + 1) as f32 / 2.0 * (GRID / SIZE as f32)
    }

    #[test]
    fn the_icon_is_one_square_in_every_look() {
        for look in looks() {
            for ink in INKS {
                assert_eq!(
                    rgba(look, ink).len(),
                    (SIZE * SIZE * 4) as usize,
                    "{look:?}"
                );
            }
        }
    }

    #[test]
    fn the_radio_sits_over_the_monitors_corner_with_a_gap_cut_around_it() {
        // Solid body in the radio's own grid; the monitor's frame far from the corner; its
        // bottom edge just left of the radio, inside the gap but beyond the rim.
        let (body, frame, in_the_gap) = (on_badge((2.0, 10.5)), (1.25, 6.75), (5.65, 12.25));
        assert!(
            badged_monitor(in_the_gap).0 < 0.0,
            "the frame runs there before the cut"
        );
        for paused in STATES {
            for ink in INKS {
                let both = both_icon(paused, ink);
                assert!(pixel(&both, body)[3] > 0, "{ink:?} paused={paused}");
                assert_eq!(pixel(&both, frame)[3], 255, "{ink:?} paused={paused}");
                assert_eq!(pixel(&both, in_the_gap)[3], 0, "{ink:?} paused={paused}");
            }
        }
        assert_eq!(
            pixel(&both_icon(false, Ink::Colour), body),
            [PLAYING[0], PLAYING[1], PLAYING[2], 255]
        );
        assert_eq!(
            pixel(&both_icon(false, Ink::Colour), frame),
            [PAUSED[0], PAUSED[1], PAUSED[2], 255]
        );
    }

    #[test]
    fn the_radio_over_the_monitor_keeps_its_antenna_cue() {
        let on_the_antenna = on_badge((5.5, 2.2));
        for ink in INKS {
            assert!(
                pixel(&both_icon(false, ink), on_the_antenna)[3] > 0,
                "{ink:?}"
            );
            assert_eq!(
                pixel(&both_icon(true, ink), on_the_antenna)[3],
                0,
                "{ink:?}"
            );
        }
    }

    #[test]
    fn the_icon_has_no_background_plate() {
        for look in looks() {
            for ink in INKS {
                let rgba = rgba(look, ink);
                for (x, y) in [(0, 0), (SIZE - 1, 0), (0, SIZE - 1), (SIZE - 1, SIZE - 1)] {
                    let alpha = rgba[((y * SIZE + x) * 4 + 3) as usize];
                    assert_eq!(alpha, 0, "{ink:?} {look:?}");
                }
            }
        }
    }

    #[test]
    fn the_antenna_is_up_only_while_playing() {
        // The antenna's tip: above the lifted body, so empty once it is folded away.
        let on_the_antenna = (5.5, 2.2);
        for ink in INKS {
            assert_eq!(pixel(&radio_icon(false, ink), on_the_antenna)[3], 255);
            assert_eq!(pixel(&radio_icon(true, ink), on_the_antenna)[3], 0);
        }
    }

    #[test]
    fn state_shows_in_the_silhouette_and_not_only_in_the_colour() {
        // Colour alone is a poor cue (colour-blind users, and a template has no colour).
        for ink in INKS {
            assert_ne!(
                alphas(&radio_icon(false, ink)),
                alphas(&radio_icon(true, ink)),
                "{ink:?}"
            );
            // Game mode fills the screen, and the radio over it still folds.
            assert_ne!(
                alphas(&monitor_icon(ink)),
                alphas(&game_icon(None, ink)),
                "{ink:?} lit screen"
            );
            assert_ne!(
                alphas(&game_icon(Some(false), ink)),
                alphas(&game_icon(Some(true), ink)),
                "{ink:?} radio over the lit screen"
            );
        }
    }

    #[test]
    fn the_speaker_is_a_hole_that_takes_the_panels_colour() {
        // The speaker's centre, lifted with the folded radio.
        for (paused, centre) in [(false, (6.0, 10.5)), (true, (6.0, 10.5 - FOLDED_LIFT))] {
            for ink in INKS {
                assert_eq!(
                    pixel(&radio_icon(paused, ink), centre)[3],
                    0,
                    "{ink:?} paused={paused}"
                );
            }
        }
    }

    #[test]
    fn the_screen_is_a_hole_in_a_frame_on_a_stand() {
        for ink in INKS {
            let monitor = monitor_icon(ink);
            assert_eq!(pixel(&monitor, (8.0, 6.75))[3], 0, "{ink:?} screen");
            assert_eq!(pixel(&monitor, (1.75, 6.75))[3], 255, "{ink:?} frame");
            assert_eq!(pixel(&monitor, (8.0, 13.5))[3], 255, "{ink:?} foot");
        }
    }

    #[test]
    fn the_lit_screen_is_filled_where_the_unlit_one_is_a_hole() {
        for ink in INKS {
            assert_eq!(
                pixel(&monitor_icon(ink), SCREEN_CENTRE)[3],
                0,
                "{ink:?} unlit"
            );
            assert_eq!(
                pixel(&game_icon(None, ink), SCREEN_CENTRE)[3],
                255,
                "{ink:?} lit"
            );
        }
    }

    #[test]
    fn the_lit_screen_is_blue_in_the_frame_of_the_stopped_grey() {
        let lit = game_icon(None, Ink::Colour);
        assert_eq!(pixel(&lit, SCREEN_CENTRE), [GAME[0], GAME[1], GAME[2], 255]);
        assert_eq!(
            pixel(&lit, (1.75, 6.75)),
            [PAUSED[0], PAUSED[1], PAUSED[2], 255]
        );
        assert_ne!(
            GAME, PLAYING,
            "a lit screen must not read as a playing radio"
        );
    }

    #[test]
    fn game_mode_draws_the_monitor_whether_or_not_hidpi_is_off() {
        for radio in [None, Some(false), Some(true)] {
            for ink in INKS {
                let lit = |monitor| {
                    rgba(
                        Look {
                            radio,
                            monitor,
                            game: true,
                        },
                        ink,
                    )
                };
                assert_eq!(lit(false), lit(true), "{radio:?} {ink:?}");
            }
        }
    }

    #[test]
    fn a_radio_over_the_lit_screen_keeps_its_gap_and_its_antenna_cue() {
        // The lit screen far from the radio; the radio's solid body; a point of the screen inside
        // the gap beside the body, beyond the rim; another inside the gap past the antenna's tip.
        let lit = (4.0, 5.0);
        let (body, beside_the_body) = (on_badge((2.0, 10.5)), (5.7, 11.0));
        let past_the_tip = on_badge((3.77, 0.71));
        for point in [beside_the_body, past_the_tip] {
            assert!(
                badged_monitor(point).2 < 0.0,
                "the screen runs at {point:?} before the cut"
            );
        }
        for ink in INKS {
            let (playing, paused) = (game_icon(Some(false), ink), game_icon(Some(true), ink));
            for drawing in [&playing, &paused] {
                assert_eq!(pixel(drawing, lit)[3], 255, "{ink:?} lit");
                assert!(pixel(drawing, body)[3] > 0, "{ink:?} radio");
                assert_eq!(pixel(drawing, beside_the_body)[3], 0, "{ink:?} gap");
            }
            // The antenna is cut out of the screen with a gap of its own, which a pixel may
            // only just overlap; the folded radio has no antenna, so the screen is lit there.
            let cut = pixel(&playing, past_the_tip)[3];
            assert!(cut < 32, "{ink:?} antenna gap is {cut}/255 inked");
            assert_eq!(pixel(&paused, past_the_tip)[3], 255, "{ink:?} no antenna");
        }
    }

    #[test]
    fn the_monitor_and_the_stopped_radio_are_vertically_centred() {
        let drawings = [
            ("monitor", monitor_icon(Ink::Template)),
            ("lit monitor", game_icon(None, Ink::Template)),
            ("stopped radio", radio_icon(true, Ink::Template)),
        ];
        for (name, drawing) in drawings {
            let centre = vertical_centre(&drawing);
            assert!(
                (centre - GRID / 2.0).abs() <= 0.05,
                "the {name} is centred at {centre}, not {}",
                GRID / 2.0
            );
        }
    }

    #[test]
    fn the_body_is_solid_in_the_palette_colour() {
        let solid_body = (2.0, 10.5);
        assert_eq!(
            pixel(&radio_icon(false, Ink::Colour), solid_body),
            [PLAYING[0], PLAYING[1], PLAYING[2], 255]
        );
        assert_eq!(
            pixel(&radio_icon(true, Ink::Colour), solid_body),
            [PAUSED[0], PAUSED[1], PAUSED[2], 255]
        );
    }

    #[test]
    fn the_colour_rim_is_white_and_stays_outside_the_radio() {
        // Just left of the body's edge, which sits at x = 1.0.
        let beside_the_body = (0.8, 10.5);
        let [r, g, b, a] = pixel(&radio_icon(false, Ink::Colour), beside_the_body);
        assert_eq!([r, g, b], [255, 255, 255]);
        assert_eq!(a, (RIM_ALPHA * 255.0).round() as u8);
        assert_eq!(
            pixel(&radio_icon(false, Ink::Template), beside_the_body)[3],
            0
        );
    }

    #[test]
    fn template_ink_is_black_and_a_paused_template_is_fainter() {
        for look in looks() {
            let drawing = rgba(look, Ink::Template);
            for p in drawing.as_chunks::<4>().0.iter().filter(|p| p[3] > 0) {
                assert_eq!(
                    p[..3],
                    [0, 0, 0],
                    "macOS tints the alpha shape; the colour is ignored"
                );
            }
        }
        let strongest = |drawing: Vec<u8>| *alphas(&drawing).iter().max().unwrap();
        assert_eq!(strongest(monitor_icon(Ink::Template)), 255);
        assert_eq!(strongest(game_icon(None, Ink::Template)), 255);
        assert_eq!(strongest(radio_icon(false, Ink::Template)), 255);
        assert_eq!(
            strongest(radio_icon(true, Ink::Template)),
            (PAUSED_TEMPLATE_ALPHA * 255.0).round() as u8
        );
        let paused_badge = on_badge((2.0, 10.5));
        assert_eq!(
            pixel(&both_icon(true, Ink::Template), paused_badge)[3],
            (PAUSED_TEMPLATE_ALPHA * 255.0).round() as u8
        );
    }

    #[test]
    fn menu_bar_ink_tints_like_a_template_and_paints_the_radio_green() {
        let (body, frame) = (on_badge((2.0, 10.5)), (1.25, 6.75));
        for (dark, tint, green) in [
            (false, [0, 0, 0], MENU_BAR_GREEN),
            (true, [255, 255, 255], MENU_BAR_GREEN_DARK),
        ] {
            let both = both_icon(false, Ink::MenuBar { dark });
            assert_eq!(pixel(&both, frame), [tint[0], tint[1], tint[2], 255]);
            assert_eq!(pixel(&both, body), [green[0], green[1], green[2], 255]);
            // No rim: the menu bar is known.
            let beside_the_body = (0.8, 10.5);
            assert_eq!(
                pixel(&radio_icon(false, Ink::MenuBar { dark }), beside_the_body)[3],
                0
            );
        }
    }

    #[test]
    fn menu_bar_ink_paints_the_lit_screen_blue_in_a_frame_of_the_tint() {
        let (lit, body, frame) = ((4.0, 5.0), on_badge((2.0, 10.5)), (1.25, 6.75));
        for (dark, tint, blue, green) in [
            (false, [0, 0, 0], MENU_BAR_BLUE, MENU_BAR_GREEN),
            (
                true,
                [255, 255, 255],
                MENU_BAR_BLUE_DARK,
                MENU_BAR_GREEN_DARK,
            ),
        ] {
            let ink = Ink::MenuBar { dark };
            let alone = game_icon(None, ink);
            assert_eq!(
                pixel(&alone, SCREEN_CENTRE),
                [blue[0], blue[1], blue[2], 255]
            );
            assert_eq!(pixel(&alone, frame), [tint[0], tint[1], tint[2], 255]);
            for paused in STATES {
                let badged = game_icon(Some(paused), ink);
                assert_eq!(pixel(&badged, lit), [blue[0], blue[1], blue[2], 255]);
                assert_eq!(pixel(&badged, frame), [tint[0], tint[1], tint[2], 255]);
                let [r, g, b, a] = pixel(&badged, body);
                let radio = if paused { tint } else { green };
                assert_eq!([r, g, b], radio, "paused={paused}");
                assert!(a > 0, "paused={paused}");
            }
        }
    }

    #[test]
    fn menu_bar_greens_and_blues_read_on_their_menu_bars() {
        let light = [
            ("macOS light menu bar", [0xf6, 0xf6, 0xf6]),
            ("white", [0xff; 3]),
        ];
        let dark = [
            ("macOS dark menu bar", [0x2c, 0x2c, 0x2e]),
            ("black", [0; 3]),
        ];
        for (name, ink, panels) in [
            ("green", MENU_BAR_GREEN, light),
            ("blue", MENU_BAR_BLUE, light),
            ("green", MENU_BAR_GREEN_DARK, dark),
            ("blue", MENU_BAR_BLUE_DARK, dark),
        ] {
            for (panel, colour) in panels {
                let ratio = contrast(ink, colour);
                assert!(ratio >= 3.0, "{name} is {ratio:.1}:1 on {panel}");
            }
        }
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
            ("macOS dark menu bar", [0x2c, 0x2c, 0x2e]),
            ("macOS light menu bar", [0xf6, 0xf6, 0xf6]),
        ];
        for (state, fill) in [
            ("playing", PLAYING),
            ("paused", PAUSED),
            ("game screen", GAME),
        ] {
            for (panel, colour) in panels {
                let ratio = contrast(fill, colour);
                assert!(ratio >= 3.0, "{state} fill is {ratio:.1}:1 on {panel}");
            }
        }
    }
}
