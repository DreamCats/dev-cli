import json
import os
import subprocess
import sys
from pathlib import Path

repo = sys.argv[1].strip("/")
roots = json.loads(sys.argv[2])
if repo.startswith("/") or repo.startswith("~/"):
    path = Path(os.path.expanduser(repo))
    print(json.dumps({"success": path.exists(), "input": repo, "path": str(path) if path.exists() else "", "searched_roots": [], "source": "direct", "error": "" if path.exists() else "path does not exist"}, ensure_ascii=False))
    sys.exit(0 if path.exists() else 1)
expanded_roots = []
for root in roots:
    expanded = os.path.expanduser(root)
    if expanded not in expanded_roots:
        expanded_roots.append(expanded)
for root in expanded_roots:
    candidate = Path(root) / repo
    if candidate.exists():
        print(json.dumps({"success": True, "input": repo, "path": str(candidate), "searched_roots": expanded_roots, "source": "root"}, ensure_ascii=False))
        sys.exit(0)
parts = repo.split("/")
suffix = os.path.join(*parts[-2:]) if len(parts) >= 2 else repo
for root in expanded_roots:
    if not Path(root).exists():
        continue
    find = subprocess.run(["find", root, "-maxdepth", "4", "-type", "d", "-path", f"*/{suffix}"], capture_output=True, text=True, timeout=10)
    matches = [line for line in find.stdout.splitlines() if line]
    if matches:
        print(json.dumps({"success": True, "input": repo, "path": matches[0], "searched_roots": expanded_roots, "source": "search", "matches": matches[:10]}, ensure_ascii=False))
        sys.exit(0)
print(json.dumps({"success": False, "input": repo, "path": "", "searched_roots": expanded_roots, "source": "search", "error": "repo not found"}, ensure_ascii=False))
sys.exit(1)

