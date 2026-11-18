mod complete;

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
    #[usage(name = "__completions", hide, effect = "read")]
    Completions {
        #[usage(value_enum)]
        shell: CompletionShell,
    },
    /// Internal radio station loop runner.
    #[usage(name = "__radio_direct_runner", hide, effect = "write")]
    RadioDirectRunner { code: String },
    /// Internal status tray, shown while `lum display off` or `lum net on` is in force.
    #[usage(name = "__status_tray", hide, effect = "read")]
    StatusTray,
    /// Internal Git credential helper.
    #[usage(name = "__git_credential", hide, effect = "read")]
    GitCredential { route_id: String, operation: String },
    /// Listen to internet radio stations.
    Radio {
        #[usage(subcommand)]
        command: RadioCommand,
    },
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
    /// Show, set or reset the system volume.
    Vol {
        #[usage(subcommand)]
        command: VolCommand,
    },
    /// Switch shaping of the wired connection on and off. (Linux and macOS)
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

#[derive(Debug, usage_rs::Subcommands)]
pub enum RadioCommand {
    /// List the stations.
    #[usage(effect = "read")]
    List,
    /// Play a station in the background, in place of the one playing.
    #[usage(effect = "write")]
    Play {
        /// Station code, as `lum radio list` shows it.
        #[usage(complete = complete::stations)]
        station: String,
    },
    /// Play a random station, never the one playing.
    #[usage(effect = "write")]
    Random,
    /// Show whether a station is playing, paused or stopped.
    #[usage(effect = "read")]
    Status,
    /// Stop playback and remove the tray icon.
    #[usage(effect = "write")]
    Stop,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum BackupCommand {
    /// Archive a target, upload it and print the code that restores it.
    #[usage(effect = "write")]
    Create {
        /// What to back up.
        #[usage(value_enum)]
        target: BackupTargetName,
    },
    /// Download a backup and put it in place of the target, keeping the old one aside.
    #[usage(effect = "destructive")]
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
    #[usage(effect = "read")]
    Init {
        /// Shell to write code for; bash and zsh get the same code. Defaults to powershell on Windows and bash elsewhere.
        #[usage(long, value_enum)]
        shell: Option<EnvShell>,
    },
    /// Set a managed environment variable alias.
    #[usage(effect = "write")]
    Set {
        /// Shell to write code for; bash and zsh get the same code. Defaults to powershell on Windows and bash elsewhere.
        #[usage(long, value_enum)]
        shell: Option<EnvShell>,
        /// Alias of the variable, as `lum env list` shows it.
        #[usage(complete = complete::env_aliases)]
        alias: String,
        /// Value to store and export.
        value: String,
    },
    /// Unset a managed environment variable alias.
    #[usage(effect = "destructive")]
    Unset {
        /// Shell to write code for; bash and zsh get the same code. Defaults to powershell on Windows and bash elsewhere.
        #[usage(long, value_enum)]
        shell: Option<EnvShell>,
        /// Alias of the variable, as `lum env list` shows it.
        #[usage(complete = complete::env_aliases)]
        alias: String,
    },
    /// Show managed aliases and forced defaults.
    #[usage(effect = "read")]
    List,
    /// Print the directory lum installs tools into, which `init` puts on PATH.
    #[usage(effect = "read")]
    BinDir,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum ToolsCommand {
    /// Install a managed tool.
    #[usage(effect = "write")]
    Install {
        /// Tool name, as `lum tools list` shows it.
        #[usage(complete = complete::tools)]
        tool: String,
        /// Overwrite a binary lum does not manage, or reinstall one it does.
        #[usage(long)]
        force: bool,
    },
    /// List managed tools and local state.
    #[usage(effect = "read")]
    List,
    /// Show install details and the latest version for one tool.
    #[usage(effect = "read")]
    Status {
        /// Tool name, as `lum tools list` shows it.
        #[usage(complete = complete::tools)]
        tool: String,
    },
    /// Install missing tools and update outdated tools.
    #[usage(effect = "write")]
    Sync {
        #[usage(long)]
        dry_run: bool,
    },
    /// Update one managed tool.
    #[usage(effect = "write")]
    Update {
        /// Tool name, as `lum tools list` shows it.
        #[usage(complete = complete::tools)]
        tool: String,
        /// Reinstall even when the installed version is current.
        #[usage(long)]
        force: bool,
    },
    /// Remove a tool lum installed.
    #[usage(effect = "destructive")]
    Uninstall {
        /// Tool name, as `lum tools list` shows it.
        #[usage(complete = complete::tools)]
        tool: String,
    },
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum AppsCommand {
    /// Install an app and apply its settings.
    #[usage(effect = "write")]
    Install {
        /// App name, as `lum apps list` shows it.
        #[usage(complete = complete::apps)]
        app: String,
        /// Replace an app lum did not install, or reinstall one it did.
        #[usage(long)]
        force: bool,
    },
    /// List apps, their install state and settings.
    #[usage(effect = "read")]
    List,
    /// Show install details, the latest version and settings for one app.
    #[usage(effect = "read")]
    Status {
        /// App name, as `lum apps list` shows it.
        #[usage(complete = complete::apps)]
        app: String,
    },
    /// Update one app and reapply its settings.
    #[usage(effect = "write")]
    Update {
        /// App name, as `lum apps list` shows it.
        #[usage(complete = complete::apps)]
        app: String,
        /// Reinstall even when the installed version is current.
        #[usage(long)]
        force: bool,
    },
    /// Install missing apps, update outdated ones and reapply drifted settings.
    #[usage(effect = "write")]
    Sync {
        #[usage(long)]
        dry_run: bool,
    },
    /// Remove an app and its launcher; its settings and saves stay.
    #[usage(effect = "destructive")]
    Uninstall {
        /// App name, as `lum apps list` shows it.
        #[usage(complete = complete::apps)]
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
    #[usage(effect = "read")]
    ConfigPath,
    /// Create a sample git identity config file if none exists.
    #[usage(effect = "write")]
    Init,
    /// Synchronize the machine with the git identity config.
    #[usage(effect = "write")]
    Sync,
    /// Show status for all configured git identities.
    #[usage(effect = "read")]
    Status,
    /// Show which git identity applies to the current directory.
    #[usage(effect = "read")]
    Where,
    /// Show detailed information about one git identity.
    #[usage(effect = "read")]
    Info {
        /// Identity name from the git-id config.
        #[usage(complete = complete::identities)]
        identity: String,
    },
    /// Print an identity public key to stdout.
    #[usage(effect = "read")]
    Pubkey {
        /// Identity name from the git-id config.
        #[usage(complete = complete::identities)]
        identity: String,
    },
    /// Show files and folders managed by git-id.
    #[usage(effect = "read")]
    Files,
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum MirrorCommand {
    /// Print the path to the mirror config file.
    #[usage(effect = "read")]
    ConfigPath,
    /// Print the path to the mirror directory.
    #[usage(effect = "read")]
    Dir,
    /// Create a sample mirror config file if none exists.
    #[usage(effect = "write")]
    Init,
    /// List configured mirror repositories.
    #[usage(effect = "read")]
    List,
    /// Clone or update all configured mirror repositories.
    #[usage(effect = "write")]
    Sync {
        /// Maximum concurrent git operations.
        #[usage(short = 'j', long, default = "4")]
        jobs: usize,
    },
    /// Check if local mirrors are up to date with their remotes.
    #[usage(effect = "read")]
    Status {
        /// Maximum concurrent git operations.
        #[usage(short = 'j', long, default = "4")]
        jobs: usize,
        /// Compare against cached remote refs instead of contacting remotes.
        #[usage(long)]
        offline: bool,
    },
    /// Watch mirror repos for upstream changes and send desktop notifications.
    #[usage(effect = "read")]
    Watch {
        /// Tag to filter repos by. Omit to see guidance.
        #[usage(complete = complete::mirror_tags)]
        tag: Option<String>,
        /// Number of poll cycles to run. Test/support plumbing; omit for infinite (Ctrl+C to stop).
        #[usage(long, hide)]
        cycles: Option<usize>,
    },
}
#[derive(Debug, usage_rs::Subcommands)]
pub enum YtCommand {
    /// Download audio from YouTube URL(s).
    #[usage(effect = "write")]
    Audio {
        /// YouTube URL(s) to download.
        #[usage(required = true)]
        urls: Vec<String>,
    },
    /// Download video from YouTube URL(s).
    #[usage(effect = "write")]
    Video {
        /// Maximum video height (default: 1080).
        #[usage(long)]
        height: Option<u32>,
        /// YouTube URL(s) to download.
        #[usage(required = true)]
        urls: Vec<String>,
    },
    /// Download an album or playlist from YouTube URL(s).
    #[usage(effect = "write")]
    Album {
        /// YouTube URL(s) to download.
        #[usage(required = true)]
        urls: Vec<String>,
    },
    /// Print channel RSS feed URL(s) for YouTube video or channel URL(s).
    #[usage(effect = "read")]
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
    #[usage(effect = "write")]
    On {
        /// Wired interface to use instead of detecting it.
        #[usage(long, complete = complete::wired_interfaces)]
        iface: Option<String>,
        /// Command to run while shaping is on.
        #[usage(
            value_name = "COMMAND",
            double_dash = "automatic",
            value_hint = usage_rs::ValueHint::CommandWithArguments
        )]
        command: Vec<String>,
    },
    /// Restore automatic link speed and remove download shaping.
    #[usage(effect = "write")]
    Off,
    /// Show whether shaping is on, and the wired interfaces.
    #[usage(effect = "read")]
    Status,
    /// Measure latency idle and under load, and count receive errors.
    #[usage(effect = "read")]
    Test {
        /// Wired interface to use instead of detecting it.
        #[usage(long, complete = complete::wired_interfaces)]
        iface: Option<String>,
        /// Seconds to load the link (5–120).
        #[usage(long, default = "20")]
        seconds: u64,
    },
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum FontCommand {
    /// List managed fonts and local state.
    #[usage(effect = "read")]
    List,
    /// Install a managed font.
    #[usage(effect = "write")]
    Install {
        /// Font name, as `lum font list` shows it.
        #[usage(complete = complete::fonts)]
        font: String,
        /// Reinstall a font that is already installed.
        #[usage(long)]
        force: bool,
    },
    /// Show where a font installs and whether it is installed.
    #[usage(effect = "read")]
    Status {
        /// Font name, as `lum font list` shows it.
        #[usage(complete = complete::fonts)]
        font: String,
    },
    /// Uninstall a managed font.
    #[usage(effect = "destructive")]
    Uninstall {
        /// Font name, as `lum font list` shows it.
        #[usage(complete = complete::fonts)]
        font: String,
    },
}

#[derive(Debug, usage_rs::Subcommands)]
pub enum VolCommand {
    /// Set the volume back to lum's default: 17 on macOS, 25 elsewhere.
    #[usage(effect = "write")]
    Reset,
    /// Set the volume to a level from 0 to 100.
    #[usage(effect = "write")]
    Set {
        /// Volume level, 0–100.
        level: u16,
    },
    /// Show the current volume.
    #[usage(effect = "read")]
    Status,
}

// Only the macOS build reads the command; elsewhere `lum display` refuses to run.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, usage_rs::Subcommands)]
pub enum DisplayCommand {
    /// Sharp HiDPI at about the monitor's own size.
    #[usage(effect = "write")]
    On,
    /// The monitor's plain resolution until you restart or log out: least GPU work, for games.
    #[usage(effect = "write")]
    Off,
    /// The sRGB color profile, for monitors still on the one macOS made for them. Asks first.
    #[usage(effect = "write")]
    Srgb,
    /// Show whether HiDPI is on for each monitor.
    #[usage(effect = "read")]
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
    fn parses_radio_play() {
        let cli = parse(&["radio", "play", "dron"]);
        match cli.command {
            Commands::Radio {
                command: RadioCommand::Play { station },
            } => assert_eq!(station, "dron"),
            _ => panic!("expected radio play"),
        }
    }

    #[test]
    fn parses_radio_random() {
        let cli = parse(&["radio", "random"]);
        assert!(matches!(
            cli.command,
            Commands::Radio {
                command: RadioCommand::Random
            }
        ));
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
    fn parses_net_on_with_a_forwarded_command() {
        let cli = parse(&[
            "net",
            "on",
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
                command: NetCommand::On { iface, command },
            } => {
                assert_eq!(iface.as_deref(), Some("enp0s31f6"));
                assert_eq!(
                    command,
                    ["flatpak", "run", "--branch=master", "com.nvidia.geforcenow"]
                );
            }
            _ => panic!("expected net on"),
        }
    }

    #[test]
    fn parses_net_on_forwarding_without_a_double_dash() {
        let cli = parse(&["net", "on", "open", "-W", "-a", "GeForce NOW"]);
        match cli.command {
            Commands::Net {
                command: NetCommand::On { iface, command },
            } => {
                assert_eq!(iface, None);
                assert_eq!(command, ["open", "-W", "-a", "GeForce NOW"]);
            }
            _ => panic!("expected net on"),
        }
    }

    #[test]
    fn parses_bare_net_on() {
        let cli = parse(&["net", "on"]);
        assert!(matches!(
            cli.command,
            Commands::Net {
                command: NetCommand::On { iface: None, ref command }
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
