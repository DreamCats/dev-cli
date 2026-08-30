use std::{fs, io::Read};

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

use crate::{
    cli::{self, Command},
    config,
    model::{self, HostConfig},
    output::Output,
    stats, update,
};

mod remote;

pub(crate) fn dispatch(cli: cli::Cli) -> Result<i32> {
    let out = Output { json: cli.json };
    match cli.command {
        Command::Config(args) => config_command(args, out),
        Command::History(args) => history(args.limit, out),
        Command::Stats(args) => stats_command(args, out),
        Command::Version => {
            println!("dev {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Command::Update { check } => update_command(check, out),
        command => remote::dispatch(command, out),
    }
}

fn update_command(check: bool, out: Output) -> Result<i32> {
    let report = update::run(check)?;
    if out.json {
        out.json(&report)?;
    } else if report.updated {
        println!(
            "已更新 dev: {} -> {}",
            report.current_version, report.latest_version
        );
    } else if report.current_version == report.latest_version {
        println!("dev 已是最新版本: {}", report.current_version);
    } else {
        println!(
            "发现新版本: {} -> {}，运行 `dev update` 安装",
            report.current_version, report.latest_version
        );
    }
    Ok(0)
}

fn config_command(args: cli::ConfigArgs, out: Output) -> Result<i32> {
    match args.command {
        cli::ConfigCommand::Show => config_show(out),
        cli::ConfigCommand::Add(args) => {
            let mut cfg = config::load()?;
            let os = model::normalize_host_os(args.os.as_deref())?;
            let host = HostConfig {
                hostname: args.hostname,
                user: args.user,
                os,
                shell: args.shell.filter(|value| !value.is_empty()),
                exec_timeout: args.exec_timeout,
                repo_roots: dedupe(args.repo_roots),
            };
            cfg.hosts.insert(args.alias.clone(), host);
            if args.set_default {
                cfg.default_host = args.alias.clone();
            }
            config::save(&cfg)?;
            let host = cfg.hosts.get(&args.alias).expect("saved host");
            println!("已添加主机: {} ({})", args.alias, host_target(host));
            if args.set_default {
                println!("已设为默认主机");
            }
            Ok(0)
        }
        cli::ConfigCommand::SetDefault(args) => {
            let mut cfg = config::load()?;
            require_alias(&cfg, &args.alias)?;
            cfg.default_host = args.alias.clone();
            config::save(&cfg)?;
            println!("已设置默认主机: {}", args.alias);
            Ok(0)
        }
        cli::ConfigCommand::SetOs(args) => {
            let value = model::normalize_host_os(Some(&args.value))?;
            update_host(&args.alias, |host| host.os = value)?;
            println!(
                "已设置 {} 的远端系统: {}",
                args.alias,
                args.value.to_ascii_lowercase()
            );
            Ok(0)
        }
        cli::ConfigCommand::SetShell(args) => {
            let shell = match args.value.as_str() {
                "none" | "" => None,
                "zsh" | "zsh-login" | "bash" | "bash-login" | "powershell" | "pwsh" | "cmd" => {
                    Some(args.value.clone())
                }
                _ => bail!(
                    "unsupported shell; use none, zsh, zsh-login, bash, bash-login, powershell, pwsh, or cmd"
                ),
            };
            let cleared = shell.is_none();
            update_host(&args.alias, |host| host.shell = shell)?;
            if cleared {
                println!("已清除 {} 的 dev exec 默认 shell", args.alias);
            } else {
                println!(
                    "已设置 {} 的 dev exec 默认 shell: {}",
                    args.alias, args.value
                );
            }
            Ok(0)
        }
        cli::ConfigCommand::SetExecTimeout(args) => {
            update_host(&args.alias, |host| {
                host.exec_timeout = (args.value > 0).then_some(args.value)
            })?;
            if args.value > 0 {
                println!(
                    "已设置 {} 的 dev exec 默认超时: {} 秒",
                    args.alias, args.value
                );
            } else {
                println!("已清除 {} 的 dev exec 默认超时", args.alias);
            }
            Ok(0)
        }
        cli::ConfigCommand::AddRepoRoot(args) => {
            let value = args.value.clone();
            update_host(&args.alias, |host| {
                host.repo_roots.push(value);
                host.repo_roots = dedupe(std::mem::take(&mut host.repo_roots));
            })?;
            println!("已添加 {} 的 repo root: {}", args.alias, args.value);
            Ok(0)
        }
        cli::ConfigCommand::ClearRepoRoots(args) => {
            update_host(&args.alias, |host| host.repo_roots.clear())?;
            println!("已清空 {} 的 repo roots", args.alias);
            Ok(0)
        }
    }
}

fn config_show(out: Output) -> Result<i32> {
    let cfg = config::load()?;
    if out.json {
        let hosts = cfg
            .hosts
            .iter()
            .map(|(alias, host)| {
                (
                    alias.clone(),
                    json!({
                        "hostname": host.hostname,
                        "user": host.user,
                        "os": model::host_os_or_default(host.os.as_deref()),
                        "shell": host.shell,
                        "exec_timeout": host.exec_timeout,
                        "repo_roots": host.repo_roots,
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        out.json(&json!({"default_host": cfg.default_host, "hosts": hosts}))?;
    } else {
        println!(
            "默认主机: {}",
            if cfg.default_host.is_empty() {
                "(未设置)"
            } else {
                &cfg.default_host
            }
        );
        println!("\n已配置主机:");
        for (alias, host) in cfg.hosts {
            let mut extras = Vec::new();
            if model::is_windows(host.os.as_deref()) {
                extras.push("os=windows".to_string());
            }
            if let Some(shell) = &host.shell {
                extras.push(format!("shell={shell}"));
            }
            if let Some(timeout) = host.exec_timeout {
                extras.push(format!("exec_timeout={timeout}"));
            }
            if !host.repo_roots.is_empty() {
                extras.push(format!("repo_roots={:?}", host.repo_roots));
            }
            println!(
                "  {alias}: {}{}",
                host_target(&host),
                if extras.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", extras.join(", "))
                }
            );
        }
    }
    Ok(0)
}

fn history(limit: usize, out: Output) -> Result<i32> {
    if limit == 0 {
        bail!("--limit 必须大于 0");
    }
    let events = stats::load_history(limit)?;
    if out.json {
        out.json(&json!({"events": events, "count": events.len()}))?;
    } else if events.is_empty() {
        println!("暂无本机命令历史");
    } else {
        println!("最近命令历史:");
        for event in events {
            println!(
                "  {}  {:<6} {:<16} {}ms{}",
                event.timestamp,
                if event.success { "ok" } else { "failed" },
                event.command,
                event.duration_ms,
                if event.session_id.is_empty() {
                    String::new()
                } else {
                    format!(" session={}", event.session_id)
                }
            );
        }
    }
    Ok(0)
}

fn stats_command(args: cli::StatsArgs, out: Output) -> Result<i32> {
    let data = stats::load();
    match args.command {
        Some(cli::StatsCommand::Suggest) => {
            let suggestions = build_stats_suggestions(&data, &config::load()?);
            if out.json {
                out.json(&json!({"suggestions": suggestions, "count": suggestions.len()}))?;
            } else if suggestions.is_empty() {
                println!("暂无明显优化建议");
            } else {
                println!("优化建议:");
                for suggestion in suggestions {
                    println!("\n  [{}] {}", suggestion["priority"], suggestion["title"]);
                    println!("      {}", suggestion["detail"]);
                    if let Some(command) = suggestion["command"]
                        .as_str()
                        .filter(|value| !value.is_empty())
                    {
                        println!("      建议命令: {command}");
                    }
                }
            }
        }
        None => {
            if out.json {
                out.json(&data)?;
            } else if data.is_empty() {
                println!("暂无使用记录");
            } else {
                let total = data.values().map(|entry| entry.count).sum::<u64>();
                let max = data.values().map(|entry| entry.count).max().unwrap_or(0);
                let mut rows = data.into_iter().collect::<Vec<_>>();
                rows.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.count));
                println!("命令使用统计 (共 {total} 次):\n");
                for (command, entry) in rows {
                    let pct = entry.count as f64 / total as f64 * 100.0;
                    let bar = if max == 0 {
                        0
                    } else {
                        (entry.count as f64 / max as f64 * 25.0 + 0.5) as usize
                    };
                    println!(
                        "  {command:<16} {:>4} 次 ({pct:>4.1}%)  {}",
                        entry.count,
                        "█".repeat(bar)
                    );
                }
            }
        }
    }
    Ok(0)
}

fn build_stats_suggestions(
    data: &std::collections::BTreeMap<String, stats::Entry>,
    cfg: &model::AppConfig,
) -> Vec<Value> {
    let mut result = Vec::new();
    let total = data.values().map(|entry| entry.count).sum::<u64>();
    let exec = data.get("exec").map_or(0, |entry| entry.count);
    if total > 0 && exec >= 10 && exec as f64 / total as f64 * 100.0 >= 60.0 {
        let pct = exec as f64 / total as f64 * 100.0;
        result.push(json!({"priority":"high","code":"prefer-structured-commands","title":"减少 raw exec，优先用结构化命令","detail":format!("exec 占 {pct:.1}%。读文件/日志优先用 slice/head/tail，搜索优先用 grep，便于控制输出和解析。") }));
    }
    let watches = data.get("exec-watch").map_or(0, |entry| entry.count);
    if exec >= 5 && (watches == 0 || exec / watches >= 20) {
        result.push(json!({"priority":"medium","code":"use-exec-watch","title":"长命令改用 exec --watch 或 exec-watch","detail":format!("exec 使用 {exec} 次，exec-watch 使用 {watches} 次。构建、测试、安装这类长命令用 watch 可以减少等待时的重复探测。"),"command":"dev exec --cwd ~/repo --watch --timeout 300 -- go test ./..."}));
    }
    let aliases = cfg
        .hosts
        .iter()
        .filter(|(_, host)| host.shell.is_none())
        .map(|(alias, _)| alias.as_str())
        .collect::<Vec<_>>();
    if let Some(first) = aliases.first() {
        result.push(json!({"priority":"medium","code":"set-default-shell","title":"给常用主机配置默认 shell","detail":format!("这些主机还没有默认 shell: {}。配置后可以少写 zsh -lc/batch shell 包裹。", aliases.join(", ")),"command":format!("dev config set-shell {first} zsh")}));
    }
    if !data.contains_key("grep") && exec >= 10 {
        result.push(json!({"priority":"low","code":"use-dev-grep","title":"远端搜索优先用 dev grep","detail":"dev grep 会优先使用远端 rg 并支持 --max-matches、--context 和 --json，比 exec rg 更适合 Agent 消费。"}));
    }
    result
}

fn require_alias(cfg: &model::AppConfig, alias: &str) -> Result<()> {
    cfg.hosts
        .contains_key(alias)
        .then_some(())
        .ok_or_else(|| anyhow!("主机 '{alias}' 未在配置中找到"))
}

fn update_host<F>(alias: &str, update: F) -> Result<()>
where
    F: FnOnce(&mut HostConfig),
{
    let mut cfg = config::load()?;
    let host = cfg
        .hosts
        .get_mut(alias)
        .ok_or_else(|| anyhow!("主机 '{alias}' 未在配置中找到"))?;
    update(host);
    config::save(&cfg)
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        if !result.contains(&value) {
            result.push(value);
        }
    }
    result
}

fn host_target(host: &HostConfig) -> String {
    if host.user.is_empty() {
        host.hostname.clone()
    } else {
        format!("{}@{}", host.user, host.hostname)
    }
}

pub(crate) fn read_stdin() -> Result<String> {
    let mut value = String::new();
    std::io::stdin().read_to_string(&mut value)?;
    Ok(value)
}

pub(crate) fn read_optional_stdin(
    value: Option<String>,
    file: Option<std::path::PathBuf>,
) -> Result<Option<String>> {
    if let Some(path) = file {
        return Ok(Some(fs::read_to_string(path)?));
    }
    match value.as_deref() {
        Some("-") => Ok(Some(read_stdin()?)),
        Some(_) => Ok(value),
        None => Ok(None),
    }
}
