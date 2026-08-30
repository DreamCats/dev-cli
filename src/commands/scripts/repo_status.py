import json
import subprocess
import sys
from pathlib import Path

def run(args):
    r = subprocess.run(args, capture_output=True, text=True)
    return {"returncode": r.returncode, "stdout": r.stdout, "stderr": r.stderr}

def parse_status(text):
    staged, unstaged, untracked, entries = [], [], [], []
    for line in text.splitlines():
        if not line or line.startswith("## "):
            continue
        code = line[:2]
        raw_path = line[3:]
        path = raw_path.split(" -> ", 1)[-1] if " -> " in raw_path else raw_path
        entry = {"code": code, "path": path}
        entries.append(entry)
        if code == "??":
            untracked.append(entry)
            continue
        if code[0] != " ": staged.append(entry)
        if code[1] != " ": unstaged.append(entry)
    return entries, staged, unstaged, untracked

def collect_ahead_behind():
    upstream = run(["git", "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
    if upstream["returncode"] != 0:
        return "", {"ahead": None, "behind": None}, ""
    counts = run(["git", "rev-list", "--left-right", "--count", "@{u}...HEAD"])
    ahead_behind = {"ahead": None, "behind": None}
    if counts["returncode"] == 0:
        parts = counts["stdout"].split()
        if len(parts) == 2:
            ahead_behind = {"behind": int(parts[0]), "ahead": int(parts[1])}
    unpushed = run(["git", "log", "--oneline", "@{u}..HEAD"])
    return upstream["stdout"].strip(), ahead_behind, unpushed["stdout"] if unpushed["returncode"] == 0 else ""

def suspicious_reasons(path, code):
    reasons = []
    normalized = path.replace("\\", "/")
    name = normalized.rsplit("/", 1)[-1]
    build_dirs = ("node_modules/", "dist/", "build/", "coverage/", "target/", ".next/", ".turbo/")
    generated_suffixes = (".pb.go", ".gen.go", ".generated.go", "_generated.go", ".min.js", ".map")
    artifact_suffixes = (".log", ".tmp", ".out", ".test", ".o", ".so", ".dylib", ".class", ".pyc")
    if any(part in normalized for part in build_dirs): reasons.append("build-artifact")
    if name in {".DS_Store", "coverage.out"} or name.endswith(artifact_suffixes): reasons.append("artifact")
    if name.endswith(generated_suffixes): reasons.append("generated")
    size_bytes = None
    try:
        stat = Path(path).stat()
        size_bytes = stat.st_size
        if stat.st_size >= 5 * 1024 * 1024: reasons.append("large-file")
    except OSError:
        pass
    if code == "??" and reasons: reasons.insert(0, "untracked")
    return reasons, size_bytes

def collect_suspicious(entries):
    items = []
    for entry in entries:
        reasons, size_bytes = suspicious_reasons(entry["path"], entry["code"])
        if reasons: items.append({"path": entry["path"], "code": entry["code"], "reasons": reasons, "size_bytes": size_bytes})
    return items

top = run(["git", "rev-parse", "--show-toplevel"])
if top["returncode"] != 0:
    print(json.dumps({"success": False, "error": "not a git repository", "stderr": top["stderr"]}, ensure_ascii=False))
    sys.exit(top["returncode"])
branch = run(["git", "branch", "--show-current"])
status = run(["git", "status", "--short", "--branch"])
porcelain = run(["git", "status", "--porcelain"])
diff_stat = run(["git", "diff", "--stat"])
staged_diff_stat = run(["git", "diff", "--cached", "--stat"])
recent = run(["git", "log", "--oneline", "-5"])
entries, staged, unstaged, untracked = parse_status(porcelain["stdout"])
upstream, ahead_behind, unpushed = collect_ahead_behind()
payload = {"success": True, "repo_root": top["stdout"].strip(), "branch": branch["stdout"].strip(), "upstream": upstream, "ahead_behind": ahead_behind, "has_unpushed_commits": bool(unpushed.strip()), "unpushed_commits": unpushed, "dirty": any(line and not line.startswith("## ") for line in status["stdout"].splitlines()), "status": status["stdout"], "status_entries": entries, "staged": staged, "unstaged": unstaged, "untracked": untracked, "diff_stat": diff_stat["stdout"], "staged_diff_stat": staged_diff_stat["stdout"], "suspicious_files": collect_suspicious(entries), "recent_commits": recent["stdout"]}
print(json.dumps(payload, ensure_ascii=False))

