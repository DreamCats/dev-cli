use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;

use crate::model::{self, AppConfig, HostConfig};

pub(crate) fn dir() -> PathBuf {
    if let Some(base) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(base).join("dev-cli");
    }
    home_dir().join(".config/dev-cli")
}

pub(crate) fn file() -> PathBuf {
    dir().join("config.yaml")
}

pub(crate) fn load() -> Result<AppConfig> {
    let path = file();
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AppConfig::default());
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    if raw.trim().is_empty() {
        return Ok(AppConfig::default());
    }
    let mut config: AppConfig =
        serde_yaml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    for (alias, host) in &mut config.hosts {
        host.os = model::normalize_host_os(host.os.as_deref())
            .with_context(|| format!("host {alias}"))?;
    }
    Ok(config)
}

pub(crate) fn save(config: &AppConfig) -> Result<()> {
    fs::create_dir_all(dir())?;
    let payload = ConfigPayload::from(config);
    fs::write(file(), serde_yaml::to_string(&payload)?)?;
    Ok(())
}

pub(crate) fn get_host(alias: &str) -> Result<HostConfig> {
    let config = load()?;
    let alias = alias.strip_prefix('@').unwrap_or(alias);
    let alias = if alias.is_empty() {
        config.default_host.as_str()
    } else {
        alias
    };
    if alias.is_empty() {
        bail!("未指定主机且未配置默认主机，请使用 @alias 或设置 default_host");
    }
    config.hosts.get(alias).cloned().ok_or_else(|| {
        let hosts = config.hosts.keys().cloned().collect::<Vec<_>>();
        anyhow!("主机 '{alias}' 未在配置中找到，可用主机: {hosts:?}")
    })
}

pub(crate) fn normalize_local_home_to_tilde(path: &str) -> String {
    let home = home_dir();
    path.strip_prefix(home.to_string_lossy().as_ref())
        .map_or_else(|| path.to_owned(), |rest| format!("~{rest}"))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

#[derive(Serialize)]
struct ConfigPayload<'a> {
    default_host: &'a str,
    hosts: std::collections::BTreeMap<&'a str, HostPayload<'a>>,
}

#[derive(Serialize)]
struct HostPayload<'a> {
    hostname: &'a str,
    user: &'a str,
    os: Option<&'a str>,
    shell: Option<&'a str>,
    exec_timeout: Option<i32>,
    repo_roots: &'a [String],
}

impl<'a> From<&'a AppConfig> for ConfigPayload<'a> {
    fn from(config: &'a AppConfig) -> Self {
        let hosts = config
            .hosts
            .iter()
            .map(|(alias, host)| {
                (
                    alias.as_str(),
                    HostPayload {
                        hostname: &host.hostname,
                        user: &host.user,
                        os: host.os.as_deref(),
                        shell: host.shell.as_deref(),
                        exec_timeout: host.exec_timeout,
                        repo_roots: &host.repo_roots,
                    },
                )
            })
            .collect();
        Self {
            default_host: &config.default_host,
            hosts,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn payload_keeps_python_compatible_empty_fields() {
        let config = AppConfig {
            default_host: "sgdev".into(),
            hosts: BTreeMap::from([(
                "sgdev".into(),
                HostConfig {
                    hostname: "10.0.0.1".into(),
                    user: "maifeng".into(),
                    ..HostConfig::default()
                },
            )]),
        };
        let raw = serde_yaml::to_string(&ConfigPayload::from(&config)).unwrap();
        for expected in [
            "os: null",
            "shell: null",
            "exec_timeout: null",
            "repo_roots: []",
        ] {
            assert!(raw.contains(expected), "missing {expected:?} in {raw}");
        }
    }
}
