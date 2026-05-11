#!/usr/bin/env python3
"""Bounded read-only Kanban monitor for HLE scaffold workflow tasks.

This helper is intentionally not a daemon: it polls for at most 90 minutes,
prints only status changes, and exits when all requested task ids have reached a
terminal/blocked state. It may be run manually by an operator after the scaffold
workflow cards already exist; it must not be installed as a service or cron job.

Boundary note: this monitor is read-only. It does not dispatch, promote, claim,
complete, block, or otherwise mutate Kanban state.
"""
import datetime
import subprocess
import sys
import time

TASK_IDS = sys.argv[1:]
if not TASK_IDS:
    print(
        "usage: scripts/kanban-hle-workflow-monitor.py <task-id> [<task-id> ...]",
        file=sys.stderr,
    )
    sys.exit(2)

END = time.time() + 60 * 90
LAST = None


def run(cmd):
    return subprocess.run(
        cmd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    ).stdout


while time.time() < END:
    lines = []
    for task_id in TASK_IDS:
        show = run(["hermes", "kanban", "show", task_id])
        status = "unknown"
        for line in show.splitlines():
            if line.strip().startswith("Status:") or " status=" in line:
                status = line.strip()
                break
        lines.append(f"{task_id}: {status}")
    snapshot = "\n".join(lines)
    if snapshot != LAST:
        now = datetime.datetime.now(datetime.UTC).isoformat().replace("+00:00", "Z")
        print(f"--- {now} ---", flush=True)
        print(snapshot, flush=True)
        LAST = snapshot
    if TASK_IDS and all(
        any(word in line.lower() for word in ["done", "blocked", "archived", "terminal"])
        for line in lines
    ):
        sys.exit(0)
    time.sleep(60)
print("HLE Kanban monitor timeout reached; leaving tasks for normal dispatcher.", flush=True)
