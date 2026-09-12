use crate::repos::scanner::ScanArgs;

/// Full version string including commit hash and build timestamp.
const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("LUM_GIT_COMMIT_HASH_SHORT"),
    ") built ",
    env!("LUM_BUILD_TIME_UTC")
);

#[derive(Debug, usage::Cli)]
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

#[derive(Debug, usage::Subcommands)]
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
    /// Manage macOS applications installed into a per-user Applications folder. (macOS only)
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
}

/// Shells accepted by the hidden `__completions` command.
#[derive(Debug, Clone, Copy, usage::ValueEnum)]
pub enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
}

impl From<CompletionShell> for usage::complete::Shell {
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

#[derive(Debug, usage::Args, Clone)]
#[usage(
    after_help = "Commands:\n  lum radio                 List stations\n  lum radio <code>          Play a station (example: lum radio atma)\n  lum radio status          Show current playback state\n  lum radio stop            Stop playback and clear state"
)]
pub struct RadioArgs {
    /// Command (status|stop|list) or station code.
    ///
    /// Omit to list stations and common playback commands.
    pub arg: Option<String>,
}

#[derive(Debug, usage::Subcommands)]
pub enum BackupCommand {
    /// Backup and restore ~/.bio.
    Bio { code: Option<String> },
    /// Backup and restore OpenEmu data.
    Openemu { code: Option<String> },
}

#[derive(Debug, Clone, Copy, usage::ValueEnum)]
pub enum EnvShell {
    Posix,
    Fish,
    Powershell,
}

#[derive(Debug, usage::Subcommands)]
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

#[derive(Debug, usage::Subcommands)]
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

#[derive(Debug, usage::Subcommands)]
pub enum AppsCommand {
    /// Install a managed macOS app.
    Install {
        app: String,
        #[usage(long)]
        force: bool,
    },
    /// List managed macOS apps and local state.
    #[usage(visible_alias = "ls")]
    List,
    /// Show detailed status for one managed app.
    Status { app: String },
    /// Update one managed macOS app.
    Update {
        app: String,
        #[usage(long)]
        force: bool,
    },
    /// Install missing, update outdated, and list up-to-date apps.
    Sync {
        #[usage(long)]
        dry_run: bool,
    },
    /// Show installed and latest version for one app.
    Version { app: String },
}

#[derive(Debug, usage::Subcommands)]
pub enum ReposCommand {
    /// Scan a directory tree for Git repositories and report branch and sync status.
    Scan(ScanArgs),
    /// Clone, update, and inspect configured mirror repositories.
    Mirror {
        #[usage(subcommand)]
        command: MirrorCommand,
    },
}

#[derive(Debug, usage::Subcommands)]
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

#[derive(Debug, usage::Subcommands)]
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
#[derive(Debug, usage::Subcommands)]
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

#[derive(Debug, usage::Subcommands)]
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
            | Commands::GitCredential { .. }
            | Commands::Tools { .. }
            | Commands::Apps { .. }
            | Commands::Repos { .. }
            | Commands::GitId { .. }
            | Commands::Yt { .. }
            | Commands::Font { .. }
            | Commands::Vol { .. } => {
                panic!("expected radio command")
            }
        }
    }

    #[test]
    fn parses_radio_with_station() {
        let cli = parse(&["radio", "atma"]);
        match cli.command {
            Commands::Radio(args) => assert_eq!(args.arg.as_deref(), Some("atma")),
            Commands::Backup { .. }
            | Commands::Env { .. }
            | Commands::Completions { .. }
            | Commands::RadioPlaylistRunner { .. }
            | Commands::GitCredential { .. }
            | Commands::Tools { .. }
            | Commands::Apps { .. }
            | Commands::Repos { .. }
            | Commands::GitId { .. }
            | Commands::Yt { .. }
            | Commands::Font { .. }
            | Commands::Vol { .. } => {
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
