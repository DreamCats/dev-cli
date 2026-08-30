import json
import subprocess
import sys

def run(args):
    r = subprocess.run(args, capture_output=True, text=True)
    return {"returncode": r.returncode, "stdout": r.stdout, "stderr": r.stderr}

def split_lines(text):
    return [line for line in text.splitlines() if line]

top = run(["git", "rev-parse", "--show-toplevel"])
if top["returncode"] != 0:
    print(json.dumps({"success": False, "error": "not a git repository", "stderr": top["stderr"]}, ensure_ascii=False))
    sys.exit(top["returncode"])
branch = run(["git", "branch", "--show-current"])
head = run(["git", "rev-parse", "--short", "HEAD"])
head_subject = run(["git", "log", "-1", "--pretty=%s"])
status = run(["git", "status", "--short", "--branch"])
diff_stat = run(["git", "diff", "--stat"])
staged_diff_stat = run(["git", "diff", "--cached", "--stat"])
name_only = run(["git", "diff", "--name-only"])
staged_name_only = run(["git", "diff", "--cached", "--name-only"])
recent = run(["git", "log", "--oneline", "-5"])
payload = {"success": True, "repo_root": top["stdout"].strip(), "branch": branch["stdout"].strip(), "head": head["stdout"].strip(), "head_subject": head_subject["stdout"].strip(), "status": status["stdout"], "dirty": any(line and not line.startswith("## ") for line in status["stdout"].splitlines()), "diff_stat": diff_stat["stdout"], "staged_diff_stat": staged_diff_stat["stdout"], "changed_files": split_lines(name_only["stdout"]), "staged_changed_files": split_lines(staged_name_only["stdout"]), "recent_commits": recent["stdout"], "verification": "not_run"}
print(json.dumps(payload, ensure_ascii=False))

