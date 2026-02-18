#!/usr/bin/env python3
"""MOAR Manager Sidecar — handles git pull + rebuild via HTTP API."""

import json
import os
import subprocess
import threading
import time
from http.server import HTTPServer, BaseHTTPRequestHandler

STATUS_FILE = "/status/update.json"
PROJECT_DIR = "/project"
MANAGER_SECRET = os.environ.get("MANAGER_SECRET", "")

_lock = threading.Lock()


def read_status():
    try:
        with open(STATUS_FILE, "r") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {"status": "idle"}


def write_status(status, message=""):
    os.makedirs(os.path.dirname(STATUS_FILE), exist_ok=True)
    data = {"status": status}
    if message:
        data["message"] = message
    if status in ("pulling", "building"):
        data["started_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    if status in ("complete", "error"):
        data["completed_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    with open(STATUS_FILE, "w") as f:
        json.dump(data, f)


def run_update():
    """Run git pull + docker compose rebuild in background."""
    if not _lock.acquire(blocking=False):
        return False

    def do_update():
        try:
            write_status("pulling")
            result = subprocess.run(
                ["git", "pull", "--ff-only"],
                cwd=PROJECT_DIR,
                capture_output=True,
                text=True,
                timeout=120,
            )
            if result.returncode != 0:
                write_status("error", f"git pull failed: {result.stderr.strip()}")
                return

            write_status("building")
            result = subprocess.run(
                [
                    "docker", "compose", "-p", "moar",
                    "up", "-d", "--build",
                    "server", "admin", "caddy",
                ],
                cwd=PROJECT_DIR,
                capture_output=True,
                text=True,
                timeout=600,
            )
            if result.returncode != 0:
                write_status("error", f"docker compose build failed: {result.stderr.strip()}")
                return

            write_status("complete", "Update successful")
        except subprocess.TimeoutExpired:
            write_status("error", "Update timed out")
        except Exception as e:
            write_status("error", str(e))
        finally:
            _lock.release()

    thread = threading.Thread(target=do_update, daemon=True)
    thread.start()
    return True


def check_auth(headers):
    auth = headers.get("Authorization", "")
    if not MANAGER_SECRET:
        return False
    return auth == f"Bearer {MANAGER_SECRET}"


class Handler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        # Simple logging
        print(f"[manager] {args[0]}")

    def do_GET(self):
        if self.path == "/health":
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"ok")
            return

        if self.path == "/status":
            status = read_status()
            body = json.dumps(status).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body)
            return

        self.send_error(404)

    def do_POST(self):
        if self.path == "/update":
            if not check_auth(self.headers):
                self.send_error(401, "Unauthorized")
                return

            current = read_status()
            if current.get("status") in ("pulling", "building"):
                self.send_response(409)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"error": "Update already in progress"}).encode())
                return

            started = run_update()
            if started:
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"status": "started"}).encode())
            else:
                self.send_response(409)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"error": "Update already in progress"}).encode())
            return

        self.send_error(404)


if __name__ == "__main__":
    if not MANAGER_SECRET:
        print("[manager] WARNING: MANAGER_SECRET not set — all requests will be rejected")

    # Initialize status file
    write_status("idle")

    server = HTTPServer(("0.0.0.0", 9090), Handler)
    print("[manager] Listening on port 9090")
    server.serve_forever()
