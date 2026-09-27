use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufRead, BufReader, Write},
    time::{Duration, SystemTime},
};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::config;

const MAX_HISTORY_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Entry {
    pub(crate) count: u64,
    pub(crate) last_used: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct HistoryEvent {
    pub(crate) timestamp: String,
    pub(crate) command: String,
    pub(crate) success: bool,
    pub(crate) duration_ms: u128,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) session_id: String,
}

fn load_unlocked() -> BTreeMap<String, Entry> {
    fs::read(config::dir().join("stats.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

pub(crate) fn load() -> BTreeMap<String, Entry> {
    with_lock(false, || Ok(load_unlocked())).unwrap_or_else(|_| load_unlocked())
}

pub(crate) fn record(command: &str, success: bool, elapsed: Duration) {
    let _ = with_lock(true, || {
        record_stats_inner(command)?;
        record_history_inner(command, success, elapsed)
    });
}

fn record_stats_inner(command: &str) -> Result<()> {
    let mut data = load_unlocked();
    let entry = data.entry(command.into()).or_default();
    entry.count += 1;
    entry.last_used = now();
    fs::write(
        config::dir().join("stats.json"),
        serde_json::to_vec_pretty(&data)?,
    )?;
    Ok(())
}

fn record_history_inner(command: &str, success: bool, elapsed: Duration) -> Result<()> {
    fs::create_dir_all(config::dir())?;
    let path = config::dir().join("history.jsonl");
    if path
        .metadata()
        .is_ok_and(|metadata| metadata.len() >= MAX_HISTORY_BYTES)
    {
        let rotated = config::dir().join("history.jsonl.1");
        let _ = fs::remove_file(&rotated);
        fs::rename(&path, rotated)?;
    }
    let event = HistoryEvent {
        timestamp: now(),
        command: command.into(),
        success,
        duration_ms: elapsed.as_millis(),
        session_id: session_id(),
    };
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let mut line = serde_json::to_vec(&event)?;
    line.push(b'\n');
    file.write_all(&line)?;
    Ok(())
}

pub(crate) fn load_history(limit: usize) -> Result<Vec<HistoryEvent>> {
    if limit == 0 {
        bail!("history limit must be positive");
    }
    with_lock(false, || load_history_unlocked(limit))
}

fn load_history_unlocked(limit: usize) -> Result<Vec<HistoryEvent>> {
    let path = config::dir().join("history.jsonl");
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut events = BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<HistoryEvent>(&line).ok())
        .filter(|event| !event.command.is_empty())
        .collect::<Vec<_>>();
    if events.len() > limit {
        events.drain(..events.len() - limit);
    }
    events.reverse();
    Ok(events)
}

fn with_lock<T>(exclusive: bool, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    fs::create_dir_all(config::dir())?;
    let mut options = fs::OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(config::dir().join("usage.lock"))?;
    if exclusive {
        FileExt::lock_exclusive(&lock)?;
    } else {
        FileExt::lock_shared(&lock)?;
    }
    operation()
}

fn session_id() -> String {
    ["DEV_SESSION_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID"]
        .into_iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default()
}

fn now() -> String {
    let value: DateTime<Utc> = SystemTime::now().into();
    value.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}
