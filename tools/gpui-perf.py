#!/usr/bin/env python3
# Resident memory of the whole app tree, idle CPU, and playing CPU (the
# silent clock), for the shipped GPUI frontend. Three samples, medians.
#
#   tools/gpui-perf.py

import subprocess
import time
import statistics
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "target/release/winkel")
SAMPLES = 3


def sh(*args):
    return subprocess.run(args, capture_output=True, text=True)


def kill_all():
    subprocess.run(["pkill", "-f", "[w]inkel --backend"], capture_output=True)
    subprocess.run(["pkill", "-x", "winkel"], capture_output=True)
    time.sleep(1.0)


def window_pid():
    out = sh("hyprctl", "-j", "clients").stdout
    import json
    try:
        clients = json.loads(out)
    except Exception:
        return None
    for c in clients:
        if c.get("class") == "dev.douglasmoura.winkel":
            return c.get("pid")
    return None


def tree_pids(pid):
    pids = [pid]
    try:
        out = subprocess.run(["pgrep", "-P", str(pid)], capture_output=True, text=True).stdout.split()
        pids += [int(p) for p in out]
    except Exception:
        pass
    return [p for p in pids if os.path.exists(f"/proc/{p}/stat")]


def tree_rss(pid):
    total = 0
    for p in tree_pids(pid):
        try:
            with open(f"/proc/{p}/status") as f:
                for line in f:
                    if line.startswith("VmRSS:"):
                        total += int(line.split()[1])
                        break
        except Exception:
            pass
    return total


def read_cpu(pid):
    with open(f"/proc/{pid}/stat") as f:
        parts = f.read().split()
    return int(parts[13]) + int(parts[14])


def tree_cpu_over(pid, seconds=1.0):
    pids = tree_pids(pid)
    before = {p: read_cpu(p) for p in pids}
    t0 = time.monotonic()
    time.sleep(seconds)
    pids = tree_pids(pid)
    total = 0
    for p in pids:
        b = before.get(p)
        if b is None:
            continue
        total += read_cpu(p) - b
    wall = time.monotonic() - t0
    return total / 100.0 / wall * 100.0  # ticks@100Hz / wall → % of one core


def run_one():
    kill_all()
    env = dict(os.environ, WINKEL_SILENT="1")
    subprocess.Popen([BIN], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    pid = None
    for _ in range(60):
        time.sleep(0.25)
        pid = window_pid()
        if pid:
            break
    if not pid:
        print("gpui: never mapped", file=__import__("sys").stderr)
        kill_all()
        return None
    time.sleep(2.0)  # fonts, first state
    rss_idle = tree_rss(pid)
    cpu_idle = tree_cpu_over(pid, 1.5)
    time.sleep(0.3)
    sh("wtype", "-k", "space")
    time.sleep(1.0)
    cpu_play = tree_cpu_over(pid, 2.0)
    kill_all()
    return (rss_idle, cpu_idle, cpu_play)


if not os.path.isfile(BIN) or not os.access(BIN, os.X_OK):
    raise SystemExit(f"build first: cargo build --release ({BIN} missing)")

print("frontend rss_kib cpu_idle_pct cpu_playing_pct")
rows = []
for _ in range(SAMPLES):
    r = run_one()
    if r:
        rows.append(r)
if rows:
    print(
        f"gpui {statistics.median(r[0] for r in rows):.0f} "
        f"{statistics.median(r[1] for r in rows):.1f} "
        f"{statistics.median(r[2] for r in rows):.1f}"
    )
kill_all()
