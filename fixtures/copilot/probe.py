#!/usr/bin/env python3
"""Hit GitHub Copilot endpoints the same way official Copilot clients do.

Writes recordings into this directory as:
  device_code_response.json          POST github.com/login/device/code
  oauth_token_response.json          POST github.com/login/oauth/access_token (device-flow exchange)
  copilot_token_response.json        GET  api.github.com/copilot_internal/v2/token
  copilot_user_response.json         GET  api.github.com/copilot_internal/user
  github_user_response.json          GET  api.github.com/user
  github_user_emails_response.json   GET  api.github.com/user/emails
  copilot_user_401_response.json     GET  api.github.com/copilot_internal/user with a bogus token
                                     (only written when --simulate-bad-token is passed)

Output JSON contains OAuth tokens and account PII; do not publish or commit
unredacted captures.

Auth flow:
  GitHub's OAuth device flow with the public VS Code OAuth App client_id
  (Iv1.b507a08c87ecfe98). This is the same client_id used by every official
  Copilot integration (VS Code, copilot.vim, copilot-cli, etc.) and is required
  because GitHub App client_ids cannot exchange at copilot_internal/v2/token.

Token cache:
  After --login, the long-lived gho_... GitHub OAuth token is cached at
  ~/.cache/yapcap/copilot-probe-gh-token.json (outside the repo). Subsequent
  probe runs read it from there. Override with $YAPCAP_COPILOT_GH_TOKEN
  or $COPILOT_GH_TOKEN.

Usage:
  ./probe.py --login     # first time: walk through device flow, cache token
  ./probe.py             # probe all read endpoints (Copilot token + user + GH user)
  ./probe.py --user-only
  ./probe.py --token-only
  ./probe.py --id-only
  ./probe.py --simulate-bad-token
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

OAUTH_CLIENT_ID = "Iv1.b507a08c87ecfe98"
OAUTH_SCOPE = "read:user"

DEVICE_CODE_URL = "https://github.com/login/device/code"
ACCESS_TOKEN_URL = "https://github.com/login/oauth/access_token"
COPILOT_TOKEN_URL = "https://api.github.com/copilot_internal/v2/token"
COPILOT_USER_URL = "https://api.github.com/copilot_internal/user"
GITHUB_USER_URL = "https://api.github.com/user"
GITHUB_USER_EMAILS_URL = "https://api.github.com/user/emails"

EDITOR_VERSION = "vscode/1.96.2"
EDITOR_PLUGIN_VERSION = "copilot-chat/0.26.7"
USER_AGENT = "GitHubCopilotChat/0.26.7"
GITHUB_API_VERSION = "2026-03-10"

DEVICE_CODE_FILE = "device_code_response.json"
OAUTH_TOKEN_FILE = "oauth_token_response.json"
COPILOT_TOKEN_FILE = "copilot_token_response.json"
COPILOT_USER_FILE = "copilot_user_response.json"
GITHUB_USER_FILE = "github_user_response.json"
GITHUB_USER_EMAILS_FILE = "github_user_emails_response.json"
COPILOT_USER_401_FILE = "copilot_user_401_response.json"

INVALID_TOKEN_PLACEHOLDER = "gho_INVALID_PROBE_FORCED_INVALID_TOKEN_FOR_ERROR_CAPTURE"


def _cache_path() -> Path:
    base = os.environ.get("XDG_CACHE_HOME") or str(Path.home() / ".cache")
    return Path(base) / "yapcap" / "copilot-probe-gh-token.json"


def _iso_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _headers_to_dict(msg: Any) -> dict[str, str]:
    return {k: v for k, v in msg.items()}


def _save(out_dir: Path, name: str, record: dict[str, Any]) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / name
    path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def _request_record(
    *,
    method: str,
    url: str,
    headers: dict[str, str],
    body: bytes | None,
) -> tuple[int, dict[str, str], str]:
    req = Request(url, data=body, method=method)
    for k, v in headers.items():
        req.add_header(k, v)
    try:
        with urlopen(req, timeout=120) as resp:
            status = getattr(resp, "status", resp.getcode())
            raw = resp.read().decode("utf-8", errors="replace")
            hdrs = _headers_to_dict(resp.headers)
            return int(status), hdrs, raw
    except HTTPError as e:
        raw = e.read().decode("utf-8", errors="replace")
        hdrs = _headers_to_dict(e.headers) if e.headers else {}
        return int(e.code), hdrs, raw


def _record(endpoint: str, method: str, url: str, status: int, headers: dict[str, str], body: str) -> dict[str, Any]:
    parsed: Any
    try:
        parsed = json.loads(body)
    except json.JSONDecodeError:
        parsed = None
    return {
        "endpoint": endpoint,
        "method": method,
        "requested_at": _iso_now(),
        "url": url,
        "status_code": status,
        "response_headers": headers,
        "body_text": body,
        "body_json": parsed,
    }


def _copilot_editor_headers(token: str) -> dict[str, str]:
    return {
        "Authorization": f"token {token}",
        "Accept": "application/json",
        "Editor-Version": EDITOR_VERSION,
        "Editor-Plugin-Version": EDITOR_PLUGIN_VERSION,
        "User-Agent": USER_AGENT,
        "X-Github-Api-Version": GITHUB_API_VERSION,
    }


def probe_device_code() -> dict[str, Any]:
    form = urlencode({"client_id": OAUTH_CLIENT_ID, "scope": OAUTH_SCOPE}).encode("utf-8")
    headers = {
        "Content-Type": "application/x-www-form-urlencoded",
        "Accept": "application/json",
        "User-Agent": USER_AGENT,
    }
    status, resp_headers, body = _request_record(
        method="POST", url=DEVICE_CODE_URL, headers=headers, body=form,
    )
    return _record("device_code", "POST", DEVICE_CODE_URL, status, resp_headers, body)


def probe_access_token(device_code: str) -> dict[str, Any]:
    form = urlencode({
        "client_id": OAUTH_CLIENT_ID,
        "device_code": device_code,
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
    }).encode("utf-8")
    headers = {
        "Content-Type": "application/x-www-form-urlencoded",
        "Accept": "application/json",
        "User-Agent": USER_AGENT,
    }
    status, resp_headers, body = _request_record(
        method="POST", url=ACCESS_TOKEN_URL, headers=headers, body=form,
    )
    return _record("access_token", "POST", ACCESS_TOKEN_URL, status, resp_headers, body)


def probe_copilot_token(gh_token: str) -> dict[str, Any]:
    status, resp_headers, body = _request_record(
        method="GET",
        url=COPILOT_TOKEN_URL,
        headers=_copilot_editor_headers(gh_token),
        body=None,
    )
    return _record("copilot_token", "GET", COPILOT_TOKEN_URL, status, resp_headers, body)


def probe_copilot_user(gh_token: str) -> dict[str, Any]:
    status, resp_headers, body = _request_record(
        method="GET",
        url=COPILOT_USER_URL,
        headers=_copilot_editor_headers(gh_token),
        body=None,
    )
    return _record("copilot_user", "GET", COPILOT_USER_URL, status, resp_headers, body)


def probe_github_user(gh_token: str) -> dict[str, Any]:
    headers = {
        "Authorization": f"token {gh_token}",
        "Accept": "application/vnd.github+json",
        "User-Agent": USER_AGENT,
        "X-GitHub-Api-Version": "2022-11-28",
    }
    status, resp_headers, body = _request_record(
        method="GET", url=GITHUB_USER_URL, headers=headers, body=None,
    )
    return _record("github_user", "GET", GITHUB_USER_URL, status, resp_headers, body)


def probe_github_user_emails(gh_token: str) -> dict[str, Any]:
    headers = {
        "Authorization": f"token {gh_token}",
        "Accept": "application/vnd.github+json",
        "User-Agent": USER_AGENT,
        "X-GitHub-Api-Version": "2022-11-28",
    }
    status, resp_headers, body = _request_record(
        method="GET", url=GITHUB_USER_EMAILS_URL, headers=headers, body=None,
    )
    return _record("github_user_emails", "GET", GITHUB_USER_EMAILS_URL, status, resp_headers, body)


def _load_cached_token() -> str | None:
    path = _cache_path()
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        tok = data.get("access_token")
        if isinstance(tok, str) and tok:
            return tok
    except FileNotFoundError:
        return None
    except (OSError, json.JSONDecodeError):
        return None
    return None


def _store_cached_token(token: str) -> Path:
    path = _cache_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {"access_token": token, "stored_at": _iso_now()}
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    try:
        os.chmod(path, 0o600)
    except OSError:
        pass
    return path


def _resolve_token(args: argparse.Namespace) -> str | None:
    env = os.environ.get("YAPCAP_COPILOT_GH_TOKEN") or os.environ.get("COPILOT_GH_TOKEN")
    if env:
        return env.strip()
    if not args.no_local_state:
        cached = _load_cached_token()
        if cached:
            return cached
    return None


def run_login(out_dir: Path) -> int:
    rec = probe_device_code()
    path = _save(out_dir, DEVICE_CODE_FILE, rec)
    print(f"wrote {path}  (status {rec['status_code']})", file=sys.stderr)
    if rec["status_code"] >= 400 or not isinstance(rec["body_json"], dict):
        print("error: device code request failed", file=sys.stderr)
        return 1

    body = rec["body_json"]
    device_code = body.get("device_code")
    user_code = body.get("user_code")
    verification_uri = body.get("verification_uri") or "https://github.com/login/device"
    interval = int(body.get("interval") or 5)
    expires_in = int(body.get("expires_in") or 900)
    if not isinstance(device_code, str) or not isinstance(user_code, str):
        print("error: malformed device_code response", file=sys.stderr)
        return 1

    print("", file=sys.stderr)
    print(f"  Open {verification_uri}", file=sys.stderr)
    print(f"  Enter code: {user_code}", file=sys.stderr)
    print("  Polling for completion (Ctrl-C to abort)...", file=sys.stderr)
    print("", file=sys.stderr)

    deadline = time.monotonic() + expires_in
    poll_interval = max(interval, 1)
    last_rec: dict[str, Any] | None = None
    while time.monotonic() < deadline:
        time.sleep(poll_interval)
        last_rec = probe_access_token(device_code)
        body_json = last_rec.get("body_json") if isinstance(last_rec, dict) else None
        if not isinstance(body_json, dict):
            continue
        err = body_json.get("error")
        if err == "authorization_pending":
            continue
        if err == "slow_down":
            poll_interval += 5
            continue
        break

    if last_rec is None:
        print("error: device code expired before any poll", file=sys.stderr)
        return 1

    path = _save(out_dir, OAUTH_TOKEN_FILE, last_rec)
    print(f"wrote {path}  (status {last_rec['status_code']})", file=sys.stderr)

    body_json = last_rec.get("body_json") if isinstance(last_rec, dict) else None
    if not isinstance(body_json, dict):
        print("error: access_token poll returned non-JSON", file=sys.stderr)
        return 1
    if "error" in body_json:
        print(f"error: {body_json.get('error')}: {body_json.get('error_description')}", file=sys.stderr)
        return 1
    token = body_json.get("access_token")
    if not isinstance(token, str) or not token:
        print("error: no access_token in response", file=sys.stderr)
        return 1

    cache_path = _store_cached_token(token)
    print(f"cached token at {cache_path}", file=sys.stderr)
    return 0


def run_simulate_bad_token(out_dir: Path) -> int:
    rec = probe_copilot_user(INVALID_TOKEN_PLACEHOLDER)
    path = _save(out_dir, COPILOT_USER_401_FILE, rec)
    print(f"wrote {path}  (status {rec['status_code']})", file=sys.stderr)
    return 0


def main() -> int:
    default_out = Path(__file__).resolve().parent

    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out-dir", type=Path, default=default_out,
                   help=f"Directory for JSON recordings (default: {default_out})")
    p.add_argument("--login", action="store_true",
                   help="Run GitHub device flow and cache the resulting OAuth token")
    p.add_argument("--no-local-state", action="store_true",
                   help="Do not read token from cache; use environment only")
    p.add_argument("--token-only", action="store_true",
                   help="Only probe copilot_internal/v2/token")
    p.add_argument("--user-only", action="store_true",
                   help="Only probe copilot_internal/user")
    p.add_argument("--id-only", action="store_true",
                   help="Only probe /user and /user/emails")
    p.add_argument("--simulate-bad-token", action="store_true",
                   help="Hit copilot_internal/user with a bogus token; save 401 response")
    args = p.parse_args()

    out_dir: Path = args.out_dir

    if args.login:
        return run_login(out_dir)
    if args.simulate_bad_token:
        return run_simulate_bad_token(out_dir)

    exclusive = sum(1 for v in (args.token_only, args.user_only, args.id_only) if v)
    if exclusive > 1:
        print("error: use at most one of --token-only / --user-only / --id-only", file=sys.stderr)
        return 2

    token = _resolve_token(args)
    if not token:
        print(
            "error: no GitHub OAuth token (run with --login, or set "
            "YAPCAP_COPILOT_GH_TOKEN / COPILOT_GH_TOKEN)",
            file=sys.stderr,
        )
        return 1

    try:
        rc = 0
        if args.token_only:
            rec = probe_copilot_token(token)
            path = _save(out_dir, COPILOT_TOKEN_FILE, rec)
            print(f"wrote {path}  (status {rec['status_code']})", file=sys.stderr)
            return 0 if int(rec["status_code"]) < 400 else 1
        if args.user_only:
            rec = probe_copilot_user(token)
            path = _save(out_dir, COPILOT_USER_FILE, rec)
            print(f"wrote {path}  (status {rec['status_code']})", file=sys.stderr)
            return 0 if int(rec["status_code"]) < 400 else 1
        if args.id_only:
            rec = probe_github_user(token)
            path = _save(out_dir, GITHUB_USER_FILE, rec)
            print(f"wrote {path}  (status {rec['status_code']})", file=sys.stderr)
            rec2 = probe_github_user_emails(token)
            path2 = _save(out_dir, GITHUB_USER_EMAILS_FILE, rec2)
            print(f"wrote {path2}  (status {rec2['status_code']})", file=sys.stderr)
            return 0 if int(rec["status_code"]) < 400 else 1

        for prober, name in (
            (probe_copilot_token, COPILOT_TOKEN_FILE),
            (probe_copilot_user, COPILOT_USER_FILE),
            (probe_github_user, GITHUB_USER_FILE),
            (probe_github_user_emails, GITHUB_USER_EMAILS_FILE),
        ):
            rec = prober(token)
            path = _save(out_dir, name, rec)
            print(f"wrote {path}  (status {rec['status_code']})", file=sys.stderr)
            if int(rec["status_code"]) >= 400:
                rc = 1
        return rc
    except URLError as e:
        print(f"error: request failed: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
