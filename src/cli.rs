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
    completion,
    unknown_flags = "error"
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
    /// Internal radio station loop runner.
    #[usage(name = "__radio_direct_runner", hide)]
    RadioDirectRunner { code: String },
    /// Internal status tray, shown while `lum display off` or `lum net game` is in force.
    #[usage(name = "__status_tray", hide)]
    StatusTray,
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
    /// Switch the wired connection between normal and game mode. (Linux and macOS)
    Net {
        #[usage(subcommand)]
        command: NetCommand,
    },
    /// HiDPI on and off, and sRGB color, for monitors that aren't Retina. (macOS)
    Display {
        #[usage(subcommand)]
        command: DisplayCommand,
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
    /// Archive a target, upload it and print the code that restores it.
    Create {
        /// What to back up.
        #[usage(value_enum)]
        target: BackupTargetName,
    },
    /// Download a backup and put it in place of the target, keeping the old one aside.
    Restore {
        /// What to restore.
        #[usage(value_enum)]
        target: BackupTargetName,
        /// Code that `lum backup create` printed, such as jtPb.gz.
        code: String,
    },
}

/// What `lum backup` backs up and restores.
#[derive(Debug, Clone, Copy, usage_rs::ValueEnum)]
pub enum BackupTargetName {
    /// ~/.bio (macOS and Linux).
    Bio,
    /// ~/ROMs: ROMs and the saves mGBA keeps beside them.
    Roms,
}

#[derive(Debug, Clone, Copy, usage_rs::ValueEnum)]
pub enum EnvShell {
    /// The same POSIX code as zsh.
    Bash,
    /// The same POSIX code as bash.
    Zsh,
    Fish,
    Powershell,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum EnvCommand {
    /// Print shell integration code for eval in shell startup.
    Init {
        /// Shell to write code for; bash and zsh get the same code. Defaults to powershell on Windows and bash elsewhere.
        #[usage(long, value_enum)]
        shell: Option<EnvShell>,
    },
    /// Set a managed environment variable alias.
    Set {
        /// Shell to write code for; bash and zsh get the same code. Defaults to powershell on Windows and bash elsewhere.
        #[usage(long, value_enum)]
        shell: Option<EnvShell>,
        /// Alias of the variable, as `lum env list` shows it.
        alias: String,
        /// Value to store and export.
        value: String,
    },
    /// Unset a managed environment variable alias.
    Unset {
        /// Shell to write code for; bash and zsh get the same code. Defaults to powershell on Windows and bash elsewhere.
        #[usage(long, value_enum)]
        shell: Option<EnvShell>,
        /// Alias of the variable, as `lum env list` shows it.
        alias: String,
    },
    /// Show managed aliases and forced defaults.
    List,
    /// Print the directory lum installs tools into, which `init` puts on PATH.
    BinDir,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum ToolsCommand {
    /// Install a managed tool.
    Install {
        /// Tool name, as `lum tools list` shows it.
        tool: String,
        /// Overwrite a binary lum does not manage, or reinstall one it does.
        #[usage(long)]
        force: bool,
    },
    /// List managed tools and local state.
    List,
    /// Show install details and the latest version for one tool.
    Status {
        /// Tool name, as `lum tools list` shows it.
        tool: String,
    },
    /// Install missing tools and update outdated tools.
    Sync {
        #[usage(long)]
        dry_run: bool,
    },
    /// Update one managed tool.
    Update {
        /// Tool name, as `lum tools list` shows it.
        tool: String,
        /// Reinstall even when the installed version is current.
        #[usage(long)]
        force: bool,
    },
    /// Remove a tool lum installed.
    Uninstall {
        /// Tool name, as `lum tools list` shows it.
        tool: String,
    },
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum AppsCommand {
    /// Install an app and apply its settings.
    Install {
        /// App name, as `lum apps list` shows it.
        app: String,
        /// Replace an app lum did not install, or reinstall one it did.
        #[usage(long)]
        force: bool,
    },
    /// List apps, their install state and settings.
    List,
    /// Show install details, the latest version and settings for one app.
    Status {
        /// App name, as `lum apps list` shows it.
        app: String,
    },
    /// Update one app and reapply its settings.
    Update {
        /// App name, as `lum apps list` shows it.
        app: String,
        /// Reinstall even when the installed version is current.
        #[usage(long)]
        force: bool,
    },
    /// Install missing apps, update outdated ones and reapply drifted settings.
    Sync {
        #[usage(long)]
        dry_run: bool,
    },
    /// Remove an app and its launcher; its settings and saves stay.
    Uninstall {
        /// App name, as `lum apps list` shows it.
        app: String,
    },
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
    Info {
        /// Identity name from the git-id config.
        identity: String,
    },
    /// Print an identity public key to stdout.
    Pubkey {
        /// Identity name from the git-id config.
        identity: String,
    },
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
    List,
    /// Clone or update all configured mirror repositories.
    Sync {
        /// Maximum concurrent git operations.
        #[usage(short = 'j', long, default = "4")]
        jobs: usize,
    },
    /// Check if local mirrors are up to date with their remotes.
    Status {
        /// Maximum concurrent git operations.
        #[usage(short = 'j', long, default = "4")]
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
    /// Force 100 Mb/s and shape downloads. Given a command, run it and switch back when it exits.
    Game {
        /// Wired interface to use instead of detecting it.
        #[usage(long)]
        iface: Option<String>,
        /// Command to run in game mode.
        #[usage(
            value_name = "COMMAND",
            double_dash = "automatic",
            value_hint = usage_rs::ValueHint::CommandWithArguments
        )]
        command: Vec<String>,
    },
    /// Restore automatic link speed and remove download shaping.
    Normal,
    /// Show game mode and the wired interfaces.
    Status,
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
    List,
    /// Install a managed font.
    Install {
        /// Font name, as `lum font list` shows it.
        font: String,
        /// Reinstall a font that is already installed.
        #[usage(long)]
        force: bool,
    },
    /// Show where a font installs and whether it is installed.
    Status {
        /// Font name, as `lum font list` shows it.
        font: String,
    },
    /// Uninstall a managed font.
    Uninstall {
        /// Font name, as `lum font list` shows it.
        font: String,
    },
}

// Only the macOS build reads the command; elsewhere `lum display` refuses to run.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, usage_rs::Subcommands)]
pub enum DisplayCommand {
    /// Sharp HiDPI at about the monitor's own size.
    On,
    /// The monitor's plain resolution until you restart or log out: least GPU work, for games.
    Off,
    /// The sRGB color profile, for monitors still on the one macOS made for them. Asks first.
    Srgb,
    /// Show whether HiDPI is on for each monitor.
    Status,
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
            | Commands::StatusTray => {
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
            | Commands::StatusTray => {
                panic!("expected radio command")
            }
        }
    }

    #[test]
    fn parses_apps_list() {
        let cli = parse(&["apps", "list"]);
        assert!(matches!(
            cli.command,
            Commands::Apps {
                command: AppsCommand::List
            }
        ));
    }

    #[test]
    fn parses_net_game_with_a_forwarded_command() {
        let cli = parse(&[
            "net",
            "game",
            "--iface",
            "enp0s31f6",
            "--",
            "flatpak",
            "run",
            "--branch=master",
            "com.nvidia.geforcenow",
        ]);
        match cli.command {
            Commands::Net {
                command: NetCommand::Game { iface, command },
            } => {
                assert_eq!(iface.as_deref(), Some("enp0s31f6"));
                assert_eq!(
                    command,
                    ["flatpak", "run", "--branch=master", "com.nvidia.geforcenow"]
                );
            }
            _ => panic!("expected net game"),
        }
    }

    #[test]
    fn parses_net_game_forwarding_without_a_double_dash() {
        let cli = parse(&["net", "game", "open", "-W", "-a", "GeForce NOW"]);
        match cli.command {
            Commands::Net {
                command: NetCommand::Game { iface, command },
            } => {
                assert_eq!(iface, None);
                assert_eq!(command, ["open", "-W", "-a", "GeForce NOW"]);
            }
            _ => panic!("expected net game"),
        }
    }

    #[test]
    fn parses_bare_net_game() {
        let cli = parse(&["net", "game"]);
        assert!(matches!(
            cli.command,
            Commands::Net {
                command: NetCommand::Game { iface: None, ref command }
            } if command.is_empty()
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
                command: DisplayCommand::On
            }
        ));
    }

    #[test]
    fn parses_display_srgb() {
        let cli = parse(&["display", "srgb"]);
        assert!(matches!(
            cli.command,
            Commands::Display {
                command: DisplayCommand::Srgb
            }
        ));
    }

    #[test]
    fn parses_display_status() {
        let cli = parse(&["display", "status"]);
        assert!(matches!(
            cli.command,
            Commands::Display {
                command: DisplayCommand::Status
            }
        ));
    }

    #[test]
    fn spec_is_valid() {
        let spec: usage_parser::Spec = Cli::to_kdl().parse().unwrap();
        let _ = spec;
    }
}
