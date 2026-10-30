mod apps;
mod artifact;
mod backup;
mod cli;
mod env;
mod ffmpeg;
mod font;
mod git_id;
mod github;
mod logging;
mod paths;
mod private_file;
mod radio;
mod repos;
mod tools;
mod vol;
mod yt;

use anyhow::Result;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();
    if let Commands::GitCredential {
        route_id,
        operation,
    } = &cli.command
    {
        if let Err(error) = git_id::run_credential_helper(route_id, operation) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = run(cli) {
        eprintln!("{error:#}");
        tracing::error!(error = ?error, "command failed");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let _log_guard = logging::init()?;
    match cli.command {
        Commands::Completions { shell } => {
            print!("{}", Cli::completion_script(shell.into()));
            Ok(())
        }
        Commands::RadioPlaylistRunner { code } => radio::run_playlist_runner(code),
        Commands::RadioDirectRunner { code } => radio::run_direct_runner(code),
        Commands::GitCredential { .. } => unreachable!("credential helper handled before logging"),
        Commands::Backup { command } => backup::run(command),
        Commands::Radio(args) => radio::run(args),
        Commands::Repos { command } => repos::run(command),
        Commands::Env { command } => env::run(command),
        Commands::GitId { command } => git_id::run(command),
        Commands::Tools { command } => tools::run(command),
        Commands::Apps { command } => apps::run(command),
        Commands::Yt { command } => yt::run(command),
        Commands::Font { command } => font::run(command),
        Commands::Vol { volume } => vol::run(vol::VolArgs { volume }),
    }
}
