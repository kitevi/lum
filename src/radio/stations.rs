#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StationKind {
    Direct,
    YouTube,
    YouTubePlaylist,
}

/// Musical grouping for a station. Declaration order is the display order
/// used by `format_listing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genre {
    ElectroacousticAndConcrete,
    ExperimentalAndAvantGarde,
    MinimalAndReductionist,
    Drone,
    DarkAmbient,
    SpaceAndAmbient,
    FieldRecordings,
    ExperimentalElectronic,
    World,
    LoFi,
}

impl Genre {
    pub const ALL: &[Genre] = &[
        Genre::ElectroacousticAndConcrete,
        Genre::ExperimentalAndAvantGarde,
        Genre::MinimalAndReductionist,
        Genre::Drone,
        Genre::DarkAmbient,
        Genre::SpaceAndAmbient,
        Genre::FieldRecordings,
        Genre::ExperimentalElectronic,
        Genre::World,
        Genre::LoFi,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Genre::ElectroacousticAndConcrete => "Electroacoustic and concrète",
            Genre::ExperimentalAndAvantGarde => "Experimental and avant-garde",
            Genre::MinimalAndReductionist => "Minimal and reductionist",
            Genre::Drone => "Drone",
            Genre::DarkAmbient => "Dark ambient",
            Genre::SpaceAndAmbient => "Space and ambient",
            Genre::FieldRecordings => "Field recordings",
            Genre::ExperimentalElectronic => "Experimental electronic",
            Genre::World => "World",
            Genre::LoFi => "Lo-fi",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Station {
    pub code: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    pub kind: StationKind,
    pub genre: Genre,
}

/// Reserved selector code that plays a random built-in station. Never a catalog entry.
pub const RANDOM_CODE: &str = "rand";

const STATIONS: &[Station] = &[
    Station {
        code: "conc",
        description: "RadCap Musique Concrète - Acousmatic music and sound collage",
        url: "http://213.141.131.10:8000/musiqueconcrete",
        kind: StationKind::Direct,
        genre: Genre::ElectroacousticAndConcrete,
    },
    Station {
        code: "expr",
        description: "RadCap Experimental - Experimental music",
        url: "http://79.111.119.111:8004/experimentalmusic",
        kind: StationKind::Direct,
        genre: Genre::ExperimentalAndAvantGarde,
    },
    Station {
        code: "impr",
        description: "RadCap Free Improvisation - Avant-garde jazz improvisation",
        url: "http://79.111.119.111:8000/avantgardejazz",
        kind: StationKind::Direct,
        genre: Genre::ExperimentalAndAvantGarde,
    },
    Station {
        code: "cavt",
        description: "RadCap Classical Avant-garde - Modernist and avant-garde classical",
        url: "http://213.141.131.10:8004/classavant",
        kind: StationKind::Direct,
        genre: Genre::ExperimentalAndAvantGarde,
    },
    Station {
        code: "mini",
        description: "RadCap Minimalism - Minimalist and reductionist composition",
        url: "http://213.141.131.10:8000/minimalism",
        kind: StationKind::Direct,
        genre: Genre::MinimalAndReductionist,
    },
    Station {
        code: "dron",
        description: "SomaFM Drone Zone - Atmospheric drone and ambient textures",
        url: "https://ice1.somafm.com/dronezone-128-mp3",
        kind: StationKind::Direct,
        genre: Genre::Drone,
    },
    Station {
        code: "rdrn",
        description: "RadCap Drone Ambient - Deep drone and ambient",
        url: "http://79.111.119.111:8000/droneambient",
        kind: StationKind::Direct,
        genre: Genre::Drone,
    },
    Station {
        code: "dark",
        description: "SomaFM Dark Zone - Dark ambient and atmospheric soundscapes",
        url: "https://ice1.somafm.com/darkzone-128-mp3",
        kind: StationKind::Direct,
        genre: Genre::DarkAmbient,
    },
    Station {
        code: "ritl",
        description: "RadCap Dark/Ritual Ambient - Ritual and dark ambient",
        url: "http://79.120.39.202:8000/darkambient",
        kind: StationKind::Direct,
        genre: Genre::DarkAmbient,
    },
    Station {
        code: "dsp1",
        description: "SomaFM Deep Space One - Deep ambient space music",
        url: "https://ice1.somafm.com/deepspaceone-128-mp3",
        kind: StationKind::Direct,
        genre: Genre::SpaceAndAmbient,
    },
    Station {
        code: "slow",
        description: "NTS Slow Focus - Slow, focused experimental music",
        url: "https://stream-mixtape-geo.ntslive.net/mixtape",
        kind: StationKind::Direct,
        genre: Genre::SpaceAndAmbient,
    },
    Station {
        code: "fiel",
        description: "NTS Field Recordings - Field recording mixtape",
        url: "https://stream-mixtape-geo.ntslive.net/mixtape23",
        kind: StationKind::Direct,
        genre: Genre::FieldRecordings,
    },
    Station {
        code: "pond",
        description: "Wave Farm Pond Station - Live hydrophone stream from Wave Farm",
        url: "https://audio.wavefarm.org/pondstation.mp3",
        kind: StationKind::Direct,
        genre: Genre::FieldRecordings,
    },
    Station {
        code: "n5md",
        description: "SomaFM n5MD Radio - Experimental electronic and IDM",
        url: "https://ice1.somafm.com/n5md-128-mp3",
        kind: StationKind::Direct,
        genre: Genre::ExperimentalElectronic,
    },
    Station {
        code: "arab",
        description: "Arab Mix FM - Arabic music stream replacement for Radio Alhara",
        url: "https://stream.zeno.fm/na3vpvn10qruv.acc",
        kind: StationKind::Direct,
        genre: Genre::World,
    },
    Station {
        code: "ytlf",
        description: "Lofi Girl - current featured live stream",
        url: "https://www.youtube.com/@LofiGirl/live",
        kind: StationKind::YouTube,
        genre: Genre::LoFi,
    },
    Station {
        code: "aphx",
        description: "Aphex Twin album playlist",
        url: "playlist:aphx",
        kind: StationKind::YouTubePlaylist,
        genre: Genre::ExperimentalElectronic,
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
    Genre::ALL
        .iter()
        .filter_map(|genre| {
            let rows: Vec<_> = all()
                .iter()
                .filter(|station| station.genre == *genre)
                .map(|station| format!("  {:<4}  {}", station.code, station.description))
                .collect();
            (!rows.is_empty()).then(|| format!("{}\n{}", genre.label(), rows.join("\n")))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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
    fn retained_stations_keep_their_genres() {
        assert_eq!(find("arab").unwrap().genre, Genre::World);
        assert_eq!(find("ytlf").unwrap().genre, Genre::LoFi);
        assert_eq!(find("aphx").unwrap().genre, Genre::ExperimentalElectronic);
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
    fn listing_groups_stations_under_genre_headings() {
        let listing = format_listing();

        assert!(listing.starts_with("Electroacoustic and concrète\n  conc  "));
        assert!(listing.contains("\n\nExperimental and avant-garde\n  expr  "));
        assert!(listing.contains("\n\nMinimal and reductionist\n  mini  "));
        assert!(listing.contains("\n\nDrone\n  dron  "));
        assert!(listing.contains("\n\nDark ambient\n  dark  "));
        assert!(listing.contains("\n\nSpace and ambient\n  dsp1  "));
        assert!(listing.contains("\n\nField recordings\n  fiel  "));
        assert!(listing.contains("\n\nExperimental electronic\n  n5md  "));
        assert!(listing.contains("\n\nWorld\n  arab  "));
        assert!(listing.ends_with("\n\nLo-fi\n  ytlf  Lofi Girl - current featured live stream"));
    }

    #[test]
    fn listing_orders_genre_headings_as_declared() {
        let listing = format_listing();
        let headers: Vec<_> = listing
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with("  "))
            .collect();

        assert_eq!(
            headers,
            [
                "Electroacoustic and concrète",
                "Experimental and avant-garde",
                "Minimal and reductionist",
                "Drone",
                "Dark ambient",
                "Space and ambient",
                "Field recordings",
                "Experimental electronic",
                "World",
                "Lo-fi",
            ]
        );
    }

    #[test]
    fn listing_shows_every_station_exactly_once() {
        let listing = format_listing();
        let rows: Vec<_> = listing
            .lines()
            .filter(|line| line.starts_with("  "))
            .collect();

        assert_eq!(rows.len(), all().len());
        for station in all() {
            let expected = format!("  {:<4}  {}", station.code, station.description);
            assert_eq!(
                rows.iter().filter(|row| **row == expected.as_str()).count(),
                1,
                "station {} should appear once",
                station.code
            );
        }
    }
}
