use crate::repos::scanner::ScanArgs;

/// Full version string including commit hash and build timestamp.
const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("LUM_GIT_COMMIT_HASH_SHORT"),
    ") built ",
    env!("LUM_BUILD_TIME_UTC")
);

#[derive(Debug, usage_rs::Cli)]
#[usage(
    bin = "lum",
    version = LONG_VERSION,
    about = "Opinionated CLI toolbox",
    arg_required_else_help,
    completion
)]
pub struct Cli {
    #[usage(subcommand)]
    pub command: Commands,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum Commands {
    /// Generate shell completions.
    #[usage(name = "__completions", hide)]
    Completions {
        #[usage(value_enum)]
        shell: CompletionShell,
    },
    /// Internal radio playlist loop runner.
    #[usage(name = "__radio_playlist_runner", hide)]
    RadioPlaylistRunner { code: String },
    /// Internal radio station loop runner.
    #[usage(name = "__radio_direct_runner", hide)]
    RadioDirectRunner { code: String },
    /// Internal display tray, shown while `lum display off` is in force.
    #[usage(name = "__display_tray", hide)]
    DisplayTray,
    /// Internal Git credential helper.
    #[usage(name = "__git_credential", hide)]
    GitCredential { route_id: String, operation: String },
    /// Listen to internet radio stations.
    Radio(RadioArgs),
    /// Backup and restore directories.
    Backup {
        #[usage(subcommand)]
        command: BackupCommand,
    },
    /// Manage shell environment variables and lum's bin path.
    Env {
        #[usage(subcommand)]
        command: EnvCommand,
    },
    /// Scan directory trees for Git repositories and report status.
    Repos {
        #[usage(subcommand)]
        command: ReposCommand,
    },
    /// Manage folder-based Git identities.
    #[usage(name = "git-id")]
    GitId {
        #[usage(subcommand)]
        command: GitIdCommand,
    },
    /// Manage curated developer tools installed into lum's bin path.
    Tools {
        #[usage(subcommand)]
        command: ToolsCommand,
    },
    /// Install desktop apps per-user and keep their settings.
    Apps {
        #[usage(subcommand)]
        command: AppsCommand,
    },
    /// Download audio, video, or albums from YouTube using yt-dlp.
    Yt {
        #[usage(subcommand)]
        command: YtCommand,
    },
    /// Install and manage fonts.
    Font {
        #[usage(subcommand)]
        command: FontCommand,
    },
    /// Set system volume to default or specified level.
    Vol {
        /// Volume level (0–100). Omit to reset to OS default.
        volume: Option<u16>,
    },
    /// Measure the wired connection. (Linux and macOS)
    Net {
        #[usage(subcommand)]
        command: NetCommand,
    },
    /// Sharp HiDPI text on monitors that aren't Retina, or off for games. (macOS)
    Display {
        /// Omit to show whether HiDPI is on.
        #[usage(value_enum)]
        state: Option<HidpiState>,
    },
}

/// Shells accepted by the hidden `__completions` command.
#[derive(Debug, Clone, Copy, usage_rs::ValueEnum)]
pub enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
}

impl From<CompletionShell> for usage_rs::complete::Shell {
    fn from(shell: CompletionShell) -> Self {
        match shell {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Elvish => Self::Elvish,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::Powershell => Self::PowerShell,
            CompletionShell::Zsh => Self::Zsh,
        }
    }
}

#[derive(Debug, usage_rs::Args, Clone)]
#[usage(
    after_help = "Commands:\n  lum radio                 List stations\n  lum radio <code>          Play a station (example: lum radio dron)\n  lum radio rand            Play a random station, never the current one\n  lum radio status          Show current playback state\n  lum radio stop            Stop playback and clear state"
)]
pub struct RadioArgs {
    /// Command (list|status|stop|rand) or station code.
    ///
    /// Omit to list stations and common playback commands.
    pub arg: Option<String>,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum BackupCommand {
    /// Backup and restore ~/.bio.
    Bio { code: Option<String> },
    /// Backup and restore ~/ROMs: ROMs and the saves mGBA keeps beside them.
    Roms { code: Option<String> },
}

#[derive(Debug, Clone, Copy, usage_rs::ValueEnum)]
pub enum EnvShell {
    Posix,
    Fish,
    Powershell,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum EnvCommand {
    /// Print shell integration code for eval in shell startup.
    Init {
        #[usage(long, value_enum)]
        shell: Option<EnvShell>,
    },
    /// Set a managed environment variable alias.
    Set {
        #[usage(long, value_enum)]
        shell: Option<EnvShell>,
        alias: String,
        value: String,
    },
    /// Unset a managed environment variable alias.
    Unset {
        #[usage(long, value_enum)]
        shell: Option<EnvShell>,
        alias: String,
    },
    /// Show managed aliases and forced defaults.
    #[usage(visible_alias = "ls")]
    List,
    /// Show alias to environment variable mappings.
    Aliases,
    /// Print lum's environment bin directory.
    Path,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum ToolsCommand {
    /// Install a managed tool.
    Install {
        tool: String,
        #[usage(long)]
        force: bool,
    },
    /// List managed tools and local state.
    #[usage(visible_alias = "ls")]
    List,
    /// Show detailed status for one tool.
    Status { tool: String },
    /// Install missing tools and update outdated tools.
    Sync {
        #[usage(long)]
        dry_run: bool,
    },
    /// Update one managed tool.
    Update {
        tool: String,
        #[usage(long)]
        force: bool,
    },
    /// Show installed and latest version for one tool.
    Version { tool: String },
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum AppsCommand {
    /// Install an app and apply its settings.
    Install {
        app: String,
        #[usage(long)]
        force: bool,
    },
    /// List apps, their install state and settings.
    #[usage(visible_alias = "ls")]
    List,
    /// Show install details and settings for one app.
    Status { app: String },
    /// Update one app and reapply its settings.
    Update {
        app: String,
        #[usage(long)]
        force: bool,
    },
    /// Install missing apps, update outdated ones and reapply drifted settings.
    Sync {
        #[usage(long)]
        dry_run: bool,
    },
    /// Show installed and latest version for one app.
    Version { app: String },
    /// Remove an app and its launcher; its settings and saves stay.
    Uninstall { app: String },
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum ReposCommand {
    /// Scan a directory tree for Git repositories and report branch and sync status.
    Scan(ScanArgs),
    /// Clone, update, and inspect configured mirror repositories.
    Mirror {
        #[usage(subcommand)]
        command: MirrorCommand,
    },
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum GitIdCommand {
    /// Print the path to the git identity config file.
    ConfigPath,
    /// Create a sample git identity config file if none exists.
    Init,
    /// Synchronize the machine with the git identity config.
    Sync,
    /// Show status for all configured git identities.
    Status,
    /// Show which git identity applies to the current directory.
    Where,
    /// Show detailed information about one git identity.
    Info { identity: String },
    /// Print an identity public key to stdout.
    Pubkey { identity: String },
    /// Show files and folders managed by git-id.
    Paths,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum MirrorCommand {
    /// Print the path to the mirror config file.
    ConfigPath,
    /// Print the path to the mirror directory.
    Dir,
    /// Create a sample mirror config file if none exists.
    Init,
    /// List configured mirror repositories.
    #[usage(visible_alias = "ls")]
    List,
    /// Clone or update all configured mirror repositories.
    Sync {
        /// Maximum concurrent git operations.
        #[usage(short = 'j', default = "4")]
        jobs: usize,
    },
    /// Check if local mirrors are up to date with their remotes.
    Status {
        /// Maximum concurrent git operations.
        #[usage(short = 'j', default = "4")]
        jobs: usize,
        /// Compare against cached remote refs instead of contacting remotes.
        #[usage(long)]
        offline: bool,
    },
    /// Watch mirror repos for upstream changes and send desktop notifications.
    Watch {
        /// Tag to filter repos by. Omit to see guidance.
        tag: Option<String>,
        /// Number of poll cycles to run. Test/support plumbing; omit for infinite (Ctrl+C to stop).
        #[usage(long, hide)]
        cycles: Option<usize>,
    },
}
#[derive(Debug, usage_rs::Subcommands)]
pub enum YtCommand {
    /// Download audio from YouTube URL(s).
    Aud {
        /// YouTube URL(s) to download.
        #[usage(required = true)]
        urls: Vec<String>,
    },
    /// Download video from YouTube URL(s).
    Vid {
        /// Maximum video height (default: 1080).
        #[usage(long)]
        height: Option<u32>,
        /// YouTube URL(s) to download.
        #[usage(required = true)]
        urls: Vec<String>,
    },
    /// Download an album or playlist from YouTube URL(s).
    Alb {
        /// YouTube URL(s) to download.
        #[usage(required = true)]
        urls: Vec<String>,
    },
    /// Print channel RSS feed URL(s) for YouTube video or channel URL(s).
    Rss {
        /// Print bare ID(s) instead of feed URL(s).
        ///
        /// With --videos-only, prints the playlist ID instead.
        #[usage(long)]
        id_only: bool,
        /// Print the long-form-videos-only feed (excludes shorts and live).
        #[usage(long)]
        videos_only: bool,
        /// YouTube video or channel URL(s). Playlists rejected.
        #[usage(required = true)]
        urls: Vec<String>,
    },
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum NetCommand {
    /// Measure latency idle and under load, and count receive errors.
    Test {
        /// Wired interface to use instead of detecting it.
        #[usage(long)]
        iface: Option<String>,
        /// Seconds to load the link (5–120).
        #[usage(long, default = "20")]
        seconds: u64,
    },
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum FontCommand {
    /// List managed fonts and local state.
    #[usage(visible_alias = "ls")]
    List,
    /// Install a managed font.
    Install {
        font: String,
        #[usage(long)]
        force: bool,
    },
    /// Uninstall a managed font.
    Uninstall { font: String },
}

// Only the macOS build reads the state; elsewhere `lum display` refuses to run.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, usage_rs::ValueEnum)]
pub enum HidpiState {
    /// Sharp HiDPI at about the monitor's own size.
    On,
    /// The monitor's plain resolution until you restart or log out: least GPU work, for games.
    Off,
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn parse(args: &[&str]) -> Cli {
        let argv: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        Cli::parse_from(&argv).unwrap()
    }

    #[test]
    fn parses_radio_without_station() {
        let cli = parse(&["radio"]);
        match cli.command {
            Commands::Radio(args) => assert_eq!(args.arg, None),
            Commands::Backup { .. }
            | Commands::Env { .. }
            | Commands::Completions { .. }
            | Commands::RadioPlaylistRunner { .. }
            | Commands::RadioDirectRunner { .. }
            | Commands::GitCredential { .. }
            | Commands::Tools { .. }
            | Commands::Apps { .. }
            | Commands::Repos { .. }
            | Commands::GitId { .. }
            | Commands::Yt { .. }
            | Commands::Font { .. }
            | Commands::Vol { .. }
            | Commands::Net { .. }
            | Commands::Display { .. }
            | Commands::DisplayTray => {
                panic!("expected radio command")
            }
        }
    }

    #[test]
    fn parses_radio_with_station() {
        let cli = parse(&["radio", "dron"]);
        match cli.command {
            Commands::Radio(args) => assert_eq!(args.arg.as_deref(), Some("dron")),
            Commands::Backup { .. }
            | Commands::Env { .. }
            | Commands::Completions { .. }
            | Commands::RadioPlaylistRunner { .. }
            | Commands::RadioDirectRunner { .. }
            | Commands::GitCredential { .. }
            | Commands::Tools { .. }
            | Commands::Apps { .. }
            | Commands::Repos { .. }
            | Commands::GitId { .. }
            | Commands::Yt { .. }
            | Commands::Font { .. }
            | Commands::Vol { .. }
            | Commands::Net { .. }
            | Commands::Display { .. }
            | Commands::DisplayTray => {
                panic!("expected radio command")
            }
        }
    }

    #[test]
    fn parses_apps_ls() {
        let cli = parse(&["apps", "ls"]);
        assert!(matches!(
            cli.command,
            Commands::Apps {
                command: AppsCommand::List
            }
        ));
    }

    #[test]
    fn parses_net_test_seconds_with_a_default() {
        let cli = parse(&["net", "test"]);
        assert!(matches!(
            cli.command,
            Commands::Net {
                command: NetCommand::Test {
                    iface: None,
                    seconds: 20
                }
            }
        ));
    }

    #[test]
    fn parses_display_on() {
        let cli = parse(&["display", "on"]);
        assert!(matches!(
            cli.command,
            Commands::Display {
                state: Some(HidpiState::On)
            }
        ));
    }

    #[test]
    fn parses_display_without_a_state() {
        let cli = parse(&["display"]);
        assert!(matches!(cli.command, Commands::Display { state: None }));
    }

    #[test]
    fn spec_is_valid() {
        let spec: usage_parser::Spec = Cli::to_kdl().parse().unwrap();
        let _ = spec;
    }

    #[test]
    fn parses_hidden_radio_playlist_runner() {
        let cli = parse(&["__radio_playlist_runner", "aphx"]);
        match cli.command {
            Commands::RadioPlaylistRunner { code } => assert_eq!(code, "aphx"),
            _ => panic!("expected hidden radio playlist runner command"),
        }
    }
}
