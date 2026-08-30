---
name: dev-connect
description: "远程开发机文件交互 CLI。当用户需要查看远程目录、读取远程文件、传输文件、搜索代码、执行命令、查看日志、写入文件、编辑文件、比较文件差异时使用此 skill。"
---

# dev-connect 远程开发机交互

`dev` 是 Rust 实现的远程开发机 CLI，封装 SSH/SCP，并兼容
`~/.config/dev-cli/config.yaml`。

优先使用 JSON 输出给 Agent 消费：

```bash
dev --json ls ~/project
dev --json cat --cwd ~/project go.mod src/main.rs
dev repo-status --cwd ~/project --json
```

## 读取与搜索

```bash
dev ls [PATH] [--cwd CWD] [--host HOST]
dev cat PATH... [--cwd CWD] [--full] [--host HOST]
dev slice FILE --range START:END [--cwd CWD] [--host HOST]
dev slice FILE --around TEXT [--lines N] [--context N] [--cwd CWD]
dev head FILE [--lines N] [--cwd CWD]
dev tail FILE [--lines N] [--cwd CWD]
dev grep PATTERN [PATH] [--cwd CWD] [--include GLOB] [-i] [--context N] [--max-matches N] [--group]
dev find NAME [PATH] [--cwd CWD] [--type f|d]
dev tree [PATH] [--cwd CWD] [--depth N]
```

`cat` 默认单文件最多 256 KiB；大文件优先用 `slice/head/tail`。`grep`
优先远端 `rg`，否则降级 `grep`，并严格区分零命中与执行失败。

## 执行、传输和写入

```bash
dev exec [--host HOST] [--cwd CWD] [--timeout N] [--shell SHELL] -- COMMAND
dev exec HOST --cwd CWD -- COMMAND
dev exec --stdin - -- python3 -
dev exec --stdin-file script.py --watch --timeout 300 -- python3 -
dev exec-watch "go test ./..." --cwd ~/repo --interval 10 --timeout 300
dev push LOCAL REMOTE [--recursive]
dev pull REMOTE LOCAL
dev write PATH -c CONTENT [--cwd CWD] [--append]
dev edit replace PATH OLD NEW [--all]
dev edit insert PATH LINE CONTENT [--after]
dev edit delete PATH START [END]
dev edit line PATH NUM CONTENT
```

`exec --watch` / `exec-watch` 在 JSON 模式下每行输出一个事件对象。远端
执行本地脚本时用 `--stdin -` 或 `--stdin-file`，不要使用 base64/heredoc
多层转义。

## Git、Patch 和 cg

```bash
dev repo-status --cwd REPO --json
dev repo-diff --cwd REPO [--stat] [--cached] [--name-only]
dev git-snapshot --cwd REPO
dev repo resolve ORG/REPO
dev verify go --cwd REPO --changed [--also PKG]
dev patch --cwd REPO [--check] < changes.patch
dev cg install
dev cg init --cwd REPO --index
dev --json cg overview --cwd REPO
dev --json cg context --repo ORG/REPO "task" --summary
```

`repo resolve` 依赖主机 `repo_roots`。`patch` 使用 Codex 结构化格式并在
失败时返回 path、hunk 和相似候选。远端无 `cg` 时先执行 `dev cg install`。

## Windows OpenSSH

```bash
dev config add winhost <IP> --user Administrator --os windows --shell powershell
dev exec --host winhost -- hostname
dev ls --host winhost '~'
```

Windows 当前支持 `exec`、`ls`、`cat`、`head`、`tail`、`grep`、`write`；
其他 POSIX-heavy 命令会明确返回 unsupported。

## 配置与 Agent 注意事项

```bash
dev config show
dev config add ALIAS HOSTNAME [--user USER] [--os posix|windows] [--shell SHELL] [--exec-timeout N] [--repo-root ROOT] [--default]
dev config set-default ALIAS
dev config set-os ALIAS posix|windows
dev config set-shell ALIAS zsh|zsh-login|bash|bash-login|powershell|pwsh|cmd|none
dev config set-exec-timeout ALIAS N
dev config add-repo-root ALIAS ROOT
dev config clear-repo-roots ALIAS
dev stats suggest
dev history --limit 20
```

- 给 Agent 解析的结果加全局 `--json`。
- `exec` 后的 `--json` 可能属于远端命令，不会提升为全局 JSON。
- 长命令使用 watch；未知大文件先用结构化读取命令。
- `history` 只记录命令名、结果、耗时和可选 `DEV_SESSION_ID`，绝不记录
  参数、路径、输出或文件内容。
