#!/usr/bin/env python3
"""End-to-end acceptance for SiteBox Studio (phase A).

A mock OpenAI-compatible server drives the tool loop, so the run is deterministic
and needs no model key. It proves the engine's behaviour: no-write turns, staged
checks, apply/rollback/undo, SSE, abort, pipeline builds, and the guards.
"""
import json
import os
import random
import shutil
import signal
import tempfile
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import http.client
import http.cookiejar
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SCRATCH = Path(tempfile.mkdtemp(prefix="studio-e2e-"))
BIN = Path(os.environ.get("STUDIO_BIN") or REPO / "engine/target/debug/siteboxd")
DASH = 4499
MOCK = 4811
TOKEN = "e2e-token"

INDEX = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Demo</title>
<style>
:root{--ink:#111;--bg:#fff}
body{background:var(--bg);color:var(--ink);font-family:system-ui;margin:0}
main{max-width:60ch;margin:4rem auto;padding:0 1rem}
</style>
</head>
<body>
<main id="main">
  <h1>Demo</h1>
  <p>One paragraph.</p>
</main>
</body>
</html>
"""

GEN_BUILD = """const fs = require('fs');
const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Gen</title></head>
<body><main><h1>Gen</h1><p>generated</p></main></body></html>`;
fs.writeFileSync('public/index.html', html);
console.log('built');
"""

LEGACY_INDEX = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Legacy</title>
<link href="https://fonts.googleapis.com/css2?family=Inter&display=swap" rel="stylesheet">
</head><body><main><h1>Legacy</h1><p>old page</p></main></body></html>"""

GEN_INDEX = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Gen</title></head>
<body><main><h1>Gen</h1><p>generated</p></main></body></html>"""

TOML = """schema = 1
pipeline = []
checks = []

[site]
id = "demo"
name = "Demo"
description = "studio e2e fixture"
port = 4601
category = "tools"
path = "sites/demo"
public_dir = "public"
serve = "static"
"""

LEGACY_TOML = """schema = 1
pipeline = []
checks = []

[site]
id = "legacy"
name = "Legacy"
description = "already red before any patch"
port = 4603
category = "tools"
path = "sites/legacy"
public_dir = "public"
serve = "static"
"""

GEN_TOML = """schema = 1
checks = []

[[pipeline]]
name = "build"
run = "node tools/build.js"
inputs = ["tools/build.js"]
outputs = ["public/index.html"]

[site]
id = "gen"
name = "Generated"
description = "pipeline fixture"
port = 4602
category = "tools"
path = "sites/gen"
public_dir = "public"
serve = "static"
"""

failures = []


def check(name, cond, detail=""):
    print(("PASS " if cond else "FAIL ") + name + (("  — " + str(detail)) if detail and not cond else ""))
    if not cond:
        failures.append(name)


class MockLLM(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def do_POST(self):
        length = int(self.headers.get("content-length") or 0)
        body = json.loads(self.rfile.read(length) or b"{}")
        model = body.get("model", "")
        msgs = body.get("messages", [])
        last_user = 0
        for i, m in enumerate(msgs):
            if m.get("role") == "user":
                last_user = i
        user_text = str(msgs[last_user].get("content") if msgs else "")
        since = [m for m in msgs[last_user:] if m.get("role") == "tool"]
        n = len(since)

        if model == "mock-slow":
            time.sleep(4)
            self._stream("slow answer", None)
            return

        if "rollback" in user_text:
            if n == 0:
                self._stream("Reading the page.", {"name": "sitebox_outline", "arguments": "{}"})
            elif n == 1:
                args = {"path": "public/index.html", "old": "<p>One paragraph.</p>",
                        "new": '<p>One paragraph.</p>\n  <img src="/missing.png">',
                        "count": 1, "reason": "add an image without dimensions (must fail SB003)"}
                self._stream("Proposing a patch that should fail the checks.", {"name": "sitebox_propose_patch", "arguments": json.dumps(args)})
            else:
                self._stream("Queued; the apply should roll back.", None)
            return

        if "legacy" in user_text:
            if n == 0:
                args = {"path": "public/index.html", "old": "<h1>Legacy</h1>",
                        "new": "<h1>Legacy Two</h1>", "count": 1,
                        "reason": "change a heading on an already-red site"}
                self._stream("Reading the legacy page.", {"name": "sitebox_propose_patch", "arguments": json.dumps(args)})
            else:
                self._stream("Queued a patch for a site that was already red.", None)
            return

        if "buildfail" in user_text:
            if n == 0:
                args = {"path": "tools/build.js", "old": "console.log('built');",
                        "new": "throw new Error('boom');", "count": 1,
                        "reason": "make the generator fail (must roll back)"}
                self._stream("Patching the generator to fail.", {"name": "sitebox_propose_patch", "arguments": json.dumps(args)})
            else:
                self._stream("Queued a build failure test.", None)
            return

        if "generated" in user_text:
            if n == 0:
                self._stream("Checking how this site is built.", {"name": "sitebox_site_kind", "arguments": "{}"})
            elif n == 1:
                args = {"path": "tools/build.js", "old": "<h1>Gen</h1>", "new": "<h1>Gen Two</h1>",
                        "count": 1, "reason": "rename the generated heading at its source"}
                self._stream("Patching the generator, not the output.", {"name": "sitebox_propose_patch", "arguments": json.dumps(args)})
            else:
                self._stream("Queued a change to the generator.", None)
            return

        if n == 0:
            self._stream("Let me look at the page.", {"name": "sitebox_outline", "arguments": "{}"})
        elif n == 1:
            args = {"path": "public/index.html", "old": "<h1>Demo</h1>", "new": "<h1>Demo Site</h1>",
                    "count": 1, "reason": "rename the heading"}
            self._stream("Proposing a one-line change.", {"name": "sitebox_propose_patch", "arguments": json.dumps(args)})
        else:
            self._stream("Proposed the heading change; apply it from the card.", None)

    def _stream(self, text, call):
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("cache-control", "no-store")
        self.send_header("connection", "close")
        self.end_headers()

        def send(obj):
            try:
                self.wfile.write(("data: " + json.dumps(obj) + "\n\n").encode())
                self.wfile.flush()
            except BrokenPipeError:
                pass

        if text:
            send({"choices": [{"delta": {"content": text}, "finish_reason": None}]})
        if call:
            send({"choices": [{"delta": {"tool_calls": [{
                "index": 0, "id": "call_1", "type": "function",
                "function": {"name": call["name"], "arguments": call["arguments"]},
            }]}, "finish_reason": None}]})
            send({"choices": [{"delta": {}, "finish_reason": "tool_calls"}]})
        else:
            send({"choices": [{"delta": {}, "finish_reason": "stop"}]})
        send({"choices": [], "usage": {"prompt_tokens": 120, "completion_tokens": 30}})
        try:
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        except BrokenPipeError:
            pass


def req(url, method="GET", body=None, token=None, opener=None, timeout=15):
    data = None if body is None else json.dumps(body).encode()
    r = urllib.request.Request(url, data=data, method=method)
    r.add_header("content-type", "application/json")
    if token:
        r.add_header("x-studio-token", token)
    o = opener or urllib.request.build_opener()
    try:
        with o.open(r, timeout=timeout) as resp:
            raw = resp.read().decode()
            return resp.status, (json.loads(raw) if raw.strip().startswith(("{", "[")) else raw), resp.headers
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        try:
            parsed = json.loads(raw)
        except Exception:
            parsed = raw
        return e.code, parsed, e.headers


class SSE(threading.Thread):
    """Reads /api/studio/events until done/error or a deadline."""

    def __init__(self, thread_id, token):
        super().__init__(daemon=True)
        self.thread_id = thread_id
        self.token = token
        self.events = []
        self.error = None

    def run(self):
        try:
            conn = http.client.HTTPConnection("127.0.0.1", DASH, timeout=25)
            conn.request("GET", f"/api/studio/events?thread={self.thread_id}",
                         headers={"x-studio-token": self.token, "accept": "text/event-stream"})
            resp = conn.getresponse()
            deadline = time.time() + 22
            while time.time() < deadline:
                line = resp.readline()
                if not line:
                    break
                line = line.decode(errors="replace").strip()
                if not line.startswith("data:"):
                    continue
                try:
                    ev = json.loads(line[5:].strip())
                except Exception:
                    continue
                self.events.append(ev)
                if ev.get("type") in ("done", "error", "aborted"):
                    break
        except Exception as e:  # noqa: BLE001
            self.error = str(e)


def wait_patch(thread_id, timeout=30):
    """Newest patch belonging to this thread (other threads' patches are noise)."""
    start = time.time()
    while time.time() - start < timeout:
        found = []
        for p in (SCRATCH / "dashboard/data/studio/patches").glob("*.json"):
            try:
                rec = json.loads(p.read_text())
            except Exception:
                continue
            if rec.get("thread_id") == thread_id:
                found.append(rec)
        if found:
            found.sort(key=lambda r: r.get("created", ""))
            return found[-1]
        time.sleep(0.25)
    return None


def wait_done(thread_id, timeout=30):
    start = time.time()
    while time.time() - start < timeout:
        s, box, _ = req(f"http://127.0.0.1:{DASH}/api/studio/threads/{thread_id}/messages", token=TOKEN)
        if s == 200 and any(m.get("role") == "assistant" for m in box.get("messages", [])):
            return box.get("messages", [])
        time.sleep(0.25)
    return []


def main():
    if not BIN.is_file():
        raise SystemExit(f"missing {BIN}; run: cargo build --bins (in engine/)")
    (SCRATCH / "sites/demo/public").mkdir(parents=True)
    (SCRATCH / "sites/legacy/public").mkdir(parents=True)
    (SCRATCH / "sites/gen/public").mkdir(parents=True)
    (SCRATCH / "sites/gen/tools").mkdir(parents=True)
    (SCRATCH / "dashboard/data").mkdir(parents=True)
    (SCRATCH / "sites/demo/sitebox.toml").write_text(TOML)
    (SCRATCH / "sites/demo/public/index.html").write_text(INDEX)
    (SCRATCH / "sites/legacy/sitebox.toml").write_text(LEGACY_TOML)
    (SCRATCH / "sites/legacy/public/index.html").write_text(LEGACY_INDEX)
    (SCRATCH / "sites/gen/sitebox.toml").write_text(GEN_TOML)
    (SCRATCH / "sites/gen/tools/build.js").write_text(GEN_BUILD)
    (SCRATCH / "sites/gen/public/index.html").write_text(GEN_INDEX)
    (SCRATCH / "dashboard/data/sites.json").write_text(json.dumps({
        "dashboard": {"name": "Test", "port": DASH},
        "sites": [
            {"id": "demo", "name": "Demo", "port": 4601, "path": "sites/demo", "category": "tools", "url": "http://127.0.0.1:4601"},
            {"id": "gen", "name": "Generated", "port": 4602, "path": "sites/gen", "category": "tools", "url": "http://127.0.0.1:4602"},
            {"id": "legacy", "name": "Legacy", "port": 4603, "path": "sites/legacy", "category": "tools", "url": "http://127.0.0.1:4603"},
        ],
    }, indent=2))

    mock = HTTPServer(("127.0.0.1", MOCK), MockLLM)
    threading.Thread(target=mock.serve_forever, daemon=True).start()

    env = dict(os.environ)
    env["STUDIO_TOKEN"] = TOKEN
    proc = subprocess.Popen(
        [str(BIN), "--root", str(SCRATCH), "--port", str(DASH), "--ui", str(REPO / "dashboard/public")],
        cwd=str(SCRATCH), env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
    base = f"http://127.0.0.1:{DASH}"
    try:
        for _ in range(80):
            try:
                s, _, _ = req(base + "/api/sites")
                if s == 200:
                    break
            except Exception:
                pass
            time.sleep(0.25)
        else:
            print(proc.stdout.read() if proc.stdout else "")
            raise SystemExit("dashboard did not start")

        print("── P0: auth, config, no key leak")
        s, _, _ = req(base + "/api/studio/status")
        check("no token is 401", s == 401, s)
        s, st0, _ = req(base + "/api/studio/status", token=TOKEN)
        check("status with token", s == 200, st0)
        check("unconfigured at boot", st0.get("settings", {}).get("configured") is False, st0)
        check("site list populated", len(st0.get("sites", [])) == 3, st0.get("sites"))
        s, cfg, _ = req(base + "/api/studio/config", "PUT", {
            "base_url": f"http://127.0.0.1:{MOCK}/v1", "model": "mock-1", "api_key": "sk-secret",
        }, token=TOKEN)
        check("config saved", s == 200 and cfg.get("configured") is True, cfg)
        check("key never returned", "sk-secret" not in json.dumps(cfg) and cfg.get("has_key") is True, cfg)
        check("endpoint normalised", cfg.get("endpoint") == f"http://127.0.0.1:{MOCK}/v1/chat/completions", cfg.get("endpoint"))
        mode = oct((SCRATCH / "dashboard/data/studio/settings.json").stat().st_mode & 0o777)
        check("settings.json is 0600", mode == "0o600", mode)

        jar = http.cookiejar.CookieJar()
        opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
        s, _, h = req(base + "/", opener=opener)
        s2, _, _ = req(base + "/api/studio/status", opener=opener)
        check("UI cookie opens the door", s == 200 and s2 == 200, s2)

        print("── P1: read-only turn, SSE, no writes")
        s, thread, _ = req(base + "/api/studio/threads", "POST", {"site_id": "demo"}, token=TOKEN)
        tid = thread["id"]
        before = (SCRATCH / "sites/demo/public/index.html").read_bytes()
        sse = SSE(tid, TOKEN)
        sse.start()
        s, _, _ = req(base + "/api/studio/chat", "POST", {"thread_id": tid, "message": "rename the heading"}, token=TOKEN)
        check("chat accepted (202)", s == 202, s)
        patch = wait_patch(tid)
        time.sleep(0.4)
        check("tool loop produced a patch", patch is not None)
        check("chat wrote no file", (SCRATCH / "sites/demo/public/index.html").read_bytes() == before)
        types = [e.get("type") for e in sse.events]
        check("SSE delivered text_delta", "text_delta" in types, types)
        check("SSE delivered tool events", "tool_start" in types and "tool_end" in types, types)
        check("SSE delivered patch + done", "patch" in types and "done" in types, types)
        if patch:
            check("patch pending", patch.get("status") == "pending", patch.get("status"))
            check("diff shows the change", "+  <h1>Demo Site</h1>" in patch.get("diff", ""), patch.get("diff"))

        print("── P2/P3: apply, re-apply refusal, undo")
        s, rep, _ = req(base + f"/api/studio/patches/{patch['id']}/apply", "POST", {}, token=TOKEN)
        after = (SCRATCH / "sites/demo/public/index.html").read_text()
        check("apply ok", s == 200 and rep.get("ok") is True, (s, rep))
        check("file changed", "<h1>Demo Site</h1>" in after)
        check("checks passed after apply", (rep.get("static") or {}).get("ok") is True, rep.get("static"))
        s, _, _ = req(base + f"/api/studio/patches/{patch['id']}/apply", "POST", {}, token=TOKEN)
        check("re-apply refused (409)", s == 409, s)
        s, rep3, _ = req(base + f"/api/studio/sites/demo/undo", "POST", {}, token=TOKEN)
        check("undo ok", s == 200 and rep3.get("ok") is True, (s, rep3))
        check("undo restores identical bytes", (SCRATCH / "sites/demo/public/index.html").read_bytes() == before)

        print("── P2/P3: a patch that breaks a check rolls back")
        s, th2, _ = req(base + "/api/studio/threads", "POST", {"site_id": "demo"}, token=TOKEN)
        req(base + "/api/studio/chat", "POST", {"thread_id": th2["id"], "message": "rollback please"}, token=TOKEN)
        bad = wait_patch(th2["id"])
        time.sleep(0.5)
        check("bad patch queued", bad and "missing.png" in bad.get("new", ""), bad and bad.get("path"))
        check("staging flagged the failure", (bad.get("verify") or {}).get("ok") is False, bad.get("verify"))
        s, rep4, _ = req(base + f"/api/studio/patches/{bad['id']}/apply", "POST", {}, token=TOKEN)
        check("apply reports rollback", s == 200 and rep4.get("restored") is True, (s, rep4))
        check("bytes untouched after rollback", (SCRATCH / "sites/demo/public/index.html").read_bytes() == before)
        check("patch marked rolled-back", json.loads((SCRATCH / "dashboard/data/studio/patches" / f"{bad['id']}.json").read_text()).get("status") == "rolled-back")

        print("── P3: generated site — patch the source, build the output")
        s, th3, _ = req(base + "/api/studio/threads", "POST", {"site_id": "gen"}, token=TOKEN)
        req(base + "/api/studio/chat", "POST", {"thread_id": th3["id"], "message": "generated change"}, token=TOKEN)
        genpatch = wait_patch(th3["id"])
        time.sleep(0.4)
        check("generated patch targets tools/", genpatch and genpatch.get("path") == "tools/build.js", genpatch and genpatch.get("path"))
        s, grep, _ = req(base + f"/api/studio/patches/{genpatch['id']}/apply", "POST", {}, token=TOKEN)
        built = (SCRATCH / "sites/gen/public/index.html").read_text()
        check("apply ok on generated site", s == 200 and grep.get("ok") is True, (s, grep))
        check("build step ran", any(b.get("name") == "build" for b in grep.get("build") or []), grep.get("build"))
        check("build output carries the change", "<h1>Gen Two</h1>" in built, built[:120])

        print("── P3: a site that was already red can still be edited")
        s, th6, _ = req(base + "/api/studio/threads", "POST", {"site_id": "legacy"}, token=TOKEN)
        req(base + "/api/studio/chat", "POST", {"thread_id": th6["id"], "message": "legacy change"}, token=TOKEN)
        lp = wait_patch(th6["id"])
        time.sleep(0.4)
        lv = (lp or {}).get("verify") or {}
        check("pre-existing failure recognised", lv.get("ok") is True and "SB002" in (lv.get("pre_existing") or []), lv)
        check("patch itself introduces nothing", (lv.get("introduced") or []) == [], lv.get("introduced"))
        s, lrep, _ = req(base + f"/api/studio/patches/{lp['id']}/apply", "POST", {}, token=TOKEN)
        legacy_now = (SCRATCH / "sites/legacy/public/index.html").read_text()
        check("apply succeeds despite red baseline", s == 200 and lrep.get("ok") is True and lrep.get("restored") is False, (s, lrep))
        check("legacy heading changed", "<h1>Legacy Two</h1>" in legacy_now)
        check("pre-existing reported on apply", "SB002" in (lrep.get("pre_existing") or []), lrep.get("pre_existing"))

        print("── P3: a failing build also rolls back")
        s, th5, _ = req(base + "/api/studio/threads", "POST", {"site_id": "gen"}, token=TOKEN)
        req(base + "/api/studio/chat", "POST", {"thread_id": th5["id"], "message": "buildfail"}, token=TOKEN)
        bp = wait_patch(th5["id"])
        time.sleep(0.4)
        gen_before = (SCRATCH / "sites/gen/tools/build.js").read_bytes()
        check("failing-build patch queued", bp and "throw new Error" in bp.get("new", ""), bp and bp.get("path"))
        s, bres, _ = req(base + f"/api/studio/patches/{bp['id']}/apply", "POST", {}, token=TOKEN)
        check("failed build returns an error", s == 409 and "snapshot restored" in json.dumps(bres), (s, bres))
        check("generator restored after failed build", (SCRATCH / "sites/gen/tools/build.js").read_bytes() == gen_before)

        print("── P4: abort a slow turn, then keep working")
        req(base + "/api/studio/config", "PUT", {"base_url": f"http://127.0.0.1:{MOCK}/v1", "model": "mock-slow"}, token=TOKEN)
        s, th4, _ = req(base + "/api/studio/threads", "POST", {"site_id": "demo"}, token=TOKEN)
        req(base + "/api/studio/chat", "POST", {"thread_id": th4["id"], "message": "take your time"}, token=TOKEN)
        time.sleep(0.8)
        t0 = time.time()
        s, _, _ = req(base + "/api/studio/abort", "POST", {"thread_id": th4["id"]}, token=TOKEN)
        check("abort returns 200", s == 200, s)
        req(base + "/api/studio/config", "PUT", {"base_url": f"http://127.0.0.1:{MOCK}/v1", "model": "mock-1"}, token=TOKEN)
        s, _, _ = req(base + "/api/studio/chat", "POST", {"thread_id": th4["id"], "message": "rename the heading"}, token=TOKEN)
        check("site lock released after abort", s == 202, s)
        check("abort was fast", time.time() - t0 < 5, time.time() - t0)

        print("── P4: one submission cannot open two turns")
        s, th7, _ = req(base + "/api/studio/threads", "POST", {"site_id": "legacy"}, token=TOKEN)
        nonce = "dup-" + str(random.random())
        s1, b1, _ = req(base + "/api/studio/chat", "POST", {"thread_id": th7["id"], "message": "legacy change", "nonce": nonce}, token=TOKEN)
        s2, b2, _ = req(base + "/api/studio/chat", "POST", {"thread_id": th7["id"], "message": "legacy change", "nonce": nonce}, token=TOKEN)
        check("duplicate nonce is accepted and ignored", s1 == 202 and s2 == 202 and b2.get("duplicate") is True, (s1, b1, s2, b2))
        time.sleep(3)
        s, box, _ = req(base + f"/api/studio/threads/{th7['id']}/messages", token=TOKEN)
        users = [m for m in box.get("messages", []) if m.get("role") == "user"]
        check("only one user message stored", len(users) == 1, len(users))

        print("── P4: guards")
        s, body, _ = req(base + "/api/studio/sites/demo/outline?path=../sitebox.toml", token=TOKEN)
        check("traversal refused (400)", s == 400 and "escape" in json.dumps(body), (s, body))
        s, body, _ = req(base + "/api/studio/sites/demo/outline?path=/etc/passwd", token=TOKEN)
        check("absolute path refused (400)", s == 400, (s, body))
        s, _, _ = req(base + "/api/studio/chat", "POST", {"thread_id": tid, "message": "x" * 9000}, token=TOKEN)
        check("oversized message refused (400)", s == 400, s)
        s, _, _ = req(base + "/api/studio/sites/nope/outline", token=TOKEN)
        check("unknown site 404", s == 404, s)
        s, _, _ = req(base + "/api/studio/shots/demo/..%2F..%2Fsettings.json", token=TOKEN)
        check("shot path guard", s in (403, 404), s)

        print("── P4: audit + legacy dashboard untouched")
        audit = [json.loads(l) for l in (SCRATCH / "dashboard/data/studio/audit.jsonl").read_text().splitlines() if l.strip()]
        check("audit has apply+undo", sum(1 for a in audit if a.get("action") == "apply") >= 2 and any(a.get("action") == "undo" for a in audit), len(audit))
        s, sites, _ = req(base + "/api/sites")
        check("original /api/sites still answers", s == 200 and len(sites) == 3, (s, sites))
        s, _, _ = req(base + "/js/studio.js")
        check("studio.js served", s == 200, s)
        print("INFO audit lines:", len(audit))
    finally:
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except Exception:
            proc.kill()
        mock.shutdown()

    if not os.environ.get("STUDIO_E2E_KEEP"):
        shutil.rmtree(SCRATCH, ignore_errors=True)
    else:
        print("kept:", SCRATCH)
    print()
    if failures:
        print(f"FAILED {len(failures)}: {failures}")
        return 1
    print("ALL CHECKS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
