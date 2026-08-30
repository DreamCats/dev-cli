use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub(crate) const HOST_OS_POSIX: &str = "posix";
pub(crate) const HOST_OS_WINDOWS: &str = "windows";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct HostConfig {
    pub(crate) hostname: String,
    #[serde(default)]
    pub(crate) user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) os: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) shell: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) exec_timeout: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) repo_roots: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct AppConfig {
    #[serde(default)]
    pub(crate) default_host: String,
    #[serde(default)]
    pub(crate) hosts: BTreeMap<String, HostConfig>,
}

pub(crate) fn normalize_host_os(value: Option<&str>) -> Result<Option<String>> {
    let normalized = value.unwrap_or_default().trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => Ok(None),
        HOST_OS_POSIX | HOST_OS_WINDOWS => Ok(Some(normalized)),
        _ => bail!("unsupported host os {normalized:?}; use posix or windows"),
    }
}

pub(crate) fn host_os_or_default(value: Option<&str>) -> &'static str {
    if value.is_some_and(|value| value.trim().eq_ignore_ascii_case(HOST_OS_WINDOWS)) {
        HOST_OS_WINDOWS
    } else {
        HOST_OS_POSIX
    }
}

pub(crate) fn is_windows(value: Option<&str>) -> bool {
    host_os_or_default(value) == HOST_OS_WINDOWS
}
