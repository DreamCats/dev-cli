from __future__ import annotations
import json
import os
import selectors
import subprocess
import time

def build_args(command, shell):
    if shell in {None, "", "none"}: return command
    if shell == "zsh": return ["zsh", "-ic", command]
    if shell == "zsh-login": return ["zsh", "-lic", command]
    if shell == "bash": return ["bash", "-ic", command]
    if shell == "bash-login": return ["bash", "-lic", command]
    return [shell, "-c", command]

def emit(payload):
    print(json.dumps(payload, ensure_ascii=False), flush=True)

command = __COMMAND__
interval = __INTERVAL__
timeout = __TIMEOUT__
shell = __SHELL__
summary_chars = __SUMMARY_CHARS__
cwd = __CWD__
stdin_text = __STDIN__
start = time.monotonic()
next_tick = start + interval
output_parts = []
output_lines = 0
last_line = ""
timed_out = False
args = build_args(command, shell)
stdin_pipe = subprocess.PIPE if stdin_text is not None else None
process = subprocess.Popen(args, stdin=stdin_pipe, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, shell=isinstance(args, str))
if process.stdin is not None:
    try:
        process.stdin.write(stdin_text.encode())
        process.stdin.close()
    except BrokenPipeError:
        pass
selector = selectors.DefaultSelector()
if process.stdout is not None:
    selector.register(process.stdout, selectors.EVENT_READ)
emit({"event":"started","command":command,"cwd":cwd,"shell":shell,"pid":process.pid,"elapsed_seconds":0,"stdin":stdin_text is not None,"stdin_bytes":0 if stdin_text is None else len(stdin_text.encode())})
while True:
    now = time.monotonic()
    if timeout and now - start >= timeout and process.poll() is None:
        timed_out = True
        process.terminate()
        try: process.wait(timeout=5)
        except subprocess.TimeoutExpired: process.kill()
    wait_for = max(0.0, min(0.2, next_tick - now))
    for key, _ in selector.select(wait_for):
        chunk = os.read(key.fileobj.fileno(), 4096)
        if not chunk: continue
        text = chunk.decode(errors="replace")
        output_parts.append(text)
        output_lines += text.count("\n")
        for line in text.splitlines():
            if line.strip(): last_line = line.rstrip("\n")
    now = time.monotonic()
    if now >= next_tick and process.poll() is None:
        emit({"event":"running","elapsed_seconds":round(now-start,1),"output_lines":output_lines,"last_line":last_line})
        next_tick += interval
    if process.poll() is not None:
        if process.stdout is not None:
            while True:
                ready = selector.select(0)
                if not ready: break
                chunk = os.read(process.stdout.fileno(), 4096)
                if not chunk: break
                text = chunk.decode(errors="replace")
                output_parts.append(text)
                output_lines += text.count("\n")
                for line in text.splitlines():
                    if line.strip(): last_line = line.rstrip("\n")
        break
returncode = process.returncode
if timed_out: returncode = 124
output = "".join(output_parts)
truncated = False
if summary_chars > 0 and len(output) > summary_chars:
    output = output[-summary_chars:]
    truncated = True
emit({"event":"finished","command":command,"cwd":cwd,"shell":shell,"returncode":returncode,"success":returncode==0,"timed_out":timed_out,"elapsed_seconds":round(time.monotonic()-start,1),"output_lines":output_lines,"last_line":last_line,"output":output,"truncated":truncated})
raise SystemExit(returncode)

