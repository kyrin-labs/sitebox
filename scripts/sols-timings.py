#!/usr/bin/env python3
"""Read timings.jsonl for a Sols thread and show where the time went.

Usage: python3 scripts/sols-timings.py --root ~/sitebox-studio-try <thread-id>
"""
import argparse
import json
import pathlib
import sys


def load(root: str, thread: str):
    p = pathlib.Path(root) / "dashboard/data/studio/threads" / thread / "timings.jsonl"
    if not p.exists():
        sys.exit(f"no timings at {p} (the turn predates the timing log?)")
    rows = []
    for line in p.read_text().splitlines():
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return rows


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True)
    ap.add_argument("thread")
    ap.add_argument("--top", type=int, default=5)
    a = ap.parse_args()
    rows = load(a.root, a.thread)

    turns = {}
    for r in rows:
        turns.setdefault(r.get("turn", 0), []).append(r)

    print(f"thread {a.thread}")
    for turn, items in sorted(turns.items()):
        llm = [r for r in items if r.get("kind") == "llm"]
        tools = [r for r in items if r.get("kind") == "tool"]
        total_llm = sum(r["ms"] for r in llm) / 1000
        total_tool = sum(r["ms"] for r in tools) / 1000
        print(
            f"\nturn {turn}: {total_llm + total_tool:7.0f}s  "
            f"llm {total_llm:6.0f}s ({len(llm)} rounds)  tools {total_tool:6.0f}s ({len(tools)} calls)"
        )
        by_tool = {}
        for r in tools:
            by_tool[r["name"]] = by_tool.get(r["name"], 0) + r["ms"]
        for name, ms in sorted(by_tool.items(), key=lambda x: -x[1])[:a.top]:
            print(f"    tool {name:26s} {ms/1000:6.1f}s")
        for r in sorted(llm, key=lambda x: -x["ms"])[:a.top]:
            print(f"    llm  round {r.get('round'):>2}                {r['ms']/1000:6.1f}s")
        for r in sorted(tools, key=lambda x: -x["ms"])[:a.top]:
            print(f"    call {r['name']:26s} {r['ms']/1000:6.1f}s")


if __name__ == "__main__":
    main()
