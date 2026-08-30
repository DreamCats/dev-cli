use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "dev",
    version,
    about = "远程开发机文件交互 CLI，对 Agent 友好。",
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    /// JSON 格式输出，便于 Agent 解析
    #[arg(long, global = true)]
    pub(crate) json: bool,

    /// 显示完整错误栈
    #[arg(long, global = true)]
    pub(crate) verbose: bool,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// 列远程目录
    Ls(PathArgs),
    /// 读取一个或多个远程文件
    Cat(CatArgs),
    /// 查看本机脱敏命令历史
    History(HistoryArgs),
    /// 精准读取远程文件片段
    Slice(SliceArgs),
    /// 搜索远程代码内容，优先 rg，降级 grep
    Grep(GrepArgs),
    /// 按名称搜索远程文件
    Find(FindArgs),
    /// 显示远程目录树
    Tree(TreeArgs),
    /// 查看文件开头
    Head(HeadTailArgs),
    /// 查看文件末尾
    Tail(HeadTailArgs),
    /// 上传文件到远程主机
    Push(PushArgs),
    /// 从远程主机下载文件
    Pull(PullArgs),
    /// 执行远程命令，支持 host-first 和 --watch
    #[command(trailing_var_arg = true)]
    Exec(RawArgs),
    /// 低频观察远程长命令，JSON 模式输出事件流
    ExecWatch(ExecWatchArgs),
    /// 写入或追加远程文件
    Write(WriteArgs),
    /// 精确编辑远程文件
    Edit(EditArgs),
    /// 比较远程文件，或远程文件与本地文件
    Diff(DiffArgs),
    /// 应用 Codex 结构化 patch
    Patch(PatchArgs),
    /// 返回远程 Git 仓库状态快照
    RepoStatus(RepoCwdArgs),
    /// 输出远程 Git diff
    RepoDiff(RepoDiffArgs),
    /// 返回只读 review 快照
    GitSnapshot(RepoCwdArgs),
    /// 远程仓库辅助命令
    Repo(RepoArgs),
    /// 按语言/仓库类型执行范围化验证
    Verify(VerifyArgs),
    /// 远程 cg 索引和知识图谱查询
    #[command(trailing_var_arg = true)]
    Cg(RawArgs),
    /// 管理主机配置
    Config(ConfigArgs),
    /// 显示命令使用统计和优化建议
    Stats(StatsArgs),
    /// 显示版本
    Version,
    /// 检查 GitHub Release 并安装最新兼容版本
    Update {
        /// 只检查是否有新版本，不修改当前二进制
        #[arg(long)]
        check: bool,
    },
}

impl Command {
    pub(crate) fn tracking_name(&self) -> &'static str {
        match self {
            Self::Ls(_) => "ls",
            Self::Cat(_) => "cat",
            Self::History(_) => "",
            Self::Slice(_) => "slice",
            Self::Grep(_) => "grep",
            Self::Find(_) => "find",
            Self::Tree(_) => "tree",
            Self::Head(_) => "head",
            Self::Tail(_) => "tail",
            Self::Push(_) => "push",
            Self::Pull(_) => "pull",
            Self::Exec(_) => "exec",
            Self::ExecWatch(_) => "exec-watch",
            Self::Write(_) => "write",
            Self::Edit(value) => value.tracking_name(),
            Self::Diff(_) => "diff",
            Self::Patch(_) => "patch",
            Self::RepoStatus(_) => "repo-status",
            Self::RepoDiff(_) => "repo-diff",
            Self::GitSnapshot(_) => "git-snapshot",
            Self::Repo(_) => "repo resolve",
            Self::Verify(_) => "verify go",
            Self::Cg(_) => "cg",
            Self::Config(value) => value.tracking_name(),
            Self::Stats(_) => "stats",
            Self::Version => "version",
            Self::Update { .. } => "update",
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct RawArgs {
    #[arg(allow_hyphen_values = true)]
    pub(crate) args: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct HostArg {
    #[arg(long, short = 'H', default_value = "")]
    pub(crate) host: String,
}

#[derive(Debug, Args)]
pub(crate) struct PathArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, default_value = "")]
    pub(crate) cwd: String,
    #[arg(default_value = "~")]
    pub(crate) path: String,
}

#[derive(Debug, Args)]
pub(crate) struct CatArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, default_value = "")]
    pub(crate) cwd: String,
    #[arg(long)]
    pub(crate) full: bool,
    #[arg(required = true)]
    pub(crate) paths: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct HistoryArgs {
    #[arg(long, short = 'n', default_value = "20")]
    pub(crate) limit: usize,
}

#[derive(Debug, Args)]
pub(crate) struct PushArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, short = 'r')]
    pub(crate) recursive: bool,
    pub(crate) local: String,
    pub(crate) remote: String,
}

#[derive(Debug, Args)]
pub(crate) struct PullArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    pub(crate) remote: String,
    pub(crate) local: String,
}

#[derive(Debug, Args)]
pub(crate) struct GrepArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, default_value = "")]
    pub(crate) cwd: String,
    #[arg(long, short = 'g', default_value = "")]
    pub(crate) include: String,
    #[arg(long, short = 'i')]
    pub(crate) ignore_case: bool,
    #[arg(long, short = 'N')]
    pub(crate) no_line_number: bool,
    #[arg(long, short = 'C', default_value = "0")]
    pub(crate) context: i32,
    #[arg(long)]
    pub(crate) max_matches: Option<usize>,
    #[arg(long)]
    pub(crate) group: bool,
    pub(crate) pattern: String,
    #[arg(default_value = ".")]
    pub(crate) path: String,
}

#[derive(Debug, Args)]
pub(crate) struct FindArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, default_value = "")]
    pub(crate) cwd: String,
    #[arg(long, short = 't', default_value = "")]
    pub(crate) file_type: String,
    pub(crate) name: String,
    #[arg(default_value = ".")]
    pub(crate) path: String,
}

#[derive(Debug, Args)]
pub(crate) struct TreeArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, default_value = "")]
    pub(crate) cwd: String,
    #[arg(long, short = 'd', default_value = "3")]
    pub(crate) depth: i32,
    #[arg(default_value = "~")]
    pub(crate) path: String,
}

#[derive(Debug, Args)]
pub(crate) struct SliceArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, default_value = "")]
    pub(crate) cwd: String,
    #[arg(long, default_value = "")]
    pub(crate) range: String,
    #[arg(long, default_value = "")]
    pub(crate) around: String,
    #[arg(long, default_value = "")]
    pub(crate) r#match: String,
    #[arg(long, default_value = "80")]
    pub(crate) lines: i32,
    #[arg(long)]
    pub(crate) context: Option<i32>,
    #[arg(long)]
    pub(crate) no_line_number: bool,
    pub(crate) path: String,
}

#[derive(Debug, Args)]
pub(crate) struct HeadTailArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, default_value = "")]
    pub(crate) cwd: String,
    #[arg(long, short = 'n', default_value = "20")]
    pub(crate) lines: i32,
    pub(crate) path: String,
}

#[derive(Debug, Args)]
pub(crate) struct ExecWatchArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, default_value = "")]
    pub(crate) cwd: String,
    #[arg(long, default_value = "10")]
    pub(crate) interval: i32,
    #[arg(long, short = 't', default_value = "300")]
    pub(crate) timeout: i32,
    #[arg(long, default_value = "20000")]
    pub(crate) summary_chars: i32,
    #[arg(long, default_value = "")]
    pub(crate) shell: String,
    #[arg(long)]
    pub(crate) stdin: Option<String>,
    #[arg(long)]
    pub(crate) stdin_file: Option<PathBuf>,
    pub(crate) command: String,
}

#[derive(Debug, Args)]
pub(crate) struct WriteArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, default_value = "")]
    pub(crate) cwd: String,
    #[arg(long, short = 'c')]
    pub(crate) content: Option<String>,
    #[arg(long, short = 'a')]
    pub(crate) append: bool,
    pub(crate) path: String,
}

#[derive(Debug, Args)]
pub(crate) struct EditArgs {
    #[command(subcommand)]
    pub(crate) command: EditCommand,
}

impl EditArgs {
    fn tracking_name(&self) -> &'static str {
        match self.command {
            EditCommand::Replace(_) => "edit replace",
            EditCommand::Insert(_) => "edit insert",
            EditCommand::Delete(_) => "edit delete",
            EditCommand::Line(_) => "edit line",
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum EditCommand {
    Replace(EditReplaceArgs),
    Insert(EditInsertArgs),
    Delete(EditDeleteArgs),
    Line(EditLineArgs),
}

#[derive(Debug, Args)]
pub(crate) struct EditReplaceArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long)]
    pub(crate) all: bool,
    pub(crate) path: String,
    pub(crate) old: String,
    pub(crate) new: String,
}

#[derive(Debug, Args)]
pub(crate) struct EditInsertArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long)]
    pub(crate) after: bool,
    pub(crate) path: String,
    pub(crate) line: i32,
    pub(crate) content: String,
}

#[derive(Debug, Args)]
pub(crate) struct EditDeleteArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    pub(crate) path: String,
    pub(crate) start: i32,
    pub(crate) end: Option<i32>,
}

#[derive(Debug, Args)]
pub(crate) struct EditLineArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    pub(crate) path: String,
    pub(crate) line: i32,
    pub(crate) content: String,
}

#[derive(Debug, Args)]
pub(crate) struct DiffArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long, short = 'l')]
    pub(crate) local: bool,
    pub(crate) file1: String,
    pub(crate) file2: String,
}

#[derive(Debug, Args)]
pub(crate) struct PatchArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long)]
    pub(crate) cwd: String,
    #[arg(long)]
    pub(crate) check: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RepoCwdArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long)]
    pub(crate) cwd: String,
}

#[derive(Debug, Args)]
pub(crate) struct RepoDiffArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long)]
    pub(crate) cwd: String,
    #[arg(long)]
    pub(crate) stat: bool,
    #[arg(long)]
    pub(crate) cached: bool,
    #[arg(long)]
    pub(crate) name_only: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RepoArgs {
    #[command(subcommand)]
    pub(crate) command: RepoCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepoCommand {
    Resolve(RepoResolveArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepoResolveArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    pub(crate) repo: String,
}

#[derive(Debug, Args)]
pub(crate) struct VerifyArgs {
    #[command(subcommand)]
    pub(crate) command: VerifyCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum VerifyCommand {
    Go(VerifyGoArgs),
}

#[derive(Debug, Args)]
pub(crate) struct VerifyGoArgs {
    #[command(flatten)]
    pub(crate) host: HostArg,
    #[arg(long)]
    pub(crate) cwd: String,
    #[arg(long)]
    pub(crate) changed: bool,
    #[arg(long)]
    pub(crate) also: Vec<String>,
    #[arg(long, short = 't', default_value = "300")]
    pub(crate) timeout: i32,
}

#[derive(Debug, Args)]
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub(crate) command: ConfigCommand,
}

impl ConfigArgs {
    fn tracking_name(&self) -> &'static str {
        match self.command {
            ConfigCommand::Show => "config show",
            ConfigCommand::Add(_) => "config add",
            ConfigCommand::SetDefault(_) => "config set-default",
            ConfigCommand::SetOs(_) => "config set-os",
            ConfigCommand::SetShell(_) => "config set-shell",
            ConfigCommand::SetExecTimeout(_) => "config set-exec-timeout",
            ConfigCommand::AddRepoRoot(_) => "config add-repo-root",
            ConfigCommand::ClearRepoRoots(_) => "config clear-repo-roots",
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    Show,
    Add(ConfigAddArgs),
    SetDefault(ConfigAliasArgs),
    SetOs(ConfigSetValueArgs),
    SetShell(ConfigSetValueArgs),
    SetExecTimeout(ConfigSetTimeoutArgs),
    AddRepoRoot(ConfigSetValueArgs),
    ClearRepoRoots(ConfigAliasArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ConfigAddArgs {
    pub(crate) alias: String,
    pub(crate) hostname: String,
    #[arg(long, default_value = "")]
    pub(crate) user: String,
    #[arg(long)]
    pub(crate) os: Option<String>,
    #[arg(long)]
    pub(crate) shell: Option<String>,
    #[arg(long)]
    pub(crate) exec_timeout: Option<i32>,
    #[arg(long = "repo-root")]
    pub(crate) repo_roots: Vec<String>,
    #[arg(long = "default")]
    pub(crate) set_default: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ConfigAliasArgs {
    pub(crate) alias: String,
}

#[derive(Debug, Args)]
pub(crate) struct ConfigSetValueArgs {
    pub(crate) alias: String,
    pub(crate) value: String,
}

#[derive(Debug, Args)]
pub(crate) struct ConfigSetTimeoutArgs {
    pub(crate) alias: String,
    pub(crate) value: i32,
}

#[derive(Debug, Args)]
pub(crate) struct StatsArgs {
    #[command(subcommand)]
    pub(crate) command: Option<StatsCommand>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum StatsCommand {
    Suggest,
}

pub(crate) fn parse() -> Cli {
    Cli::parse()
}
