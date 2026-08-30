mod cli;
mod commands;
mod config;
mod model;
mod output;
mod stats;
mod transport;
mod update;

use std::time::Instant;

use anyhow::Result;

pub fn run() -> Result<i32> {
    let cli = cli::parse();
    let tracked = cli.command.tracking_name();
    let started = Instant::now();
    let result = commands::dispatch(cli);
    if !tracked.is_empty() {
        stats::record(tracked);
        stats::record_history(
            tracked,
            result.as_ref().is_ok_and(|code| *code == 0),
            started.elapsed(),
        );
    }
    result
}
