mod apps;
mod artifact;
mod backup;
mod cli;
mod display;
mod env;
mod ffmpeg;
mod fonts;
mod git_id;
mod github;
mod logging;
mod net;
mod paths;
mod private_file;
mod radio;
mod repos;
mod tools;
mod tray;
mod vol;
mod yt;

use std::io::{self, Write as _};

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
        // Not `eprintln!`, which panics when stderr is gone: `lum net on`
        // can outlive the terminal it was started from.
        let _ = writeln!(io::stderr(), "{error:#}");
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
        Commands::RadioDirectRunner { code } => radio::run_direct_runner(code),
        Commands::GitCredential { .. } => unreachable!("credential helper handled before logging"),
        Commands::Backup { command } => backup::run(command),
        Commands::Radio { command } => radio::run(command),
        Commands::Repos { command } => repos::run(command),
        Commands::Env { command } => env::run(command),
        Commands::GitId { command } => git_id::run(command),
        Commands::Tools { command } => tools::run(command),
        Commands::Apps { command } => apps::run(command),
        Commands::Yt { command } => yt::run(command),
        Commands::Fonts { command } => fonts::run(command),
        Commands::Vol { command } => vol::run(command),
        Commands::Net { command } => net::run(command),
        Commands::Display { command } => display::run(command),
        Commands::StatusTray => tray::status::run(),
    }
}
