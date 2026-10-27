#!/usr/bin/env python3
"""SolidCraft display check for István's machines (Fúzió).

    python3 ~/Projektek/solidcraft/scripts/fuzio-render-diag.py [design.solidcraft]

Starts SolidCraft a few times, each with one renderer setting changed (as it is set, refined
display off, OpenGL, the software renderer, Normal display quality, bodies meshed on the GPU), opens the design (default:
a tube drawn here), looks at it from below and up close, and saves a screenshot of each, with the
program's own messages and the graphics card's details, into ~/solidcraft-diag.zip. The window
opens and closes by itself; touch nothing until it says "Kész". Preferences are put back as they
were. Nothing is sent anywhere: send the zip yourself.
"""

import json
import os
import platform
import secrets
import shutil
import socket
import subprocess
import sys
import time
import zipfile

HOME = os.path.expanduser("~")
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HOME, "solidcraft-diag")
PORT = 47931


def find_app():
    if os.environ.get("SOLIDCRAFT_APP"):
        return os.environ["SOLIDCRAFT_APP"]
    for p in [os.path.join(HERE, "..", "target", "fuzio", "solidcraft"), os.path.join(HERE, "..", "target", "release", "solidcraft")]:
        if os.access(p, os.X_OK):
            return os.path.abspath(p)
    return shutil.which("solidcraft")


class Control:
    def __init__(self, token):
        self.token = token
        self.n = 0
        deadline = time.time() + 60
        while True:
            try:
                self.sock = socket.create_connection(("127.0.0.1", PORT), timeout=120)
                break
            except OSError:
                if time.time() > deadline:
                    raise
                time.sleep(0.5)
        self.f = self.sock.makefile("rw")

    def call(self, method, params=None):
        self.n += 1
        req = {"id": self.n, "method": method, "params": params or {}}
        if self.n == 1:
            req["token"] = self.token
        self.f.write(json.dumps(req) + "\n")
        self.f.flush()
        line = self.f.readline()
        r = json.loads(line) if line else {}
        return r.get("result"), r.get("error")


def tube(c):
    """The same tube as in the report: circles r 26 and r 16.56, extruded 87 mm."""
    for cmd, p in [
        ("sketch.create", {"plane": "XY"}),
        ("sketch.circle.center", {"center": [0, 0], "radius": 26}),
        ("sketch.circle.center", {"center": [0, 0], "radius": 16.560495}),
        ("sketch.finish", {}),
        ("solid.extrude", {"distance": 87}),
    ]:
        c.call("engine.execute", {"command": cmd, "params": p})


def run(name, env_extra, design, app, log, prefs_change=None):
    token = secrets.token_hex(16)
    env = dict(os.environ, SOLIDCRAFT_CONTROL_TOKEN=token, **env_extra)
    args = [app, "--control", str(PORT)] + ([design] if design else [])
    with open(os.path.join(OUT, f"{name}.log"), "w") as out:
        proc = subprocess.Popen(args, env=env, stdout=out, stderr=subprocess.STDOUT)
    try:
        c = Control(token)
        time.sleep(4)
        c.call("ui.key", {"key": "Escape"})  # a crash notice from an earlier run, if one shows
        if not design:
            tube(c)
        saved = None
        if prefs_change:
            saved, _ = c.call("ui.prefs", {})
            c.call("ui.prefs", prefs_change)
        time.sleep(3)
        views = [("alulrol", {"yaw": 2.6, "pitch": -0.75, "fit": True}), ("felulrol", {"yaw": 0.6, "pitch": 0.5, "fit": True})]
        for view, cam in views:
            c.call("ui.camera", cam)
            time.sleep(3)
            c.call("ui.screenshot", {"path": os.path.join(OUT, f"{name}-{view}.png")})
        # Up close (the view refines what it shows).
        c.call("ui.camera", views[0][1])
        at, _ = c.call("ui.at", {"world": [0, 0, 0]})
        x, y = (at[0], at[1]) if isinstance(at, list) and len(at) == 2 else (800, 500)
        for _ in range(4):
            c.call("ui.scroll", {"x": x, "y": y, "delta": 120})
            time.sleep(0.4)
        time.sleep(4)
        c.call("ui.screenshot", {"path": os.path.join(OUT, f"{name}-kozel.png")})
        if prefs_change and isinstance(saved, dict):
            c.call("ui.prefs", {k: saved.get(k) for k in prefs_change if k in saved})
        c.call("app.quit", {"force": True})
        proc.wait(timeout=30)
        log.write(f"{name}: kész\n")
    except Exception as e:  # noqa: BLE001 - a report of what happened, whatever it was
        log.write(f"{name}: hiba: {e}\n")
        proc.kill()


def sh(cmd):
    try:
        return subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=30).stdout
    except Exception as e:  # noqa: BLE001
        return f"({e})"


def main():
    app = find_app()
    if not app:
        print("Nem találom a SolidCraftot (target/fuzio/solidcraft vagy a PATH-on).")
        return 1
    design = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else None
    shutil.rmtree(OUT, ignore_errors=True)
    os.makedirs(OUT)
    with open(os.path.join(OUT, "info.txt"), "w") as info:
        info.write(f"app: {app}\nplatform: {platform.platform()}\n")
        info.write("commit: " + sh(f"git -C {HERE} rev-parse --short HEAD"))
        for cmd in ["nvidia-smi", "vulkaninfo --summary", "glxinfo -B", "lscpu | head -20", "cat /proc/meminfo | head -3", "echo $XDG_SESSION_TYPE"]:
            info.write(f"\n$ {cmd}\n{sh(cmd)}")
        for f in ["graphics.json", "prefs.json"]:
            for d in [os.path.join(HOME, ".config", "solidcraft"), os.path.join(HOME, ".local", "share", "solidcraft")]:
                p = os.path.join(d, f)
                if os.path.exists(p):
                    info.write(f"\n--- {p}\n{open(p).read()}")
    with open(os.path.join(OUT, "futasok.txt"), "w") as log:
        runs = [
            ("1-alap", {}, None),
            ("2-finomitas-ki", {"SOLIDCRAFT_DISPLAY_REFINE": "0"}, None),
            ("3-opengl", {"WGPU_BACKEND": "gl"}, None),
            ("4-szoftveres", {"SOLIDCRAFT_RENDERER": "cpu"}, None),
            ("5-normal-minoseg", {}, {"display_quality": "normal", "supersampling": 1.0}),
            ("6-gpu-halozas", {"SOLIDCRAFT_GPU_TESS": "1"}, None),
        ]
        for name, env, prefs in runs:
            print(f"==> {name}")
            run(name, env, design, app, log, prefs)
    z = os.path.join(HOME, "solidcraft-diag.zip")
    with zipfile.ZipFile(z, "w", zipfile.ZIP_DEFLATED) as zf:
        for f in sorted(os.listdir(OUT)):
            zf.write(os.path.join(OUT, f), f)
    print(f"Kész: {z} — ezt küldd el.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
