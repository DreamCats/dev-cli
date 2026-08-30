import json
import subprocess
import sys

def run(args):
    r = subprocess.run(args, capture_output=True, text=True)
    return {"returncode": r.returncode, "stdout": r.stdout, "stderr": r.stderr}

def lines(result):
    return [line for line in str(result["stdout"]).splitlines() if line.endswith(".go")]

def package_for(path):
    directory = path.rsplit("/", 1)[0] if "/" in path else "."
    return "." if directory in {"", "."} else "./" + directory

top = run(["git", "rev-parse", "--show-toplevel"])
if top["returncode"] != 0:
    print(json.dumps({"success": False, "error": "not a git repository", "stderr": top["stderr"]}, ensure_ascii=False))
    sys.exit(top["returncode"])
diff = run(["git", "diff", "--name-only", "--diff-filter=ACMR"])
cached = run(["git", "diff", "--cached", "--name-only", "--diff-filter=ACMR"])
untracked = run(["git", "ls-files", "--others", "--exclude-standard"])
also_packages = json.loads("__ALSO_PACKAGES__")
changed_files = sorted(set(lines(diff) + lines(cached) + lines(untracked)))
packages = sorted({package_for(path) for path in changed_files} | set(also_packages))
if not packages:
    print(json.dumps({"success": True, "skipped": True, "reason": "no changed go files", "changed_files": [], "packages": [], "command": "", "returncode": 0, "stdout": "", "stderr": ""}, ensure_ascii=False))
    sys.exit(0)
go_list = run(["go", "list", *packages])
if go_list["returncode"] != 0:
    print(json.dumps({"success": False, "skipped": False, "changed_files": changed_files, "packages": packages, "command": "go list " + " ".join(packages), "returncode": go_list["returncode"], "stdout": go_list["stdout"], "stderr": go_list["stderr"]}, ensure_ascii=False))
    sys.exit(go_list["returncode"])
command = ["go", "test", *packages]
result = run(command)
print(json.dumps({"success": result["returncode"] == 0, "skipped": False, "changed_files": changed_files, "packages": packages, "command": " ".join(command), "returncode": result["returncode"], "stdout": result["stdout"], "stderr": result["stderr"]}, ensure_ascii=False))
sys.exit(result["returncode"])

