#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StationKind {
    Direct,
    YouTube,
    YouTubePlaylist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Station {
    pub code: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub kind: StationKind,
    pub tags: &'static [&'static str],
}

/// Reserved selector code that plays a random built-in station. Never a catalog entry.
pub const RANDOM_CODE: &str = "rand";

const STATIONS: &[Station] = &[
    Station {
        code: "conc",
        name: "RadCap Musique Concrète",
        url: "http://213.141.131.10:8000/musiqueconcrete",
        kind: StationKind::Direct,
        tags: &["concrète", "acousmatic", "collage"],
    },
    Station {
        code: "expr",
        name: "RadCap Experimental",
        url: "http://79.111.119.111:8004/experimentalmusic",
        kind: StationKind::Direct,
        tags: &["experimental"],
    },
    Station {
        code: "impr",
        name: "RadCap Free Improvisation",
        url: "http://79.111.119.111:8000/avantgardejazz",
        kind: StationKind::Direct,
        tags: &["free improv", "avant-garde jazz"],
    },
    Station {
        code: "cavt",
        name: "RadCap Classical Avant-garde",
        url: "http://213.141.131.10:8004/classavant",
        kind: StationKind::Direct,
        tags: &["avant-garde", "classical", "modernist"],
    },
    Station {
        code: "mini",
        name: "RadCap Minimalism",
        url: "http://213.141.131.10:8000/minimalism",
        kind: StationKind::Direct,
        tags: &["minimalism"],
    },
    Station {
        code: "dron",
        name: "SomaFM Drone Zone",
        url: "https://ice1.somafm.com/dronezone-128-mp3",
        kind: StationKind::Direct,
        tags: &["drone", "ambient"],
    },
    Station {
        code: "rdrn",
        name: "RadCap Drone Ambient",
        url: "http://79.111.119.111:8000/droneambient",
        kind: StationKind::Direct,
        tags: &["drone", "ambient"],
    },
    Station {
        code: "dark",
        name: "SomaFM Dark Zone",
        url: "https://ice1.somafm.com/darkzone-128-mp3",
        kind: StationKind::Direct,
        tags: &["dark ambient", "soundscape"],
    },
    Station {
        code: "ritl",
        name: "RadCap Dark/Ritual Ambient",
        url: "http://79.120.39.202:8000/darkambient",
        kind: StationKind::Direct,
        tags: &["dark ambient", "ritual"],
    },
    Station {
        code: "dsp1",
        name: "SomaFM Deep Space One",
        url: "https://ice1.somafm.com/deepspaceone-128-mp3",
        kind: StationKind::Direct,
        tags: &["space", "ambient"],
    },
    Station {
        code: "slow",
        name: "NTS Slow Focus",
        url: "https://stream-mixtape-geo.ntslive.net/mixtape",
        kind: StationKind::Direct,
        tags: &["experimental", "slow"],
    },
    Station {
        code: "fiel",
        name: "NTS Field Recordings",
        url: "https://stream-mixtape-geo.ntslive.net/mixtape23",
        kind: StationKind::Direct,
        tags: &["field recordings"],
    },
    Station {
        code: "pond",
        name: "Wave Farm Pond Station",
        url: "https://audio.wavefarm.org/pondstation.mp3",
        kind: StationKind::Direct,
        tags: &["field recordings", "hydrophone", "live"],
    },
    Station {
        code: "n5md",
        name: "SomaFM n5MD Radio",
        url: "https://ice1.somafm.com/n5md-128-mp3",
        kind: StationKind::Direct,
        tags: &["electronic", "idm"],
    },
    Station {
        code: "arab",
        name: "Arab Mix FM",
        url: "https://stream.zeno.fm/na3vpvn10qruv.acc",
        kind: StationKind::Direct,
        tags: &["arabic", "world"],
    },
    Station {
        code: "ytlf",
        name: "Lofi Girl",
        url: "https://www.youtube.com/@LofiGirl/live",
        kind: StationKind::YouTube,
        tags: &["lo-fi", "live"],
    },
    Station {
        code: "aphx",
        name: "Aphex Twin albums",
        url: "playlist:aphx",
        kind: StationKind::YouTubePlaylist,
        tags: &["electronic", "idm", "playlist"],
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
    let width = all()
        .iter()
        .map(|station| station.name.chars().count())
        .max()
        .unwrap_or(0);
    all()
        .iter()
        .map(|station| {
            format!(
                "{:<4}  {:<width$}  {}",
                station.code,
                station.name,
                station.tags.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_codes_are_stable() {
        let codes: Vec<_> = all().iter().map(|station| station.code).collect();
        assert_eq!(
            codes,
            [
                "conc", "expr", "impr", "cavt", "mini", "dron", "rdrn", "dark", "ritl", "dsp1",
                "slow", "fiel", "pond", "n5md", "arab", "ytlf", "aphx",
            ]
        );
    }

    #[test]
    fn finds_station_by_exact_code() {
        assert_eq!(
            find("conc").unwrap().url,
            "http://213.141.131.10:8000/musiqueconcrete"
        );
        assert_eq!(
            find("pond").unwrap().url,
            "https://audio.wavefarm.org/pondstation.mp3"
        );
        assert!(find("CONC").is_none());
    }

    #[test]
    fn eligible_stations_drop_only_the_remembered_station() {
        let eligible = eligible_stations(Some("conc"));

        assert_eq!(eligible.len(), all().len() - 1);
        assert_eq!(eligible.first().map(|station| station.code), Some("expr"));
        assert_eq!(eligible.last().map(|station| station.code), Some("aphx"));
        assert!(eligible.iter().all(|station| station.code != "conc"));
    }

    #[test]
    fn eligible_stations_ignore_a_retired_exclusion_code() {
        let eligible = eligible_stations(Some("atma"));

        assert_eq!(eligible.len(), all().len());
    }

    #[test]
    fn eligible_stations_without_exclusion_are_the_whole_catalog() {
        let eligible = eligible_stations(None);

        assert_eq!(eligible.len(), all().len());
        assert_eq!(eligible.first().map(|station| station.code), Some("conc"));
        assert_eq!(eligible.last().map(|station| station.code), Some("aphx"));
    }

    #[test]
    fn choose_station_draws_from_the_pool_with_the_injected_index() {
        let eligible = eligible_stations(Some("conc"));
        let mut observed_len = 0;

        let first = choose_station(&eligible, |len| {
            observed_len = len;
            0
        });
        let last = eligible.len() - 1;
        let last = choose_station(&eligible, |_| last);

        assert_eq!(observed_len, eligible.len());
        assert_eq!(first.map(|station| station.code), Some("expr"));
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
    fn retained_stations_keep_their_tags() {
        assert!(find("arab").unwrap().tags.contains(&"world"));
        assert!(find("ytlf").unwrap().tags.contains(&"lo-fi"));
        assert!(find("aphx").unwrap().tags.contains(&"idm"));
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
        assert_eq!(station.name, "Aphex Twin albums");
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
    fn listing_has_one_row_per_station_with_name_and_tags() {
        let listing = format_listing();
        let rows: Vec<_> = listing.lines().collect();

        assert_eq!(rows.len(), all().len());
        for (row, station) in rows.iter().zip(all()) {
            assert!(row.starts_with(station.code), "{row}");
            assert!(row.contains(station.name), "{row}");
            assert!(row.ends_with(&station.tags.join(", ")), "{row}");
            assert!(!station.tags.is_empty(), "{} needs tags", station.code);
        }
    }
}
