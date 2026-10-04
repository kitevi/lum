#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StationKind {
    Direct,
    YouTube,
    YouTubePlaylist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Station {
    pub code: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    pub kind: StationKind,
}

/// Reserved selector code that plays a random built-in station. Never a catalog entry.
pub const RANDOM_CODE: &str = "rand";

const STATIONS: &[Station] = &[
    Station {
        code: "atma",
        description: "atma.fm Channel 1 - Ambient and experimental electroacoustic music",
        url: "https://atma.fm/channel1",
        kind: StationKind::Direct,
    },
    Station {
        code: "atm2",
        description: "atma.fm Channel 2 - Darkwave, dark ambient, and neoclassical/gothic music",
        url: "https://atma.fm/channel2",
        kind: StationKind::Direct,
    },
    Station {
        code: "ssom",
        description: "SomaFM Space Station Soma - Spaced-out ambient and mid-tempo electronica",
        url: "https://ice5.somafm.com/spacestation-128-mp3",
        kind: StationKind::Direct,
    },
    Station {
        code: "beat",
        description: "SomaFM Beat Blender - Eclectic downtempo and electronic music",
        url: "https://ice5.somafm.com/beatblender-128-mp3",
        kind: StationKind::Direct,
    },
    Station {
        code: "grve",
        description: "SomaFM Groove Salad - Listener-supported downtempo and chill electronic music",
        url: "https://ice5.somafm.com/groovesalad-128-mp3",
        kind: StationKind::Direct,
    },
    Station {
        code: "nood",
        description: "Noods Radio - Music-heavy community radio from Bristol",
        url: "https://noods-radio.radiocult.fm/stream",
        kind: StationKind::Direct,
    },
    Station {
        code: "drmm",
        description: "Intergalactic FM Dream Machine - Experimental music from The Hague",
        url: "https://radio.intergalactic.fm/3A",
        kind: StationKind::Direct,
    },
    Station {
        code: "9128",
        description: "9128.live - Curated ambient/drone stream with zero talk",
        url: "https://streams.radio.co/s0aa1e6f4a/listen",
        kind: StationKind::Direct,
    },
    Station {
        code: "arab",
        description: "Arab Mix FM - Arabic music stream replacement for Radio Alhara",
        url: "https://stream.zeno.fm/na3vpvn10qruv.acc",
        kind: StationKind::Direct,
    },
    Station {
        code: "ytlf",
        description: "Lofi Girl - current featured live stream",
        url: "https://www.youtube.com/@LofiGirl/live",
        kind: StationKind::YouTube,
    },
    Station {
        code: "aphx",
        description: "Aphex Twin album playlist",
        url: "playlist:aphx",
        kind: StationKind::YouTubePlaylist,
    },
];

pub const fn all() -> &'static [Station] {
    STATIONS
}

pub fn find(code: &str) -> Option<&'static Station> {
    STATIONS.iter().find(|station| station.code == code)
}

/// Draws one station from `pool`. `choose` receives the pool length and must
/// return an index below it, so tests can pin the draw without touching the RNG.
pub fn choose_station(
    pool: &[&'static Station],
    mut choose: impl FnMut(usize) -> usize,
) -> Option<&'static Station> {
    match pool.split_first() {
        Some(_) => Some(pool[choose(pool.len())]),
        None => None,
    }
}

/// Catalog stations eligible for a random draw, minus the station to skip.
pub fn eligible_stations(exclude_code: Option<&str>) -> Vec<&'static Station> {
    STATIONS
        .iter()
        .filter(|station| Some(station.code) != exclude_code)
        .collect()
}

pub fn playlist_urls(code: &str) -> Option<&'static [&'static str]> {
    match code {
        "aphx" => Some(&[
            "https://www.youtube.com/watch?v=oR4gjzXs5EE",
            "https://www.youtube.com/watch?v=Xw5AiRVqfqk",
        ]),
        _ => None,
    }
}

/// A `lum radio` command that dispatches to code rather than to a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PseudoCommand {
    pub code: &'static str,
    pub description: &'static str,
}

const PSEUDO_COMMANDS: &[PseudoCommand] = &[PseudoCommand {
    code: RANDOM_CODE,
    description: "Play a random built-in station",
}];

pub fn format_pseudo_commands() -> String {
    PSEUDO_COMMANDS
        .iter()
        .map(|command| format!("{:<4}  {}", command.code, command.description))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Catalog plus non-station commands, used by `lum radio list` and unknown-code errors.
pub fn format_reference() -> String {
    format!("{}\n\n{}", format_listing(), format_pseudo_commands())
}

pub fn format_listing() -> String {
    all()
        .iter()
        .map(|station| format!("{:<4}  {}", station.code, station.description))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_ruv_codes() {
        let codes: Vec<_> = all().iter().map(|station| station.code).collect();
        assert_eq!(
            codes,
            [
                "atma", "atm2", "ssom", "beat", "grve", "nood", "drmm", "9128", "arab", "ytlf",
                "aphx",
            ]
        );
    }

    #[test]
    fn finds_station_by_exact_code() {
        assert_eq!(find("atma").unwrap().url, "https://atma.fm/channel1");
        assert_eq!(find("atm2").unwrap().url, "https://atma.fm/channel2");
        assert!(find("ATMA").is_none());
    }

    #[test]
    fn eligible_stations_drop_only_the_remembered_station() {
        let eligible = eligible_stations(Some("atma"));

        assert_eq!(eligible.len(), all().len() - 1);
        assert_eq!(eligible.first().map(|station| station.code), Some("atm2"));
        assert_eq!(eligible.last().map(|station| station.code), Some("aphx"));
        assert!(eligible.iter().all(|station| station.code != "atma"));
    }

    #[test]
    fn eligible_stations_without_exclusion_are_the_whole_catalog() {
        let eligible = eligible_stations(None);

        assert_eq!(eligible.len(), all().len());
        assert_eq!(eligible.first().map(|station| station.code), Some("atma"));
        assert_eq!(eligible.last().map(|station| station.code), Some("aphx"));
    }

    #[test]
    fn choose_station_draws_from_the_pool_with_the_injected_index() {
        let eligible = eligible_stations(Some("atma"));
        let mut observed_len = 0;

        let first = choose_station(&eligible, |len| {
            observed_len = len;
            0
        });
        let last = eligible.len() - 1;
        let last = choose_station(&eligible, |_| last);

        assert_eq!(observed_len, eligible.len());
        assert_eq!(first.map(|station| station.code), Some("atm2"));
        assert_eq!(last.map(|station| station.code), Some("aphx"));
    }

    #[test]
    fn choose_station_has_no_candidate_for_an_empty_pool() {
        assert!(choose_station(&[], |_| 0).is_none());
    }

    #[test]
    fn station_reference_lists_stations_then_the_random_selector() {
        let reference = format_reference();

        assert_eq!(
            reference,
            format!(
                "{}\n\nrand  Play a random built-in station",
                format_listing()
            )
        );
        assert!(
            !format_listing()
                .lines()
                .any(|line| line.starts_with("rand "))
        );
    }

    #[test]
    fn finds_youtube_station_by_code() {
        let station = find("ytlf").expect("ytlf station should exist");
        assert_eq!(station.kind, StationKind::YouTube);
    }

    #[test]
    fn ytlf_follows_the_channel_featured_live_stream() {
        let station = find("ytlf").expect("ytlf station should exist");
        assert_eq!(station.url, "https://www.youtube.com/@LofiGirl/live");
        assert_eq!(station.kind, StationKind::YouTube);
    }

    #[test]
    fn aphx_station_is_a_youtube_playlist_with_clean_album_urls() {
        let station = find("aphx").expect("aphx station should exist");
        assert_eq!(station.kind, StationKind::YouTubePlaylist);
        assert_eq!(station.description, "Aphex Twin album playlist");
        assert_eq!(
            playlist_urls("aphx").expect("aphx playlist should have urls"),
            &[
                "https://www.youtube.com/watch?v=oR4gjzXs5EE",
                "https://www.youtube.com/watch?v=Xw5AiRVqfqk",
            ]
        );
    }

    #[test]
    fn random_selector_is_reserved_and_never_a_catalog_entry() {
        assert!(find(RANDOM_CODE).is_none());
        assert!(!all().iter().any(|station| station.code == RANDOM_CODE));
    }

    #[test]
    fn listing_matches_ruv_plain_format() {
        let listing = format_listing();
        assert!(listing.starts_with("atma  atma.fm Channel 1"));
        assert!(listing.contains(
            "\natm2  atma.fm Channel 2 - Darkwave, dark ambient, and neoclassical/gothic music"
        ));
        assert!(listing.contains("\nssom  SomaFM Space Station Soma"));
        assert!(listing.ends_with("aphx  Aphex Twin album playlist"));
        assert_eq!(listing.lines().count(), all().len());
    }
}
