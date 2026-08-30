from __future__ import annotations
import difflib, json, posixpath, subprocess, sys
from pathlib import Path

class PatchError(Exception):
    def __init__(self, message, path=None, details=None):
        super().__init__(message); self.path = path; self.details = details or {}

def norm(path):
    if not path or path.startswith("/"): raise PatchError(f"unsafe path: {path}")
    n = posixpath.normpath(path)
    if n in {"", "."} or n.startswith("../") or ".." in n.split("/") or n.split("/")[0] == ".git":
        raise PatchError(f"unsafe path: {path}")
    return n

def is_action(line):
    return line.startswith("*** Add File: ") or line.startswith("*** Update File: ") or line.startswith("*** Delete File: ") or line.startswith("*** End Patch")

def parse(text):
    lines = text.splitlines()
    if not lines or lines[0] != "*** Begin Patch": raise PatchError("patch must start with '*** Begin Patch'")
    if lines[-1] != "*** End Patch": raise PatchError("patch must end with '*** End Patch'")
    ops=[]; i=1
    while i < len(lines)-1:
        line=lines[i]
        if line.startswith("*** Add File: "):
            path=norm(line.removeprefix("*** Add File: ")); i+=1; content=[]
            while i < len(lines)-1 and not is_action(lines[i]):
                if not lines[i].startswith("+"): raise PatchError("add-file lines must start with '+'", path)
                content.append(lines[i][1:]+"\n"); i+=1
            ops.append(("add", path, content)); continue
        if line.startswith("*** Delete File: "):
            ops.append(("delete", norm(line.removeprefix("*** Delete File: ")), None)); i+=1; continue
        if line.startswith("*** Update File: "):
            path=norm(line.removeprefix("*** Update File: ")); i+=1; hunks=[]
            while i < len(lines)-1 and not is_action(lines[i]):
                if not lines[i].startswith("@@"): raise PatchError("update hunk must start with '@@'", path)
                i+=1; h=[]
                while i < len(lines)-1 and not lines[i].startswith("@@") and not is_action(lines[i]):
                    if lines[i][:1] not in {" ","-","+"}: raise PatchError("update lines must start with ' ', '-' or '+'", path)
                    h.append((lines[i][0], lines[i][1:]+"\n")); i+=1
                if not h: raise PatchError("empty update hunk", path)
                hunks.append(h)
            ops.append(("update", path, hunks)); continue
        raise PatchError(f"unknown patch directive: {line}")
    if not ops: raise PatchError("patch contains no operations")
    return ops

def trim_lines(lines, limit=8):
    visible = [line.rstrip("\n") for line in lines[:limit]]
    if len(lines) > limit: visible.append("...")
    return visible

def similar_windows(lines, needle):
    if not needle or not lines: return []
    width = len(needle); expected = "".join(needle); out = []
    for start in range(max(len(lines)-width, 0)+1):
        window = lines[start:start+width]
        score = difflib.SequenceMatcher(None, expected, "".join(window)).ratio()
        out.append({"start_line": start+1, "score": round(score, 3), "snippet": trim_lines(window)})
    out.sort(key=lambda item: item["score"], reverse=True)
    return out[:3]

def apply_hunk(lines, hunk, path, hunk_index):
    old=[v for m,v in hunk if m in {" ","-"}]; new=[v for m,v in hunk if m in {" ","+"}]
    matches=[i for i in range(len(lines)-len(old)+1) if lines[i:i+len(old)] == old]
    if not matches:
        raise PatchError("hunk context did not match", path, {"hunk_index": hunk_index, "expected": trim_lines(old), "candidates": similar_windows(lines, old)})
    if len(matches)>1:
        raise PatchError("hunk context matched multiple locations", path, {"hunk_index": hunk_index, "match_lines": [m+1 for m in matches[:10]], "match_count": len(matches)})
    i=matches[0]; return lines[:i]+new+lines[i+len(old):]

def count_changed(before, after):
    prefix = 0; max_prefix = min(len(before), len(after))
    while prefix < max_prefix and before[prefix] == after[prefix]: prefix += 1
    before_suffix = len(before); after_suffix = len(after)
    while before_suffix > prefix and after_suffix > prefix and before[before_suffix-1] == after[after_suffix-1]:
        before_suffix -= 1; after_suffix -= 1
    return after_suffix - prefix, before_suffix - prefix

def patch_stat(changed):
    lines=[]; total_add=0; total_del=0
    for item in changed:
        add=int(item["additions"]); delete=int(item["deletions"]); total_add += add; total_del += delete
        lines.append(f" {item['path']} | {add+delete} {'+'*min(add,30)}{'-'*min(delete,30)}")
    if changed: lines.append(f" {len(changed)} files changed, {total_add} insertions(+), {total_del} deletions(-)")
    return "\n".join(lines) + ("\n" if lines else "")

def git_diff_stat(repo, changed):
    paths=[str(item["path"]) for item in changed]
    if not paths: return ""
    result=subprocess.run(["git","diff","--stat","--",*paths], cwd=repo, capture_output=True, text=True)
    return result.stdout if result.returncode == 0 else ""

def prune_empty_dirs(path, repo):
    while path != repo:
        try: path.rmdir()
        except OSError: return
        path = path.parent

def main():
    repo=Path(sys.argv[1]).resolve(); patch=Path(sys.argv[2]).read_text(); check=len(sys.argv)>3 and sys.argv[3]=="--check"
    changed=[]; pending={}; touched=set()
    try:
        for action,path,data in parse(patch):
            if path in touched: raise PatchError("multiple operations for one file are not supported", path)
            touched.add(path)
            target=(repo/path).resolve()
            try: target.relative_to(repo)
            except ValueError: raise PatchError("path escapes repository", path)
            if action=="add":
                if target.exists(): raise PatchError("add target already exists", path)
                pending[path]=data; changed.append({"path":path,"action":"add","additions":len(data),"deletions":0})
            elif action=="delete":
                if not target.is_file(): raise PatchError("delete target does not exist or is not a file", path)
                lines=target.read_text().splitlines(keepends=True); pending[path]=None; changed.append({"path":path,"action":"delete","additions":0,"deletions":len(lines)})
            else:
                if not target.is_file(): raise PatchError("update target does not exist or is not a file", path)
                before=target.read_text().splitlines(keepends=True); lines=before
                for hunk_index,h in enumerate(data, start=1): lines=apply_hunk(lines,h,path,hunk_index)
                additions,deletions=count_changed(before,lines)
                pending[path]=lines; changed.append({"path":path,"action":"update","additions":additions,"deletions":deletions})
        if not check:
            for path,lines in pending.items():
                target=repo/path
                if lines is None:
                    target.unlink(); prune_empty_dirs(target.parent, repo)
                else: target.parent.mkdir(parents=True, exist_ok=True); target.write_text("".join(lines))
        pstat=patch_stat(changed); gstat="" if check else git_diff_stat(repo, changed)
        print(json.dumps({"success":True,"applied":not check,"changed_files":changed,"patch_stat":pstat,"git_diff_stat":gstat,"diff_stat":gstat or pstat}, ensure_ascii=False)); return 0
    except PatchError as e:
        print(json.dumps({"success":False,"applied":False,"changed_files":[],"patch_stat":"","git_diff_stat":"","diff_stat":"","error":str(e),"path":e.path,"details":e.details}, ensure_ascii=False)); return 1

if __name__ == "__main__": raise SystemExit(main())

