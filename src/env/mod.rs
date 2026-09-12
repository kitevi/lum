mod catalog;
mod shell;
mod state;

use anyhow::{Context, Result};

use crate::cli::{EnvCommand, EnvShell};

pub fn run(command: EnvCommand) -> Result<()> {
    match command {
        EnvCommand::Init { shell } => init(shell.unwrap_or_default()),
        EnvCommand::Set {
            shell,
            alias,
            value,
        } => set(&alias, &value, shell.unwrap_or_default()),
        EnvCommand::Unset { shell, alias } => unset(&alias, shell.unwrap_or_default()),
        EnvCommand::Path => {
            println!("{}", state::bin_dir()?.display());
            Ok(())
        }
        EnvCommand::List => list(),
        EnvCommand::Aliases => {
            for (alias, variable) in catalog::ALIASES {
                println!("{alias:<10} → {variable}");
            }
            Ok(())
        }
    }
}

fn init(shell: EnvShell) -> Result<()> {
    let bin = state::bin_dir()?;
    std::fs::create_dir_all(&bin)
        .with_context(|| format!("failed to create bin directory {}", bin.display()))?;

    let stored = state::read_state()?;
    match shell {
        EnvShell::Posix => shell::emit_posix_init(&stored, &bin),
        EnvShell::Fish => shell::emit_fish_init(&stored, &bin),
        EnvShell::Powershell => shell::emit_powershell_init(&stored, &bin),
    }
    Ok(())
}

fn set(alias: &str, value: &str, shell: EnvShell) -> Result<()> {
    let variable = catalog::variable_for_alias(alias)
        .with_context(|| format!("unknown environment alias: {alias}"))?;
    let mut stored = state::read_state()?;
    stored.insert(alias.to_owned(), value.to_owned());
    state::write_state(&stored)?;
    match shell {
        EnvShell::Posix => println!("export {variable}={}", shell::shell_quote(value)),
        EnvShell::Fish => println!("set -gx {variable} {}", shell::fish_quote(value)),
        EnvShell::Powershell => println!("$env:{variable} = {}", shell::powershell_quote(value)),
    }
    eprintln!("[lum env] Set {alias} ({variable})");
    Ok(())
}

fn unset(alias: &str, shell: EnvShell) -> Result<()> {
    let variable = catalog::variable_for_alias(alias)
        .with_context(|| format!("unknown environment alias: {alias}"))?;
    let mut stored = state::read_state()?;
    stored.remove(alias);
    state::write_state(&stored)?;
    match shell {
        EnvShell::Posix => println!("unset {variable}"),
        EnvShell::Fish => println!("set -e {variable}"),
        EnvShell::Powershell => {
            println!("Remove-Item Env:{variable} -ErrorAction SilentlyContinue")
        }
    }
    eprintln!("[lum env] Unset {alias} ({variable})");
    Ok(())
}

struct EnvRow<'a> {
    alias: &'a str,
    variable: &'a str,
    value: Option<&'a String>,
}

fn list() -> Result<()> {
    let stored = state::read_state()?;
    println!("Aliases (set with 'lum env set <alias> <value>'):");
    let mut rows: Vec<EnvRow> = catalog::ALIASES
        .iter()
        .map(|(alias, variable)| EnvRow {
            alias,
            variable,
            value: stored.get(*alias),
        })
        .collect();
    // Stable sort: set aliases first, unset aliases last; catalog
    // (alphabetical) order is preserved within each group.
    rows.sort_by_key(|row| row.value.is_none());
    for row in &rows {
        match row.value {
            Some(value) => println!(
                "  {:<10} {:<24} = {}",
                row.alias,
                row.variable,
                catalog::mask_secret(value)
            ),
            None => println!("  {:<10} {:<24}   (not set)", row.alias, row.variable),
        }
    }
    println!();
    println!("Forced defaults (auto-set, not configurable):");
    for (name, value) in catalog::FORCED_ENV {
        println!("  {name:<45} {value}");
    }
    Ok(())
}
