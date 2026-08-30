use std::{
    io::Write,
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use wait_timeout::ChildExt;

use crate::{config, model};

#[derive(Debug)]
pub(crate) struct CommandResult {
    pub(crate) return_code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl CommandResult {
    pub(crate) fn success(&self) -> bool {
        self.return_code == 0
    }
}

pub(crate) fn run_command(
    command: &str,
    host_alias: &str,
    timeout: i32,
    shell: &str,
    stdin: Option<&str>,
) -> Result<CommandResult> {
    let host = config::get_host(host_alias)?;
    let wrapped = wrap_shell_cmd(command, shell);
    let mut args = ssh_args(&host);
    args.push(wrapped);
    run_local(&args, timeout, "命令执行", stdin)
}

pub(crate) fn ssh_command_args(host_alias: &str, command: &str) -> Result<Vec<String>> {
    let host = config::get_host(host_alias)?;
    let mut args = ssh_args(&host);
    args.push(command.into());
    Ok(args)
}

pub(crate) fn upload(
    local: &str,
    remote: &str,
    host_alias: &str,
    timeout: i32,
    recursive: bool,
) -> Result<CommandResult> {
    let host = config::get_host(host_alias)?;
    run_local(
        &scp_args(&host, local, remote, false, recursive),
        timeout,
        "上传",
        None,
    )
}

pub(crate) fn download(
    remote: &str,
    local: &str,
    host_alias: &str,
    timeout: i32,
) -> Result<CommandResult> {
    let host = config::get_host(host_alias)?;
    run_local(
        &scp_args(&host, remote, local, true, false),
        timeout,
        "下载",
        None,
    )
}

fn run_local(
    args: &[String],
    timeout: i32,
    label: &str,
    stdin: Option<&str>,
) -> Result<CommandResult> {
    let (program, command_args) = args.split_first().expect("command has program");
    let mut command = Command::new(program);
    command
        .args(command_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().with_context(|| format!("{label}失败"))?;
    if let Some(value) = stdin {
        child
            .stdin
            .take()
            .expect("stdin piped")
            .write_all(value.as_bytes())?;
    }
    if timeout > 0
        && child
            .wait_timeout(Duration::from_secs(timeout as u64))?
            .is_none()
    {
        let _ = child.kill();
        let _ = child.wait();
        bail!("{label}超时（{timeout} 秒）");
    }
    let output = child.wait_with_output()?;
    Ok(CommandResult {
        return_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn ssh_args(host: &model::HostConfig) -> Vec<String> {
    vec![
        "ssh".into(),
        "-o".into(),
        "ControlMaster=auto".into(),
        "-o".into(),
        "ControlPath=~/.ssh/sockets/%r@%h-%p".into(),
        "-o".into(),
        "ControlPersist=600".into(),
        remote_target(host),
    ]
}

fn scp_args(
    host: &model::HostConfig,
    source: &str,
    destination: &str,
    reverse: bool,
    recursive: bool,
) -> Vec<String> {
    let mut args = vec![
        "scp".into(),
        "-o".into(),
        "ControlMaster=auto".into(),
        "-o".into(),
        "ControlPath=~/.ssh/sockets/%r@%h-%p".into(),
        "-o".into(),
        "ControlPersist=600".into(),
    ];
    if recursive {
        args.push("-r".into());
    }
    let remote = remote_target(host);
    if reverse {
        args.extend([format!("{remote}:{source}"), destination.into()]);
    } else {
        args.extend([source.into(), format!("{remote}:{destination}")]);
    }
    args
}

pub(crate) fn remote_target(host: &model::HostConfig) -> String {
    if host.user.is_empty() {
        host.hostname.clone()
    } else {
        format!("{}@{}", host.user, host.hostname)
    }
}

pub(crate) fn wrap_shell_cmd(command: &str, shell: &str) -> String {
    match shell {
        "" | "none" => command.into(),
        "powershell" | "pwsh" => format!(
            "{shell} -NoProfile -NonInteractive -EncodedCommand {}",
            powershell_encoded_command(command)
        ),
        "cmd" => format!("cmd /C {}", cmd_quote(command)),
        "zsh" => format!("zsh -ic {}", shell_quote(command)),
        "zsh-login" => format!("zsh -lic {}", shell_quote(command)),
        "bash" => format!("bash -ic {}", shell_quote(command)),
        "bash-login" => format!("bash -lic {}", shell_quote(command)),
        custom => format!("{custom} {}", shell_quote(command)),
    }
}

pub(crate) fn powershell_command(script: &str) -> String {
    format!(
        "powershell -NoProfile -NonInteractive -EncodedCommand {}",
        powershell_encoded_command(script)
    )
}

pub(crate) fn powershell_encoded_command(script: &str) -> String {
    let bytes = script
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    STANDARD.encode(bytes)
}

pub(crate) fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(crate) fn cmd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(crate) fn wrap_remote_cwd(command: &str, cwd: &str) -> String {
    if cwd.is_empty() {
        command.into()
    } else {
        format!("cd {} && {command}", quote_remote_path(cwd))
    }
}

pub(crate) fn wrap_remote_cwd_for_os(
    command: &str,
    cwd: &str,
    host_os: Option<&str>,
    shell: &str,
) -> String {
    if cwd.is_empty() {
        return command.into();
    }
    if model::is_windows(host_os) {
        if matches!(shell, "powershell" | "pwsh") {
            format!(
                "Set-Location -LiteralPath {}\n{command}",
                powershell_quote(cwd)
            )
        } else {
            format!("cd /d {} && {command}", cmd_quote(cwd))
        }
    } else {
        wrap_remote_cwd(command, cwd)
    }
}

pub(crate) fn quote_remote_path(path: &str) -> String {
    if path == "~" {
        return "~".into();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return if needs_quoting(rest) {
            format!("~/{}", shell_quote(rest))
        } else {
            path.into()
        };
    }
    if needs_quoting(path) {
        shell_quote(path)
    } else {
        path.into()
    }
}

pub(crate) fn expand_tilde(path: &str) -> String {
    quote_remote_path(path)
}

pub(crate) fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".into();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn needs_quoting(value: &str) -> bool {
    value.is_empty()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'/' | b'@' | b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_remote_paths_and_shells_like_go_reference() {
        assert_eq!(quote_remote_path("~/a b"), "~/'a b'");
        assert_eq!(shell_quote("a'b"), "'a'\"'\"'b'");
        assert_eq!(wrap_shell_cmd("echo ok", "zsh"), "zsh -ic 'echo ok'");
    }

    #[test]
    fn powershell_encoding_round_trips_utf16le() {
        let encoded = powershell_encoded_command("Write-Output '你好'");
        let bytes = STANDARD.decode(encoded).unwrap();
        let words = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        assert_eq!(String::from_utf16(&words).unwrap(), "Write-Output '你好'");
    }
}
