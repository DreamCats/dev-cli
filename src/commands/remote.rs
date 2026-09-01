use std::{
    fs,
    io::{BufRead, BufReader, Read},
    process::{Command as ProcessCommand, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

use crate::{
    cli::{self, Command},
    commands::read_optional_stdin,
    config, model,
    output::Output,
    transport,
};

const DEFAULT_CAT_MAX_BYTES: usize = 256 * 1024;

pub(super) fn dispatch(command: Command, out: Output) -> Result<i32> {
    match command {
        Command::Ls(args) => list_dir(args, out),
        Command::Cat(args) => cat(args, out),
        Command::Push(args) => push(args, out),
        Command::Pull(args) => pull(args, out),
        Command::Exec(args) => exec(args.args, out),
        Command::Grep(args) => grep(args, out),
        Command::Find(args) => find(args, out),
        Command::Tree(args) => tree(args, out),
        Command::Head(args) => head_tail("head", args, out),
        Command::Tail(args) => head_tail("tail", args, out),
        Command::Slice(args) => slice(args, out),
        Command::Write(args) => write(args, out),
        Command::Edit(args) => edit(args, out),
        Command::Diff(args) => diff(args, out),
        Command::RepoStatus(args) => repo_status(args, out),
        Command::RepoDiff(args) => repo_diff(args, out),
        Command::GitSnapshot(args) => git_snapshot(args, out),
        Command::Repo(args) => repo(args, out),
        Command::Verify(args) => verify(args, out),
        Command::ExecWatch(args) => exec_watch(args, out),
        Command::Patch(args) => patch(args, out),
        Command::Cg(args) => cg(args.args, out),
        Command::Version
        | Command::Update { .. }
        | Command::Config(_)
        | Command::History(_)
        | Command::Stats(_) => {
            unreachable!("handled by parent dispatcher")
        }
    }
}

fn normalize(path: &str) -> String {
    config::normalize_local_home_to_tilde(path)
}

fn host_os(alias: &str) -> Result<String> {
    let host = config::get_host(alias)?;
    Ok(model::host_os_or_default(host.os.as_deref()).into())
}

fn is_windows(alias: &str) -> Result<bool> {
    Ok(host_os(alias)? == model::HOST_OS_WINDOWS)
}

fn windows_unsupported(command: &str, out: Output) -> Result<i32> {
    let message = format!("{command} is not supported for Windows hosts yet");
    if out.json {
        out.json(&json!({
            "success": false, "unsupported": true, "os": "windows",
            "command": command, "error": message,
        }))?;
    } else {
        out.stderr(&format!("错误: {message}\n"))?;
    }
    Ok(1)
}

fn list_dir(args: cli::PathArgs, out: Output) -> Result<i32> {
    let path = normalize(&args.path);
    let cwd = normalize_optional(&args.cwd);
    if is_windows(&args.host.host)? {
        return list_dir_windows(&path, &args.host.host, &cwd, out);
    }
    let mut target = transport::expand_tilde(&path);
    if target == "~" {
        target = "~/".into();
    }
    let command = transport::wrap_remote_cwd(&format!("ls -la {target}"), &cwd);
    let result = transport::run_command(&command, &args.host.host, 30, "", None)?;
    if !result.success() {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    if out.json {
        out.json(&parse_ls(&result.stdout, &path))?;
    } else {
        out.stdout(&format!("{}\n", result.stdout))?;
    }
    Ok(0)
}

fn list_dir_windows(path: &str, host: &str, cwd: &str, out: Output) -> Result<i32> {
    let result = transport::run_command(&build_windows_ls_cmd(path, cwd), host, 30, "", None)?;
    let payload = parse_json_result(&result.stdout, path);
    if out.json {
        out.json(&payload)?;
    } else if result.success() {
        out.stdout(payload["plain"].as_str().unwrap_or_default())?;
    } else {
        out.stderr(&format!(
            "错误: {}\n",
            first_non_empty(
                payload["error"].as_str().unwrap_or_default(),
                &result.stderr
            )
        ))?;
    }
    Ok(if result.success() {
        0
    } else {
        fallback_code(result.return_code)
    })
}

fn build_windows_ls_cmd(path: &str, cwd: &str) -> String {
    let mut steps = vec![windows_path_resolver_script().into()];
    if !cwd.is_empty() {
        steps.push(format!(
            "Set-Location -LiteralPath (Resolve-DevPath {})",
            transport::powershell_quote(cwd)
        ));
    }
    steps.push(format!(
        r#"
$target = Resolve-DevPath {}
try {{
  $entries = @(Get-ChildItem -LiteralPath $target -Force -ErrorAction Stop)
  $items = @()
  foreach ($entry in $entries) {{
    $itemType = if ($entry.PSIsContainer) {{ "directory" }} else {{ "file" }}
    $itemSize = if ($entry.PSIsContainer) {{ "" }} else {{ [string]$entry.Length }}
    $items += [pscustomobject]@{{name=$entry.Name; type=$itemType; permissions=$entry.Mode; size=$itemSize}}
  }}
  $plain = ($entries | Format-Table Mode,Length,LastWriteTime,Name -AutoSize | Out-String)
  [pscustomobject]@{{path={}; items=$items; count=$items.Count; plain=$plain; success=$true; error=""}} | ConvertTo-Json -Compress -Depth 5
}} catch {{
  [pscustomobject]@{{path={}; items=@(); count=0; plain=""; success=$false; error=$_.Exception.Message}} | ConvertTo-Json -Compress -Depth 5
  exit 1
}}
"#,
        transport::powershell_quote(path),
        transport::powershell_quote(path),
        transport::powershell_quote(path)
    ));
    transport::powershell_command(&steps.join("\n"))
}

fn parse_ls(raw: &str, path: &str) -> Value {
    let items = raw
        .trim()
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 9 {
                return None;
            }
            let name = fields[8..].join(" ");
            if matches!(name.as_str(), "." | "..") {
                return None;
            }
            Some(json!({
                "name": name,
                "type": if fields[0].starts_with('d') { "directory" } else { "file" },
                "permissions": fields[0], "size": fields[4],
            }))
        })
        .collect::<Vec<_>>();
    json!({"path": path, "count": items.len(), "items": items})
}

fn cat(args: cli::CatArgs, out: Output) -> Result<i32> {
    let paths = args
        .paths
        .iter()
        .map(|path| normalize(path))
        .collect::<Vec<_>>();
    let cwd = normalize_optional(&args.cwd);
    let command = if is_windows(&args.host.host)? {
        build_windows_cat_cmd(&paths, &cwd, args.full)
    } else {
        build_cat_cmd(&paths, &cwd, args.full)?
    };
    let result = transport::run_command(&command, &args.host.host, 30, "", None)?;
    let payload: Value = match serde_json::from_str(&result.stdout) {
        Ok(payload) => payload,
        Err(_) => {
            if result.stderr.is_empty() {
                out.stderr("错误: 无法解析远程 cat 输出\n")?;
            } else {
                out.stderr(&format!("错误: {}", result.stderr))?;
            }
            return Ok(fallback_code(result.return_code));
        }
    };
    if out.json {
        out.json(&payload)?;
    } else {
        print_cat_plain(&payload, out)?;
        out.stderr(&result.stderr)?;
    }
    Ok(if result.success() {
        0
    } else {
        result.return_code
    })
}

fn build_cat_cmd(paths: &[String], cwd: &str, full: bool) -> Result<String> {
    let paths = serde_json::to_string(paths)?;
    let max_bytes = if full {
        "None".into()
    } else {
        DEFAULT_CAT_MAX_BYTES.to_string()
    };
    let script = format!(
        r#"
import json
import os
from pathlib import Path
import sys
paths = {paths:?}
max_bytes = {max_bytes}
items = []
has_error = False
for path in json.loads(paths):
    try:
        target = Path(os.path.expanduser(path))
        size = target.stat().st_size
        if max_bytes is not None and size > max_bytes:
            items.append({{"path": path, "content": "", "size": size, "truncated": True, "success": True, "error": ""}})
            continue
        content = target.read_text(errors="replace")
        items.append({{"path": path, "content": content, "size": size, "truncated": False, "success": True, "error": ""}})
    except Exception as exc:
        has_error = True
        items.append({{"path": path, "content": "", "size": 0, "truncated": False, "success": False, "error": str(exc)}})
print(json.dumps({{"cwd": os.getcwd(), "files": items, "count": len(items), "success": not has_error}}, ensure_ascii=False))
sys.exit(1 if has_error else 0)
"#
    );
    let mut steps = vec!["set -e".into()];
    if !cwd.is_empty() {
        steps.push(format!("cd {}", transport::quote_remote_path(cwd)));
    }
    steps.extend(["python3 - <<'PY'".into(), script.trim().into(), "PY".into()]);
    Ok(steps.join("\n"))
}

fn build_windows_cat_cmd(paths: &[String], cwd: &str, full: bool) -> String {
    let paths = serde_json::to_string(paths).expect("serialize paths");
    let max_bytes = if full {
        "$null".into()
    } else {
        DEFAULT_CAT_MAX_BYTES.to_string()
    };
    let mut steps = vec![windows_path_resolver_script().into()];
    if !cwd.is_empty() {
        steps.push(format!(
            "Set-Location -LiteralPath (Resolve-DevPath {})",
            transport::powershell_quote(cwd)
        ));
    }
    steps.push(format!(
        r#"
$paths = @({} | ConvertFrom-Json)
$maxBytes = {max_bytes}
$items = @()
$hasError = $false
foreach ($path in $paths) {{
  try {{
    $target = Resolve-DevPath ([string]$path)
    $size = (Get-Item -LiteralPath $target -ErrorAction Stop).Length
    if ($null -ne $maxBytes -and $size -gt $maxBytes) {{
      $items += [pscustomobject]@{{path=[string]$path; content=""; size=$size; truncated=$true; success=$true; error=""}}
      continue
    }}
    $content = Get-Content -LiteralPath $target -Raw -ErrorAction Stop
    if ($null -eq $content) {{ $content = "" }}
    $items += [pscustomobject]@{{path=[string]$path; content=$content; size=$size; truncated=$false; success=$true; error=""}}
  }} catch {{
    $hasError = $true
    $items += [pscustomobject]@{{path=[string]$path; content=""; size=0; truncated=$false; success=$false; error=$_.Exception.Message}}
  }}
}}
$payload = [pscustomobject]@{{cwd=(Get-Location).Path; files=$items; count=$items.Count; success=(-not $hasError)}}
$payload | ConvertTo-Json -Compress -Depth 5
if ($hasError) {{ exit 1 }}
"#,
        transport::powershell_quote(&paths)
    ));
    transport::powershell_command(&steps.join("\n"))
}

fn print_cat_plain(payload: &Value, out: Output) -> Result<()> {
    let files = payload["files"]
        .as_array()
        .ok_or_else(|| anyhow!("远程 cat 输出缺少 files"))?;
    let single = files.len() == 1;
    for (index, item) in files.iter().enumerate() {
        let path = item["path"].as_str().unwrap_or_default();
        let success = item["success"].as_bool().unwrap_or(false);
        if !single {
            if index > 0 {
                out.stdout("\n")?;
            }
            out.stdout(&format!(
                "===== {path}{} =====\n",
                if success { "" } else { " (error)" }
            ))?;
        }
        if item["truncated"].as_bool().unwrap_or(false) {
            out.stdout(&format!(
                "文件过大（{} bytes），未读取全文。使用 dev slice {path} --range START:END，或 dev cat --full {path}。\n",
                item["size"]
            ))?;
        } else if success {
            out.stdout(item["content"].as_str().unwrap_or_default())?;
        } else {
            out.stderr(&format!("错误: {path}: {}\n", item["error"]))?;
        }
    }
    Ok(())
}

fn push(args: cli::PushArgs, out: Output) -> Result<i32> {
    let recursive = args.recursive || fs::metadata(&args.local).is_ok_and(|value| value.is_dir());
    let remote = normalize(&args.remote);
    let result = transport::upload(&args.local, &remote, &args.host.host, 60, recursive)?;
    if !result.success() {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    out.stdout(&format!("已上传: {} -> {remote}\n", args.local))?;
    Ok(0)
}

fn pull(args: cli::PullArgs, out: Output) -> Result<i32> {
    let remote = normalize(&args.remote);
    let result = transport::download(&remote, &args.local, &args.host.host, 60)?;
    if !result.success() {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    out.stdout(&format!("已下载: {remote} -> {}\n", args.local))?;
    Ok(0)
}

#[derive(Debug, Default)]
struct ExecOptions {
    host: String,
    cwd: String,
    timeout: Option<i32>,
    watch: bool,
    interval: i32,
    summary_chars: i32,
    shell: String,
    stdin: Option<String>,
    stdin_file: Option<std::path::PathBuf>,
    command: Vec<String>,
}

fn exec(args: Vec<String>, out: Output) -> Result<i32> {
    let options = parse_exec(args)?;
    let stdin = read_optional_stdin(options.stdin, options.stdin_file)?;
    let command = shell_join(&options.command);
    if options.watch {
        return exec_watch_values(
            &command,
            &options.host,
            options.interval,
            options.timeout.unwrap_or(300),
            &options.shell,
            options.summary_chars,
            &options.cwd,
            stdin.as_deref(),
            out,
        );
    }
    exec_values(
        &command,
        &options.host,
        options.timeout,
        &options.shell,
        &options.cwd,
        stdin.as_deref(),
        out,
    )
}

fn parse_exec(args: Vec<String>) -> Result<ExecOptions> {
    let mut options = ExecOptions {
        interval: 10,
        summary_chars: 20_000,
        ..ExecOptions::default()
    };
    let mut index = 0;
    if args.len() >= 2 && looks_like_exec_host(&args[0]) {
        options.host = args[0].clone();
        index = 1;
    }
    while index < args.len() {
        match args[index].as_str() {
            "--" => {
                options.command.extend_from_slice(&args[index + 1..]);
                break;
            }
            "--host" | "-H" | "-h" => options.host = next_value(&args, &mut index)?.into(),
            "--cwd" => options.cwd = normalize(next_value(&args, &mut index)?),
            "--timeout" | "-t" => options.timeout = Some(next_value(&args, &mut index)?.parse()?),
            "--watch" | "--wait" => options.watch = true,
            "--interval" => options.interval = next_value(&args, &mut index)?.parse()?,
            "--summary-chars" => options.summary_chars = next_value(&args, &mut index)?.parse()?,
            "--shell" => options.shell = next_value(&args, &mut index)?.into(),
            "--stdin" => options.stdin = Some(next_value(&args, &mut index)?.into()),
            "--stdin-file" => options.stdin_file = Some(next_value(&args, &mut index)?.into()),
            _ => {
                options.command.extend_from_slice(&args[index..]);
                break;
            }
        }
        index += 1;
    }
    if options.command.is_empty() {
        bail!("缺少 COMMAND。示例: dev exec --host myhost --cwd ~/repo -- go test ./...");
    }
    if options.host.is_empty()
        && options.command.len() >= 2
        && looks_like_exec_host(&options.command[0])
    {
        options.host = options.command.remove(0);
    }
    Ok(options)
}

fn looks_like_exec_host(value: &str) -> bool {
    value.starts_with('@')
        || config::load()
            .ok()
            .and_then(|config| config.hosts.get(value).cloned())
            .is_some_and(|host| !host.hostname.is_empty())
}

fn next_value<'a>(args: &'a [String], index: &mut usize) -> Result<&'a str> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("{} requires value", args[*index - 1]))
}

fn shell_join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| {
            if part
                .bytes()
                .any(|byte| b" \t\n'\"\\$`|&;<>*?()[]{}!".contains(&byte))
            {
                transport::shell_quote(part)
            } else {
                part.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn exec_values(
    command: &str,
    host: &str,
    timeout: Option<i32>,
    shell: &str,
    cwd: &str,
    stdin: Option<&str>,
    out: Output,
) -> Result<i32> {
    let host_config = config::get_host(host)?;
    let active_shell = if shell == "none" {
        String::new()
    } else if shell.is_empty() {
        host_config.shell.clone().unwrap_or_default()
    } else {
        shell.into()
    };
    let active_timeout = timeout.or(host_config.exec_timeout).unwrap_or(30);
    let effective =
        transport::wrap_remote_cwd_for_os(command, cwd, host_config.os.as_deref(), &active_shell);
    if !out.json && active_timeout >= 45 {
        out.stderr(&format!("started: {command} timeout={active_timeout}s\n"))?;
    }
    let result = transport::run_command(&effective, host, active_timeout, &active_shell, stdin)?;
    if out.json {
        out.json(&json!({
            "command": command, "effective_command": effective, "cwd": nullable(cwd),
            "shell": nullable(&active_shell), "timeout": active_timeout,
            "returncode": result.return_code, "stdin": stdin.is_some(),
            "stdin_bytes": stdin.map_or(0, str::len), "stdout": result.stdout,
            "stderr": result.stderr, "success": result.success(),
        }))?;
    } else {
        out.stdout(&result.stdout)?;
        out.stderr(&result.stderr)?;
    }
    Ok(if result.success() {
        0
    } else {
        result.return_code
    })
}

fn normalize_optional(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        normalize(path)
    }
}

fn nullable(value: &str) -> Value {
    if value.is_empty() {
        Value::Null
    } else {
        Value::String(value.into())
    }
}

fn fallback_code(code: i32) -> i32 {
    if code == 0 { 1 } else { code }
}

fn first_non_empty<'a>(first: &'a str, second: &'a str) -> &'a str {
    if first.is_empty() { second } else { first }
}

fn parse_json_result(raw: &str, fallback: &str) -> Value {
    serde_json::from_str(raw)
        .unwrap_or_else(|_| json!({"success": false, "error": fallback, "stdout": raw}))
}

fn windows_path_resolver_script() -> &'static str {
    r#"function Resolve-DevPath([string]$Path) {
  if ($Path -eq "~") { return $HOME }
  if ($Path.StartsWith("~/") -or $Path.StartsWith("~\")) { return Join-Path $HOME $Path.Substring(2) }
  return [Environment]::ExpandEnvironmentVariables($Path)
}"#
}

fn ps_bool(value: bool) -> &'static str {
    if value { "$true" } else { "$false" }
}

fn py_string(value: &str) -> String {
    serde_json::to_string(value).expect("serialize Python string")
}

fn py_string_or_none(value: &str) -> String {
    if value.is_empty() {
        "None".into()
    } else {
        py_string(value)
    }
}

fn grep(args: cli::GrepArgs, out: Output) -> Result<i32> {
    if args.context < 0 {
        bail!("--context 不能小于 0");
    }
    if args.max_matches == Some(0) {
        bail!("--max-matches 必须大于 0");
    }
    let windows = is_windows(&args.host.host)?;
    let use_rg = if windows {
        false
    } else {
        transport::run_command("which rg", &args.host.host, 5, "", None)?.success()
    };
    let path = normalize(&args.path);
    let cwd = normalize_optional(&args.cwd);
    let line_number = !args.no_line_number || out.json || args.context > 0 || args.group;
    let mut command = build_grep_cmd(
        &args.pattern,
        &path,
        use_rg,
        &args.include,
        line_number,
        args.context,
        args.max_matches,
        args.ignore_case,
    );
    command = transport::wrap_remote_cwd(&command, &cwd);
    if windows {
        command = build_windows_grep_cmd(
            &args.pattern,
            &path,
            &cwd,
            &args.include,
            line_number,
            args.context,
            args.max_matches,
            args.ignore_case,
        );
    }
    let result = transport::run_command(&command, &args.host.host, 30, "", None)?;
    if !result.success() && result.return_code != 1 {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    let (matches, files) = parse_grep_output(&result.stdout, args.max_matches);
    if out.json {
        out.json(&json!({
            "pattern": args.pattern, "path": path, "tool": if use_rg { "rg" } else { "grep" },
            "context": args.context, "cwd": nullable(&cwd), "ignore_case": args.ignore_case,
            "count": matches.len(), "file_count": files.len(), "matches": matches, "files": files,
        }))?;
    } else if args.group {
        print_grep_grouped(&files, out)?;
    } else {
        out.stdout(&result.stdout)?;
    }
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
fn build_grep_cmd(
    pattern: &str,
    path: &str,
    use_rg: bool,
    include: &str,
    line_number: bool,
    context: i32,
    max_matches: Option<usize>,
    ignore_case: bool,
) -> String {
    let mut parts = if use_rg {
        vec!["rg".into()]
    } else {
        vec!["grep".into(), "-r".into()]
    };
    if line_number {
        parts.push("-n".into());
    }
    if ignore_case {
        parts.push("-i".into());
    }
    if context > 0 {
        parts.extend(["-C".into(), context.to_string()]);
    }
    if let Some(max) = max_matches {
        parts.extend(["-m".into(), max.to_string()]);
    }
    if !include.is_empty() {
        if use_rg {
            parts.extend(["--glob".into(), transport::shell_quote(include)]);
        } else {
            parts.push(format!("--include={}", transport::shell_quote(include)));
        }
    }
    parts.extend([
        transport::shell_quote(pattern),
        transport::expand_tilde(path),
    ]);
    parts.join(" ")
}

#[allow(clippy::too_many_arguments)]
fn build_windows_grep_cmd(
    pattern: &str,
    path: &str,
    cwd: &str,
    include: &str,
    line_number: bool,
    context: i32,
    max_matches: Option<usize>,
    ignore_case: bool,
) -> String {
    let mut steps = vec![windows_path_resolver_script().into()];
    if !cwd.is_empty() {
        steps.push(format!(
            "Set-Location -LiteralPath (Resolve-DevPath {})",
            transport::powershell_quote(cwd)
        ));
    }
    steps.push(format!(r#"
$pattern = {}
$root = Resolve-DevPath {}
$include = {}
$lineNumber = {}
$context = {context}
$maxMatches = {}
$regexOptions = if ({}) {{ [System.Text.RegularExpressions.RegexOptions]::IgnoreCase }} else {{ [System.Text.RegularExpressions.RegexOptions]::None }}
$count = 0
if (Test-Path -LiteralPath $root -PathType Leaf) {{
  $files = @(Get-Item -LiteralPath $root)
}} elseif ($include -ne "") {{
  $files = @(Get-ChildItem -LiteralPath $root -Recurse -File -Force -Filter $include -ErrorAction Stop)
}} else {{
  $files = @(Get-ChildItem -LiteralPath $root -Recurse -File -Force -ErrorAction Stop)
}}
foreach ($file in $files) {{
  try {{ $content = @(Get-Content -LiteralPath $file.FullName -ErrorAction Stop) }} catch {{ continue }}
  for ($i = 0; $i -lt $content.Count; $i++) {{
    $line = [string]$content[$i]
    if (-not [regex]::IsMatch($line, $pattern, $regexOptions)) {{ continue }}
    if ($context -gt 0 -and $lineNumber) {{
      $start = [Math]::Max(0, $i - $context)
      $end = [Math]::Min($content.Count - 1, $i + $context)
      for ($j = $start; $j -le $end; $j++) {{
        $sep = if ($j -eq $i) {{ ":" }} else {{ "-" }}
        [Console]::Out.WriteLine(("{{0}}{{1}}{{2}}{{1}}{{3}}" -f $file.FullName, $sep, ($j + 1), [string]$content[$j]))
      }}
    }} elseif ($lineNumber) {{
      [Console]::Out.WriteLine(("{{0}}:{{1}}:{{2}}" -f $file.FullName, ($i + 1), $line))
    }} else {{
      [Console]::Out.WriteLine(("{{0}}:{{1}}" -f $file.FullName, $line))
    }}
    $count++
    if ($maxMatches -gt 0 -and $count -ge $maxMatches) {{ exit 0 }}
  }}
}}
if ($count -eq 0) {{ exit 1 }}
"#,
        transport::powershell_quote(pattern), transport::powershell_quote(path),
        transport::powershell_quote(include), ps_bool(line_number), max_matches.unwrap_or(0),
        ps_bool(ignore_case)
    ));
    transport::powershell_command(&steps.join("\n"))
}

#[derive(Clone, Debug)]
struct ParsedLine {
    file: String,
    line: usize,
    content: String,
    is_match: bool,
}

fn parse_grep_output(output: &str, max_matches: Option<usize>) -> (Vec<Value>, Vec<Value>) {
    let mut matches = Vec::<Value>::new();
    let mut files = std::collections::BTreeMap::<String, Vec<Value>>::new();
    let mut order = Vec::<String>::new();
    let mut pending = std::collections::BTreeMap::<String, Vec<Value>>::new();
    let mut last = std::collections::BTreeMap::<String, usize>::new();
    for raw in output.lines() {
        if raw == "--" {
            pending.clear();
            last.clear();
            continue;
        }
        let Some(item) = parse_grep_line(raw) else {
            continue;
        };
        if !files.contains_key(&item.file) {
            order.push(item.file.clone());
        }
        files.entry(item.file.clone()).or_default();
        if item.is_match {
            if max_matches.is_some_and(|max| matches.len() >= max) {
                break;
            }
            let value = json!({
                "file": item.file, "line": item.line, "content": item.content,
                "before": pending.remove(&item.file).unwrap_or_default(), "after": [],
            });
            matches.push(value.clone());
            files.get_mut(&item.file).expect("file bucket").push(value);
            last.insert(item.file, matches.len() - 1);
        } else {
            let context = json!({"line": item.line, "content": item.content});
            if let Some(index) = last.get(&item.file).copied() {
                matches[index]["after"]
                    .as_array_mut()
                    .expect("after array")
                    .push(context.clone());
                if let Some(bucket) = files.get_mut(&item.file) {
                    if let Some(last_match) = bucket.last_mut() {
                        last_match["after"]
                            .as_array_mut()
                            .expect("after array")
                            .push(context.clone());
                    }
                }
            }
            pending.entry(item.file).or_default().push(context);
        }
    }
    let grouped = order
        .into_iter()
        .map(|file| {
            let bucket = files.remove(&file).unwrap_or_default();
            json!({"file": file, "count": bucket.len(), "matches": bucket})
        })
        .collect();
    (matches, grouped)
}

fn parse_grep_line(line: &str) -> Option<ParsedLine> {
    if line.is_empty() || line == "--" {
        return None;
    }
    let colon = split_numbered_line(line, ':');
    let dash = split_numbered_line(line, '-');
    match (colon, dash) {
        (None, None) => Some(ParsedLine {
            file: String::new(),
            line: 0,
            content: line.into(),
            is_match: true,
        }),
        (Some(mut value), None) => {
            value.is_match = true;
            Some(value)
        }
        (None, Some(mut value)) => {
            value.is_match = false;
            Some(value)
        }
        (Some(mut colon), Some(mut dash)) => {
            let colon_index = colon.file.len() + colon.line.to_string().len() + 1;
            let dash_index = dash.file.len() + dash.line.to_string().len() + 1;
            if colon_index <= dash_index {
                colon.is_match = true;
                Some(colon)
            } else {
                dash.is_match = false;
                Some(dash)
            }
        }
    }
}

fn split_numbered_line(line: &str, separator: char) -> Option<ParsedLine> {
    for (first, _) in line.match_indices(separator) {
        let rest = &line[first + separator.len_utf8()..];
        let Some(second_rel) = rest.find(separator) else {
            continue;
        };
        if let Ok(number) = rest[..second_rel].parse() {
            return Some(ParsedLine {
                file: line[..first].into(),
                line: number,
                content: rest[second_rel + separator.len_utf8()..].into(),
                is_match: false,
            });
        }
    }
    None
}

fn print_grep_grouped(files: &[Value], out: Output) -> Result<()> {
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.stdout("\n")?;
        }
        out.stdout(&format!(
            "===== {} ({}) =====\n",
            file["file"], file["count"]
        ))?;
        for item in file["matches"].as_array().into_iter().flatten() {
            for context in item["before"].as_array().into_iter().flatten() {
                out.stdout(&format!("{}- {}\n", context["line"], context["content"]))?;
            }
            out.stdout(&format!("{}: {}\n", item["line"], item["content"]))?;
            for context in item["after"].as_array().into_iter().flatten() {
                out.stdout(&format!("{}- {}\n", context["line"], context["content"]))?;
            }
        }
    }
    Ok(())
}

fn find(args: cli::FindArgs, out: Output) -> Result<i32> {
    if is_windows(&args.host.host)? {
        return windows_unsupported("find", out);
    }
    let path = normalize(&args.path);
    let cwd = normalize_optional(&args.cwd);
    let mut parts = vec![
        "find".into(),
        "-L".into(),
        transport::expand_tilde(&path),
        "-name".into(),
        transport::shell_quote(&args.name),
    ];
    if !args.file_type.is_empty() {
        parts.extend(["-type".into(), args.file_type]);
    }
    let result = transport::run_command(
        &transport::wrap_remote_cwd(&parts.join(" "), &cwd),
        &args.host.host,
        30,
        "",
        None,
    )?;
    if !result.success() {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    if out.json {
        let files = result.stdout.trim().lines().filter(|line| !line.is_empty()).map(|line| {
            json!({"path": line, "name": std::path::Path::new(line).file_name().and_then(|v| v.to_str()).unwrap_or_default()})
        }).collect::<Vec<_>>();
        out.json(&json!({"name": args.name, "path": path, "cwd": nullable(&cwd), "count": files.len(), "files": files}))?;
    } else {
        out.stdout(&result.stdout)?;
    }
    Ok(0)
}

fn head_tail(kind: &str, args: cli::HeadTailArgs, out: Output) -> Result<i32> {
    let path = normalize(&args.path);
    let cwd = normalize_optional(&args.cwd);
    let command = if is_windows(&args.host.host)? {
        build_windows_head_tail_cmd(kind, &path, &cwd, args.lines)
    } else {
        transport::wrap_remote_cwd(
            &format!(
                "{kind} -n {} {}",
                args.lines,
                transport::expand_tilde(&path)
            ),
            &cwd,
        )
    };
    let result = transport::run_command(&command, &args.host.host, 30, "", None)?;
    if !result.success() {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    if out.json {
        let mut payload = json!({"cwd": nullable(&cwd), "lines": args.lines, "content": result.stdout, "size": result.stdout.len()});
        payload[if kind == "tail" { "file" } else { "path" }] = Value::String(path);
        out.json(&payload)?;
    } else {
        out.stdout(&result.stdout)?;
    }
    Ok(0)
}

fn build_windows_head_tail_cmd(kind: &str, path: &str, cwd: &str, lines: i32) -> String {
    let mut steps = vec![windows_path_resolver_script().into()];
    if !cwd.is_empty() {
        steps.push(format!(
            "Set-Location -LiteralPath (Resolve-DevPath {})",
            transport::powershell_quote(cwd)
        ));
    }
    steps.push(format!(
        r#"
$target = Resolve-DevPath {}
$content = @(Get-Content -LiteralPath $target {} {lines} -ErrorAction Stop)
foreach ($line in $content) {{ [Console]::Out.WriteLine([string]$line) }}
"#,
        transport::powershell_quote(path),
        if kind == "tail" {
            "-Tail"
        } else {
            "-TotalCount"
        }
    ));
    transport::powershell_command(&steps.join("\n"))
}

fn tree(args: cli::TreeArgs, out: Output) -> Result<i32> {
    if is_windows(&args.host.host)? {
        return windows_unsupported("tree", out);
    }
    let path = normalize(&args.path);
    let cwd = normalize_optional(&args.cwd);
    let command = transport::wrap_remote_cwd(
        &format!(
            "find -L {} -maxdepth {} -type f -o -type d | head -100",
            transport::expand_tilde(&path),
            args.depth
        ),
        &cwd,
    );
    let result = transport::run_command(&command, &args.host.host, 30, "", None)?;
    if !result.success() {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    if out.json {
        let items = result.stdout.trim().lines().filter(|line| !line.trim().is_empty()).map(|line| json!({
            "path": line, "name": std::path::Path::new(line.trim_end_matches('/')).file_name().and_then(|v| v.to_str()).unwrap_or_default(),
            "type": if line.ends_with('/') { "directory" } else { "file" },
        })).collect::<Vec<_>>();
        out.json(&json!({"path": path, "cwd": nullable(&cwd), "depth": args.depth, "count": items.len(), "items": items}))?;
    } else {
        print_tree(&result.stdout, &path, out)?;
    }
    Ok(0)
}

fn print_tree(raw: &str, base: &str, out: Output) -> Result<()> {
    let base_parts = base.trim_end_matches('/').split('/').count();
    for line in raw.trim().lines().filter(|line| !line.trim().is_empty()) {
        let parts = line.split('/').collect::<Vec<_>>();
        let relative = parts[base_parts.min(parts.len())..].join("/");
        if relative.is_empty() {
            out.stdout(&format!("{base}/\n"))?;
            continue;
        }
        let depth = relative.matches('/').count();
        let name = relative.rsplit('/').next().unwrap_or_default();
        out.stdout(&format!(
            "{}{}{}\n",
            "  ".repeat(depth),
            name,
            if line.ends_with('/') { "/" } else { "" }
        ))?;
    }
    Ok(())
}

fn slice(args: cli::SliceArgs, out: Output) -> Result<i32> {
    if is_windows(&args.host.host)? {
        return windows_unsupported("slice", out);
    }
    let selectors = [&args.range, &args.around, &args.r#match]
        .iter()
        .filter(|value| !value.is_empty())
        .count();
    if selectors != 1 {
        bail!("--range、--around、--match 必须且只能指定一个");
    }
    if args.lines < 1 {
        bail!("--lines 必须大于 0");
    }
    if args.context.is_some_and(|value| value < 0) {
        bail!("--context 不能小于 0");
    }
    let path = normalize(&args.path);
    let cwd = normalize_optional(&args.cwd);
    let result = transport::run_command(
        &build_slice_cmd(
            &path,
            &cwd,
            &args.range,
            &args.around,
            &args.r#match,
            args.lines,
            args.context,
        ),
        &args.host.host,
        30,
        "",
        None,
    )?;
    let payload: Value = match serde_json::from_str(&result.stdout) {
        Ok(value) => value,
        Err(_) => {
            if result.stderr.is_empty() {
                out.stderr("错误: 无法解析远程 slice 输出\n")?;
            } else {
                out.stderr(&format!("错误: {}", result.stderr))?;
            }
            return Ok(fallback_code(result.return_code));
        }
    };
    if out.json {
        out.json(&payload)?;
    } else {
        print_slice_plain(&payload, !args.no_line_number, out)?;
        out.stderr(&result.stderr)?;
    }
    Ok(if result.success() {
        0
    } else {
        result.return_code
    })
}

fn build_slice_cmd(
    path: &str,
    cwd: &str,
    range: &str,
    around: &str,
    match_text: &str,
    lines: i32,
    context: Option<i32>,
) -> String {
    let script = format!(
        r#"
import json
import os
from pathlib import Path
import sys
path = {}
line_range = {}
around = {}
match = {}
window_lines = {lines}
context = {}
def fail(message):
    print(json.dumps({{"path": path, "cwd": os.getcwd(), "success": False, "error": message}}, ensure_ascii=False))
    sys.exit(1)
try:
    text = Path(os.path.expanduser(path)).read_text(errors="replace")
except Exception as exc:
    fail(str(exc))
all_lines = text.splitlines()
total = len(all_lines)
matched_line = None
selector = {{}}
if line_range is not None:
    selector = {{"type": "range", "value": line_range}}
    raw = line_range.strip()
    if ":" not in raw: fail("--range must use START:END")
    left, right = raw.split(":", 1)
    try:
        start = int(left) if left else 1
        end = int(right) if right else total
    except ValueError: fail("--range bounds must be integers")
    if start < 1 or end < start: fail("--range must satisfy 1 <= START <= END")
else:
    needle = around if around is not None else match
    selector = {{"type": "around" if around is not None else "match", "value": needle}}
    for index, line in enumerate(all_lines, start=1):
        if needle in line:
            matched_line = index
            break
    if matched_line is None: fail(f"pattern not found: {{needle}}")
    if context is not None:
        start = matched_line - context
        end = matched_line + context
    else:
        before = max((window_lines - 1) // 2, 0)
        after = max(window_lines - before - 1, 0)
        start = matched_line - before
        end = matched_line + after
start = max(start, 1)
end = min(end, total)
items = [{{"number": number, "text": all_lines[number - 1]}} for number in range(start, end + 1)]
content = "\n".join(item["text"] for item in items)
if items and text.endswith("\n") and end == total: content += "\n"
print(json.dumps({{"path": path, "cwd": os.getcwd(), "selector": selector, "start": start, "end": end, "total_lines": total, "matched_line": matched_line, "content": content, "lines": items, "count": len(items), "success": True, "error": ""}}, ensure_ascii=False))
"#,
        py_string(path),
        py_string_or_none(range),
        py_string_or_none(around),
        py_string_or_none(match_text),
        context.map_or_else(|| "None".into(), |value| value.to_string())
    );
    let mut steps = vec!["set -e".into()];
    if !cwd.is_empty() {
        steps.push(format!("cd {}", transport::quote_remote_path(cwd)));
    }
    steps.extend(["python3 - <<'PY'".into(), script.trim().into(), "PY".into()]);
    steps.join("\n")
}

fn print_slice_plain(payload: &Value, line_number: bool, out: Output) -> Result<()> {
    if !payload["success"].as_bool().unwrap_or(false) {
        out.stderr(&format!("错误: {}\n", payload["error"]))?;
        return Ok(());
    }
    if !line_number {
        out.stdout(payload["content"].as_str().unwrap_or_default())?;
        return Ok(());
    }
    let end = payload["end"].as_i64().unwrap_or(0);
    let width = end.to_string().len().max(1);
    for item in payload["lines"].as_array().into_iter().flatten() {
        out.stdout(&format!(
            "{:>width$} | {}\n",
            item["number"].as_i64().unwrap_or(0),
            item["text"].as_str().unwrap_or_default()
        ))?;
    }
    Ok(())
}

fn write(args: cli::WriteArgs, out: Output) -> Result<i32> {
    let value = args.content.unwrap_or(crate::commands::read_stdin()?);
    let path = normalize(&args.path);
    let cwd = normalize_optional(&args.cwd);
    let windows = is_windows(&args.host.host)?;
    let (command, stdin) = if windows {
        (
            build_windows_write_cmd(&path, &cwd, args.append),
            Some(value.as_str()),
        )
    } else {
        let operator = if args.append { ">>" } else { ">" };
        (
            transport::wrap_remote_cwd(
                &format!(
                    "cat {operator} {} << 'DEV_CONNECT_EOF'\n{value}\nDEV_CONNECT_EOF",
                    transport::expand_tilde(&path)
                ),
                &cwd,
            ),
            None,
        )
    };
    let result = transport::run_command(&command, &args.host.host, 30, "", stdin)?;
    if !result.success() {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    out.stdout(&format!(
        "已{}: {path} ({} 字节)\n",
        if args.append { "追加" } else { "写入" },
        value.len()
    ))?;
    Ok(0)
}

fn build_windows_write_cmd(path: &str, cwd: &str, append: bool) -> String {
    let mut steps = vec![windows_path_resolver_script().into()];
    if !cwd.is_empty() {
        steps.push(format!(
            "Set-Location -LiteralPath (Resolve-DevPath {})",
            transport::powershell_quote(cwd)
        ));
    }
    steps.push(format!(
        r#"
$target = Resolve-DevPath {}
$content = [Console]::In.ReadToEnd()
$encoding = New-Object System.Text.UTF8Encoding -ArgumentList $false
[System.IO.File]::{}($target, $content, $encoding)
"#,
        transport::powershell_quote(path),
        if append {
            "AppendAllText"
        } else {
            "WriteAllText"
        }
    ));
    transport::powershell_command(&steps.join("\n"))
}
fn edit(args: cli::EditArgs, out: Output) -> Result<i32> {
    let (host, action, path, params, success) = match args.command {
        cli::EditCommand::Replace(args) => {
            let scope = if args.all { "所有" } else { "首次" };
            let message = format!("已替换 {scope}匹配: '{}' -> '{}'\n", args.old, args.new);
            (
                args.host.host,
                "replace",
                normalize(&args.path),
                json!({"old": args.old, "new": args.new, "all": args.all}),
                message,
            )
        }
        cli::EditCommand::Insert(args) => {
            let position = if args.after { "后" } else { "前" };
            let message = format!("已在第 {} 行{position}插入内容\n", args.line);
            (
                args.host.host,
                "insert",
                normalize(&args.path),
                json!({"line": args.line, "content": args.content, "after": args.after}),
                message,
            )
        }
        cli::EditCommand::Delete(args) => {
            let end = args.end.unwrap_or(args.start);
            let message = if args.end.is_some() {
                format!("已删除第 {}-{end} 行\n", args.start)
            } else {
                format!("已删除第 {} 行\n", args.start)
            };
            (
                args.host.host,
                "delete",
                normalize(&args.path),
                json!({"start": args.start, "end": if end == args.start { args.start.to_string() } else { format!("{},{end}", args.start) }}),
                message,
            )
        }
        cli::EditCommand::Line(args) => {
            let message = format!("已修改第 {} 行\n", args.line);
            (
                args.host.host,
                "line",
                normalize(&args.path),
                json!({"line": args.line, "content": args.content}),
                message,
            )
        }
    };
    if is_windows(&host)? {
        return windows_unsupported("edit", out);
    }
    let result =
        transport::run_command(&build_edit_cmd(action, &path, &params), &host, 30, "", None)?;
    if !result.success() {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    out.stdout(&success)?;
    Ok(0)
}

fn build_edit_cmd(action: &str, path: &str, params: &Value) -> String {
    let script = format!(
        r#"
import json
import os
from pathlib import Path
action = {}
path = {}
params = json.loads({})
target = Path(os.path.expanduser(path))
text = target.read_text()
if action == "replace":
    count = -1 if params["all"] else 1
    text = text.replace(params["old"], params["new"], count)
elif action == "insert":
    lines = text.splitlines(keepends=True)
    line = int(params["line"])
    if 1 <= line <= len(lines):
        index = line if params["after"] else line - 1
        content = params["content"]
        if not content.endswith("\n"): content += "\n"
        lines.insert(index, content)
        text = "".join(lines)
elif action == "delete":
    lines = text.splitlines(keepends=True)
    start = int(params["start"])
    raw_end = str(params["end"])
    end = int(raw_end.split(",", 1)[1]) if "," in raw_end else start
    if start <= len(lines) and end >= start:
        del lines[max(start - 1, 0):min(end, len(lines))]
        text = "".join(lines)
elif action == "line":
    lines = text.splitlines(keepends=True)
    line = int(params["line"])
    if 1 <= line <= len(lines):
        newline = "\n" if lines[line - 1].endswith("\n") else ""
        lines[line - 1] = params["content"] + newline
        text = "".join(lines)
target.write_text(text)
"#,
        py_string(action),
        py_string(path),
        py_string(&params.to_string())
    );
    format!("python3 - <<'PY'\n{}\nPY", script.trim())
}

fn diff(args: cli::DiffArgs, out: Output) -> Result<i32> {
    if is_windows(&args.host.host)? {
        return windows_unsupported("diff", out);
    }
    let file1 = normalize(&args.file1);
    if args.local {
        let temp = tempfile::NamedTempFile::new()?;
        let result = transport::download(
            &file1,
            temp.path().to_string_lossy().as_ref(),
            &args.host.host,
            60,
        )?;
        if !result.success() {
            out.stderr(&format!("错误: 下载远程文件失败: {}", result.stderr))?;
            return Ok(1);
        }
        let diff = std::process::Command::new("diff")
            .args(["-u", temp.path().to_string_lossy().as_ref(), &args.file2])
            .output()?;
        let text = String::from_utf8_lossy(&diff.stdout).into_owned();
        if out.json {
            out.json(&json!({"remote_file": file1, "local_file": args.file2, "diff": text, "has_changes": !text.is_empty()}))?;
        } else if text.is_empty() {
            out.stdout("文件相同\n")?;
        } else {
            out.stdout(&text)?;
        }
        return Ok(0);
    }
    let result = transport::run_command(
        &format!(
            "diff -u {} {} || true",
            transport::expand_tilde(&file1),
            transport::expand_tilde(&args.file2)
        ),
        &args.host.host,
        30,
        "",
        None,
    )?;
    if !result.success() {
        out.stderr(&format!("错误: {}", result.stderr))?;
        return Ok(1);
    }
    if out.json {
        out.json(&json!({"file1": file1, "file2": args.file2, "diff": result.stdout, "has_changes": !result.stdout.is_empty()}))?;
    } else if result.stdout.is_empty() {
        out.stdout("文件相同\n")?;
    } else {
        out.stdout(&result.stdout)?;
    }
    Ok(0)
}
fn repo_status(args: cli::RepoCwdArgs, out: Output) -> Result<i32> {
    if is_windows(&args.host.host)? {
        return windows_unsupported("repo-status", out);
    }
    let cwd = normalize(&args.cwd);
    let command = python_in_repo(&cwd, include_str!("scripts/repo_status.py"));
    let result = transport::run_command(&command, &args.host.host, 30, "", None)?;
    let payload = parse_json_result(&result.stdout, &cwd);
    if out.json {
        out.json(&payload)?;
    } else {
        if !payload["success"].as_bool().unwrap_or(false) {
            out.stderr(&format!(
                "{}\n",
                value_text(&payload["error"], "status failed")
            ))?;
        } else {
            out.stdout(&format!(
                "repo: {}\nbranch: {}\ndirty: {}\n",
                payload["repo_root"], payload["branch"], payload["dirty"]
            ))?;
            if let Some(status) = non_empty_str(&payload["status"]) {
                out.stdout(&format!("\nstatus:\n{status}"))?;
            }
            if let Some(stat) = non_empty_str(&payload["diff_stat"]) {
                out.stdout(&format!("\ndiff stat:\n{stat}"))?;
            }
        }
        out.stderr(&result.stderr)?;
    }
    Ok(if result.success() {
        0
    } else {
        result.return_code
    })
}

fn repo_diff(args: cli::RepoDiffArgs, out: Output) -> Result<i32> {
    if is_windows(&args.host.host)? {
        return windows_unsupported("repo-diff", out);
    }
    if args.stat && args.name_only {
        bail!("--stat 和 --name-only 不能同时使用");
    }
    let cwd = normalize(&args.cwd);
    let mut parts = vec!["git", "diff"];
    if args.cached {
        parts.push("--cached");
    }
    if args.stat {
        parts.push("--stat");
    }
    if args.name_only {
        parts.push("--name-only");
    }
    let command = format!(
        "set -e\ncd {}\n{}",
        transport::quote_remote_path(&cwd),
        parts.join(" ")
    );
    let result = transport::run_command(&command, &args.host.host, 30, "", None)?;
    if out.json {
        out.json(&json!({"cwd": cwd, "cached": args.cached, "stat": args.stat, "name_only": args.name_only, "returncode": result.return_code, "stdout": result.stdout, "stderr": result.stderr, "success": result.success()}))?;
    } else {
        out.stdout(&result.stdout)?;
        out.stderr(&result.stderr)?;
    }
    Ok(if result.success() {
        0
    } else {
        result.return_code
    })
}

fn git_snapshot(args: cli::RepoCwdArgs, out: Output) -> Result<i32> {
    if is_windows(&args.host.host)? {
        return windows_unsupported("git-snapshot", out);
    }
    let cwd = normalize(&args.cwd);
    let result = transport::run_command(
        &python_in_repo(&cwd, include_str!("scripts/git_snapshot.py")),
        &args.host.host,
        30,
        "",
        None,
    )?;
    let payload = parse_json_result(&result.stdout, &cwd);
    if out.json {
        out.json(&payload)?;
    } else {
        if !payload["success"].as_bool().unwrap_or(false) {
            out.stderr(&format!(
                "{}\n",
                value_text(&payload["error"], "snapshot failed")
            ))?;
        } else {
            out.stdout(&format!(
                "repo: {}\nbranch: {}\nhead: {} {}\nhead full: {}\ndirty: {}\n",
                payload["repo_root"],
                payload["branch"],
                payload["head"],
                payload["head_subject"],
                payload["head_full"],
                payload["dirty"]
            ))?;
            if let Some(origin) = non_empty_str(&payload["origin_url"]) {
                out.stdout(&format!("origin: {origin}\n"))?;
            } else if let Some(error) = non_empty_str(&payload["origin_error"]) {
                out.stdout(&format!("origin: unavailable ({error})\n"))?;
            }
            for section in ["status", "diff_stat", "staged_diff_stat"] {
                if let Some(value) = non_empty_str(&payload[section]) {
                    out.stdout(&format!("\n{}:\n{value}", section.replace('_', " ")))?;
                }
            }
            out.stdout("\nverification: not_run\n")?;
        }
        out.stderr(&result.stderr)?;
    }
    Ok(if result.success() {
        0
    } else {
        result.return_code
    })
}

fn repo(args: cli::RepoArgs, out: Output) -> Result<i32> {
    match args.command {
        cli::RepoCommand::Resolve(args) => repo_resolve(&args.repo, &args.host.host, out),
    }
}

fn repo_resolve(repo: &str, host_alias: &str, out: Output) -> Result<i32> {
    let host = config::get_host(host_alias)?;
    if model::is_windows(host.os.as_deref()) {
        return windows_unsupported("repo resolve", out);
    }
    if !repo.starts_with('/') && !repo.starts_with("~/") && host.repo_roots.is_empty() {
        bail!(
            "主机未配置 repo_roots，无法解析 '{repo}'。请先运行: dev config add-repo-root <ALIAS> <ROOT>"
        );
    }
    let roots = serde_json::to_string(&host.repo_roots)?;
    let command = format!(
        "python3 - {} {} <<'PY'\n{}\nPY",
        transport::quote_remote_path(repo),
        transport::quote_remote_path(&roots),
        include_str!("scripts/repo_resolve.py").trim()
    );
    let result = transport::run_command(&command, host_alias, 30, "", None)?;
    let payload = parse_json_result(&result.stdout, repo);
    if out.json {
        out.json(&payload)?;
    } else if payload["success"].as_bool().unwrap_or(false) {
        out.stdout(&format!(
            "{}\n",
            payload["path"].as_str().unwrap_or_default()
        ))?;
    } else {
        out.stderr(&format!(
            "{}\n",
            value_text(&payload["error"], "repo not found")
        ))?;
        if let Some(roots) = payload["searched_roots"]
            .as_array()
            .filter(|roots| !roots.is_empty())
        {
            out.stderr(&format!(
                "searched roots: {}\n",
                roots
                    .iter()
                    .map(|value| value.as_str().unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))?;
        }
    }
    Ok(if payload["success"].as_bool().unwrap_or(false) {
        0
    } else {
        1
    })
}

fn verify(args: cli::VerifyArgs, out: Output) -> Result<i32> {
    match args.command {
        cli::VerifyCommand::Go(args) => verify_go(args, out),
    }
}

fn verify_go(args: cli::VerifyGoArgs, out: Output) -> Result<i32> {
    if is_windows(&args.host.host)? {
        return windows_unsupported("verify go", out);
    }
    if !args.changed {
        bail!("当前只支持 --changed");
    }
    let cwd = normalize(&args.cwd);
    let also = serde_json::to_string(&args.also)?;
    let script = include_str!("scripts/verify_go.py").replace(
        "__ALSO_PACKAGES__",
        &also.replace('\\', "\\\\").replace('"', "\\\""),
    );
    let result = transport::run_command(
        &python_in_repo(&cwd, &script),
        &args.host.host,
        args.timeout,
        "",
        None,
    )?;
    let payload = parse_command_payload(&result.stdout, &cwd, result.return_code, &result.stderr);
    if out.json {
        out.json(&payload)?;
    } else {
        if payload["skipped"].as_bool().unwrap_or(false) {
            out.stdout(&format!("verify go skipped: {}\n", payload["reason"]))?;
        } else {
            out.stdout(&format!("verify go: {}\n", payload["command"]))?;
            if let Some(packages) = payload["packages"]
                .as_array()
                .filter(|values| !values.is_empty())
            {
                out.stdout("packages:\n")?;
                for package in packages {
                    out.stdout(&format!("  {}\n", package.as_str().unwrap_or_default()))?;
                }
            }
            out.stdout(payload["stdout"].as_str().unwrap_or_default())?;
        }
        out.stderr(payload["stderr"].as_str().unwrap_or_default())?;
    }
    Ok(if result.success() {
        0
    } else {
        result.return_code
    })
}

fn python_in_repo(cwd: &str, script: &str) -> String {
    format!(
        "set -e\ncd {}\npython3 - <<'PY'\n{}\nPY",
        transport::quote_remote_path(cwd),
        script.trim()
    )
}

fn parse_command_payload(stdout: &str, cwd: &str, return_code: i32, stderr: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|_| json!({"success": return_code == 0, "cwd": cwd, "returncode": return_code, "stdout": stdout, "stderr": stderr}))
}

fn non_empty_str(value: &Value) -> Option<&str> {
    value.as_str().filter(|value| !value.is_empty())
}

fn value_text<'a>(value: &'a Value, fallback: &'a str) -> &'a str {
    non_empty_str(value).unwrap_or(fallback)
}
fn exec_watch(args: cli::ExecWatchArgs, out: Output) -> Result<i32> {
    let stdin = read_optional_stdin(args.stdin, args.stdin_file)?;
    exec_watch_values(
        &args.command,
        &args.host.host,
        args.interval,
        args.timeout,
        &args.shell,
        args.summary_chars,
        &normalize_optional(&args.cwd),
        stdin.as_deref(),
        out,
    )
}
#[allow(clippy::too_many_arguments)]
fn exec_watch_values(
    command: &str,
    host: &str,
    interval: i32,
    timeout: i32,
    shell: &str,
    summary_chars: i32,
    cwd: &str,
    stdin: Option<&str>,
    out: Output,
) -> Result<i32> {
    if is_windows(host)? {
        return windows_unsupported("exec-watch", out);
    }
    if interval <= 0 {
        bail!("--interval 必须大于 0");
    }
    if timeout <= 0 {
        bail!("--timeout 必须大于 0");
    }
    let host_config = config::get_host(host)?;
    let active_shell = if shell == "none" {
        String::new()
    } else if shell.is_empty() {
        host_config.shell.unwrap_or_default()
    } else {
        shell.into()
    };
    let effective = transport::wrap_remote_cwd(command, cwd);
    let remote = build_watch_cmd(
        &effective,
        interval,
        timeout,
        &active_shell,
        summary_chars,
        cwd,
        stdin,
    );
    let args = transport::ssh_command_args(host, &remote)?;
    let mut process = ProcessCommand::new(&args[0])
        .args(&args[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("执行失败")?;
    let stdout = process.stdout.take().expect("stdout piped");
    let mut final_return_code = 1;
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        let payload: Value = match serde_json::from_str(&line) {
            Ok(payload) => payload,
            Err(_) => {
                out.stdout(&format!("{line}\n"))?;
                continue;
            }
        };
        if payload["event"] == "finished" {
            final_return_code = payload["returncode"].as_i64().unwrap_or(1) as i32;
        }
        output_watch_event(&payload, out)?;
    }
    let mut stderr = String::new();
    process
        .stderr
        .take()
        .expect("stderr piped")
        .read_to_string(&mut stderr)?;
    let status = process.wait()?;
    out.stderr(&stderr)?;
    if !status.success() {
        return Ok(status.code().unwrap_or(1));
    }
    Ok(final_return_code)
}

fn build_watch_cmd(
    command: &str,
    interval: i32,
    timeout: i32,
    shell: &str,
    summary_chars: i32,
    cwd: &str,
    stdin: Option<&str>,
) -> String {
    let script = include_str!("scripts/watch.py")
        .replace("__COMMAND__", &py_string(command))
        .replace("__INTERVAL__", &interval.to_string())
        .replace("__TIMEOUT__", &timeout.to_string())
        .replace("__SHELL__", &py_string_or_none(shell))
        .replace("__SUMMARY_CHARS__", &summary_chars.to_string())
        .replace("__CWD__", &py_string_or_none(cwd))
        .replace("__STDIN__", &stdin.map_or_else(|| "None".into(), py_string));
    format!("python3 - <<'PY'\n{}\nPY", script.trim())
}

fn output_watch_event(payload: &Value, out: Output) -> Result<()> {
    if out.json {
        out.stdout(&format!("{}\n", serde_json::to_string(payload)?))?;
        return Ok(());
    }
    match payload["event"].as_str() {
        Some("started") => out.stdout(&format!(
            "started: {}\n",
            payload["command"].as_str().unwrap_or_default()
        ))?,
        Some("running") => out.stdout(&format!(
            "[{}s] running lines={} last={}\n",
            payload["elapsed_seconds"],
            payload["output_lines"],
            payload["last_line"].as_str().unwrap_or_default()
        ))?,
        Some("finished") => {
            let status = if payload["timed_out"].as_bool().unwrap_or(false) {
                "timed out"
            } else if payload["success"].as_bool().unwrap_or(false) {
                "ok"
            } else {
                "failed"
            };
            out.stdout(&format!(
                "finished: {status} rc={} elapsed={}s lines={}\n",
                payload["returncode"], payload["elapsed_seconds"], payload["output_lines"]
            ))?;
            out.stdout(payload["output"].as_str().unwrap_or_default())?;
        }
        _ => {}
    }
    Ok(())
}
fn patch(args: cli::PatchArgs, out: Output) -> Result<i32> {
    if is_windows(&args.host.host)? {
        return windows_unsupported("patch", out);
    }
    let cwd = normalize(&args.cwd);
    let raw = crate::commands::read_stdin()?;
    let result = transport::run_command(
        &build_patch_cmd(&cwd, args.check),
        &args.host.host,
        120,
        "",
        Some(&raw),
    )?;
    let mut payload = parse_json_result(&result.stdout, &cwd);
    if let Some(object) = payload.as_object_mut() {
        object.insert("cwd".into(), Value::String(cwd.clone()));
        object.insert("check_only".into(), Value::Bool(args.check));
        object.insert("returncode".into(), Value::from(result.return_code));
        object.insert("stdout".into(), Value::String(result.stdout.clone()));
        object.insert("stderr".into(), Value::String(result.stderr.clone()));
    }
    if out.json {
        out.json(&payload)?;
    } else if !payload["success"].as_bool().unwrap_or(false) {
        let path = payload["path"].as_str().unwrap_or_default();
        out.stderr(&format!(
            "patch failed: {}{}\n",
            if path.is_empty() { "" } else { path },
            if path.is_empty() {
                value_text(&payload["error"], "unknown error").into()
            } else {
                format!(": {}", value_text(&payload["error"], "unknown error"))
            }
        ))?;
        print_patch_failure_details(&payload, out)?;
    } else {
        out.stdout(&format!(
            "patch {}: {cwd}\n",
            if args.check { "checked" } else { "applied" }
        ))?;
        if let Some(files) = payload["changed_files"]
            .as_array()
            .filter(|files| !files.is_empty())
        {
            out.stdout("changed files:\n")?;
            for item in files {
                out.stdout(&format!(
                    "  {}{}\n",
                    item["path"].as_str().unwrap_or_default(),
                    item["action"]
                        .as_str()
                        .map_or(String::new(), |action| format!(" [{action}]"))
                ))?;
            }
        }
        let (label, stat) = if let Some(stat) = non_empty_str(&payload["git_diff_stat"]) {
            ("git diff stat", stat)
        } else if let Some(stat) = non_empty_str(&payload["patch_stat"]) {
            (
                if args.check {
                    "patch stat (check-only, not git diff)"
                } else {
                    "patch stat"
                },
                stat,
            )
        } else {
            ("", "")
        };
        if !stat.is_empty() {
            out.stdout(&format!("{label}:\n{stat}"))?;
        }
    }
    if !out.json {
        out.stderr(&result.stderr)?;
    }
    Ok(if result.success() {
        0
    } else {
        result.return_code
    })
}

fn build_patch_cmd(cwd: &str, check: bool) -> String {
    format!(
        r#"set -e
cd {}
patch_file="$(mktemp)"
applier_file="$(mktemp)"
trap 'rm -f "$patch_file" "$applier_file"' EXIT
cat > "$patch_file"
cat > "$applier_file" <<'PYAPPLIER'
{}
PYAPPLIER
python3 "$applier_file" "$PWD" "$patch_file"{}"#,
        transport::quote_remote_path(cwd),
        include_str!("scripts/patch_applier.py").trim(),
        if check { " --check" } else { "" }
    )
}

fn print_patch_failure_details(payload: &Value, out: Output) -> Result<()> {
    let Some(details) = payload["details"].as_object() else {
        return Ok(());
    };
    if let Some(hunk) = details.get("hunk_index") {
        out.stderr(&format!("hunk: {hunk}\n"))?;
    }
    if let Some(lines) = details
        .get("match_lines")
        .and_then(Value::as_array)
        .filter(|lines| !lines.is_empty())
    {
        out.stderr(&format!("matched lines: {lines:?}\n"))?;
    }
    if let Some(candidates) = details
        .get("candidates")
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
    {
        out.stderr("similar candidates:\n")?;
        for candidate in candidates {
            out.stderr(&format!(
                "  line {}, score {}\n",
                candidate["start_line"], candidate["score"]
            ))?;
            for line in candidate["snippet"].as_array().into_iter().flatten() {
                out.stderr(&format!("    {}\n", line.as_str().unwrap_or_default()))?;
            }
        }
    }
    Ok(())
}
fn cg(args: Vec<String>, out: Output) -> Result<i32> {
    if args.first().is_some_and(|value| value == "install") {
        return cg_install(&args[1..], out);
    }
    let mut host = String::new();
    let mut cwd = String::new();
    let mut repo = String::new();
    let mut timeout = 300;
    let mut cg_args = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--" => {
                cg_args.extend_from_slice(&args[index + 1..]);
                break;
            }
            "--host" | "-H" => host = next_value(&args, &mut index)?.into(),
            "--cwd" => cwd = normalize(next_value(&args, &mut index)?),
            "--repo" => repo = next_value(&args, &mut index)?.into(),
            "--timeout" | "-t" => timeout = next_value(&args, &mut index)?.parse()?,
            value => cg_args.push(value.into()),
        }
        index += 1;
    }
    if cg_args.is_empty() {
        bail!("cg requires COMMAND");
    }
    if is_windows(&host)? {
        return windows_unsupported("cg", out);
    }
    if !repo.is_empty() {
        cwd = resolve_repo_path(&repo, &host)?;
    }
    if out.json && !cg_args.iter().any(|value| value == "--json") {
        cg_args.insert(0, "--json".into());
    }
    if !cwd.is_empty() {
        cg_args = inject_cg_path(cg_args, &cwd);
    }
    let result = transport::run_command(&build_cg_proxy_cmd(&cg_args), &host, timeout, "", None)?;
    out.stdout(&result.stdout)?;
    out.stderr(&result.stderr)?;
    Ok(if result.success() {
        0
    } else {
        fallback_code(result.return_code)
    })
}

fn cg_install(args: &[String], out: Output) -> Result<i32> {
    let mut host = String::new();
    let mut remote_dir = "~/.local/bin".to_string();
    let mut timeout = 120;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--host" | "-H" => host = next_value(args, &mut index)?.into(),
            "--remote-dir" => remote_dir = next_value(args, &mut index)?.into(),
            "--timeout" | "-t" => timeout = next_value(args, &mut index)?.parse()?,
            value => bail!("unknown flag {value} for command cg install"),
        }
        index += 1;
    }
    if is_windows(&host)? {
        return windows_unsupported("cg install", out);
    }
    let command = format!(
        "set -eu\ncurl --fail --location --silent --show-error https://raw.githubusercontent.com/DreamCats/cg-cli/main/scripts/install.sh | CG_INSTALL_DIR={} sh",
        transport::expand_tilde(&remote_dir)
    );
    let result = transport::run_command(&command, &host, timeout, "", None)?;
    if out.json {
        out.json(&json!({"success": result.success(), "remote_dir": remote_dir, "remote_path": format!("{remote_dir}/cg"), "returncode": result.return_code, "stdout": result.stdout, "stderr": result.stderr}))?;
    } else if result.success() {
        out.stdout(&format!(
            "已安装 cg: {}/cg\n{}",
            remote_dir.trim_end_matches('/'),
            result.stdout
        ))?;
    } else {
        out.stderr(&format!("错误: cg 安装后验证失败\n{}", result.stderr))?;
    }
    Ok(if result.success() {
        0
    } else {
        fallback_code(result.return_code)
    })
}

fn resolve_repo_path(repo: &str, host_alias: &str) -> Result<String> {
    let host = config::get_host(host_alias)?;
    if model::is_windows(host.os.as_deref()) {
        bail!("repo resolve is not supported for Windows hosts yet");
    }
    if !repo.starts_with('/') && !repo.starts_with("~/") && host.repo_roots.is_empty() {
        bail!(
            "主机未配置 repo_roots，无法解析 '{repo}'。请先运行: dev config add-repo-root <ALIAS> <ROOT>"
        );
    }
    let roots = serde_json::to_string(&host.repo_roots)?;
    let command = format!(
        "python3 - {} {} <<'PY'\n{}\nPY",
        transport::quote_remote_path(repo),
        transport::quote_remote_path(&roots),
        include_str!("scripts/repo_resolve.py").trim()
    );
    let result = transport::run_command(&command, host_alias, 30, "", None)?;
    let payload = parse_json_result(&result.stdout, repo);
    if !payload["success"].as_bool().unwrap_or(false) {
        bail!("{}", value_text(&payload["error"], "repo not found"));
    }
    payload["path"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("repo resolver returned empty path"))
}

fn build_cg_proxy_cmd(args: &[String]) -> String {
    format!(
        r#"if command -v cg >/dev/null 2>&1; then CG="$(command -v cg)"; elif [ -x "$HOME/.local/bin/cg" ]; then CG="$HOME/.local/bin/cg"; else echo "cg not found; run: dev cg install" >&2; exit 127; fi
"$CG" {}"#,
        args.iter()
            .map(|arg| transport::shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn inject_cg_path(mut args: Vec<String>, cwd: &str) -> Vec<String> {
    if args.iter().any(|value| {
        value == "--path"
            || value == "--target"
            || value.starts_with("--path=")
            || value.starts_with("--target=")
    }) {
        return args;
    }
    let mut skip = false;
    let command = args.iter().enumerate().find_map(|(index, value)| {
        if skip {
            skip = false;
            return None;
        }
        if value == "--json" || value == "--verbose" {
            return None;
        }
        if value == "--target" {
            skip = true;
            return None;
        }
        if value.starts_with('-') {
            return None;
        }
        Some((index, value.as_str()))
    });
    if command.is_some_and(|(_, command)| {
        matches!(
            command,
            "init"
                | "index"
                | "sync"
                | "query"
                | "files"
                | "status"
                | "resolve"
                | "callers"
                | "callees"
                | "impact"
                | "affected"
                | "context"
                | "overview"
                | "explore"
        )
    }) {
        args.extend(["--path".into(), cwd.into()]);
    }
    args
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use base64::{Engine, engine::general_purpose::STANDARD};

    use super::*;

    #[test]
    fn shell_join_quotes_only_when_needed() {
        assert_eq!(
            shell_join(&["go".into(), "test".into(), "./pkg with space".into()]),
            "go test './pkg with space'"
        );
    }

    #[test]
    fn parses_ls_payload() {
        let payload = parse_ls(
            "total 1\n-rw-r--r-- 1 user group 3 Jan 1 00:00 a b.txt\n",
            "~/",
        );
        assert_eq!(payload["count"], 1);
        assert_eq!(payload["items"][0]["name"], "a b.txt");
    }

    #[test]
    fn grep_builder_and_parser_match_reference_contract() {
        assert_eq!(
            build_grep_cmd(
                "hello world",
                "~/repo",
                true,
                "*.go",
                true,
                2,
                Some(3),
                true
            ),
            "rg -n -i -C 2 -m 3 --glob '*.go' 'hello world' ~/repo"
        );
        let (matches, files) = parse_grep_output(
            "a.py-1-before\na.py:2:match\na.py-3-after\n--\nb.py:10:hit\n",
            None,
        );
        assert_eq!(matches.len(), 2);
        assert_eq!(files.len(), 2);
        assert_eq!(matches[0]["line"], 2);
        assert_eq!(matches[0]["before"][0]["content"], "before");
        assert_eq!(matches[0]["after"][0]["content"], "after");
        let (windows, _) = parse_grep_output("C:\\repo\\app.go:12:match\n", None);
        assert_eq!(windows[0]["file"], "C:\\repo\\app.go");
        assert_eq!(windows[0]["line"], 12);
    }

    #[test]
    fn windows_builders_keep_powershell_contract() {
        for (command, expected) in [
            (
                build_windows_ls_cmd("%USERPROFILE%\\logs", "C:\\Work"),
                "Get-ChildItem",
            ),
            (
                build_windows_cat_cmd(&["~/a.txt".into()], "C:\\Work", false),
                "truncated=$true",
            ),
            (
                build_windows_head_tail_cmd("tail", "C:\\log.txt", "C:\\Work", 10),
                "-Tail 10",
            ),
            (
                build_windows_write_cmd("C:\\tmp\\a.txt", "C:\\Work", false),
                "WriteAllText",
            ),
        ] {
            let encoded = command.split_whitespace().last().unwrap();
            let raw = STANDARD.decode(encoded).unwrap();
            let words = raw
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>();
            let script = String::from_utf16(&words).unwrap();
            assert!(
                script.contains(expected),
                "script missing {expected:?}:\n{script}"
            );
        }
    }

    #[test]
    fn cg_path_injection_matches_reference() {
        assert_eq!(
            inject_cg_path(
                vec![
                    "--json".into(),
                    "context".into(),
                    "fix login".into(),
                    "--summary".into()
                ],
                "~/repo"
            ),
            vec![
                "--json",
                "context",
                "fix login",
                "--summary",
                "--path",
                "~/repo"
            ]
        );
        assert_eq!(inject_cg_path(vec!["list".into()], "~/repo"), vec!["list"]);
    }

    #[test]
    fn patch_applier_adds_updates_and_deletes() {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir(repo.path().join("src")).unwrap();
        fs::write(
            repo.path().join("src/app.py"),
            "def main():\n    return 1\n",
        )
        .unwrap();
        fs::write(repo.path().join("old.txt"), "delete me\n").unwrap();
        let mut script = tempfile::NamedTempFile::new().unwrap();
        script
            .write_all(include_str!("scripts/patch_applier.py").as_bytes())
            .unwrap();
        let mut patch = tempfile::NamedTempFile::new().unwrap();
        patch.write_all(b"*** Begin Patch\n*** Add File: src/new.py\n+value = 1\n*** Update File: src/app.py\n@@\n def main():\n-    return 1\n+    return 2\n*** Delete File: old.txt\n*** End Patch\n").unwrap();
        let output = ProcessCommand::new("python3")
            .args([script.path(), repo.path(), patch.path()])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(payload["success"], true);
        assert_eq!(
            fs::read_to_string(repo.path().join("src/app.py")).unwrap(),
            "def main():\n    return 2\n"
        );
        assert!(!repo.path().join("old.txt").exists());
    }
}
