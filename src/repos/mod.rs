pub mod mirror;
pub mod scanner;

use anyhow::{Context, Result};

use crate::cli::ReposCommand;

pub fn run(command: ReposCommand) -> Result<()> {
    match command {
        ReposCommand::Scan(args) => scanner::run(&args),
        ReposCommand::Mirror { command } => mirror::run(command),
    }
}

pub(crate) fn ensure_git_on_path() -> Result<()> {
    which::which("git").context("git executable not found on PATH")?;
    Ok(())
}

fn worker_pool(jobs: usize, tasks: usize) -> Result<rayon::ThreadPool> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.max(1).min(tasks.max(1)))
        .build()
        .context("failed to create repository worker pool")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_pool_honors_jobs_and_available_tasks() {
        for (jobs, tasks, expected) in [(0, 3, 1), (1, 3, 1), (2, 3, 2), (10, 3, 3)] {
            let pool = worker_pool(jobs, tasks).unwrap();
            assert_eq!(pool.current_num_threads(), expected);
        }
    }
}
