#!/usr/bin/env python3
"""Hit Google Antigravity's Code Assist endpoints the way the Antigravity CLI/IDE does.

Antigravity reuses the same cloudcode-pa.googleapis.com/v1internal:* host as
gemini-cli, but with ideType=ANTIGRAVITY metadata and its own OAuth client. This
probe mirrors ../gemini/probe.py.

Writes recordings into this directory as:
  oauth_token_response.json               POST oauth2.googleapis.com/token (refresh_token grant)
  load_code_assist_response.json          POST cloudcode-pa.googleapis.com/v1internal:loadCodeAssist
  fetch_available_models_response.json     POST cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels
  retrieve_user_quota_response.json        POST cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary
  oauth_token_400_response.json           POST oauth2.googleapis.com/token with a bogus refresh_token
                                          (only written when --simulate-bad-refresh is passed)

Output JSON may contain OAuth tokens and account PII; do not publish or commit unredacted captures.

Credentials (later wins per field: file, then environment):
  Antigravity CLI/IDE state: ~/.gemini/oauth_creds_ag.json
    (same shape as gemini-cli's oauth_creds.json, but for the Antigravity client)
    Fields used: access_token, refresh_token, expiry_date (Unix ms), id_token.
  Environment (optional overrides):
    ANTIGRAVITY_REFRESH_TOKEN or YAPCAP_ANTIGRAVITY_REFRESH_TOKEN
    ANTIGRAVITY_ACCESS_TOKEN  or YAPCAP_ANTIGRAVITY_ACCESS_TOKEN
    ANTIGRAVITY_PROJECT_ID    or YAPCAP_ANTIGRAVITY_PROJECT_ID  (skips loadCodeAssist discovery)
    ANTIGRAVITY_CLIENT_ID / ANTIGRAVITY_CLIENT_SECRET (override the OAuth client pair)

The OAuth client id/secret pairs below are the public values embedded in the
Antigravity CLI binary (`agy`), extracted via `strings`. There are two clients in
the wild (IDE vs CLI / version drift). The probe reads the `aud` claim from the
stored id_token to pick the matching client, and falls back to trying every known
secret if a refresh_token grant is rejected as invalid_client.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

# Public Antigravity OAuth clients, extracted from the `agy` CLI binary.
# Keyed by client_id; each maps to the secret embedded alongside it.
KNOWN_CLIENTS: dict[str, str] = {
    "884354919052-36trc1jjb3tguiac32ov6cod268c5blh.apps.googleusercontent.com":
        "GOCSPX-9YQWpF7RWDC0QTdj-YxKMwR0Zts",
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com":
        "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",
}

TOKEN_URL = "https://oauth2.googleapis.com/token"

# Code Assist host. Production is cloudcode-pa.googleapis.com; Antigravity "daily"
# builds talk to daily-cloudcode-pa.googleapis.com. Override with
# ANTIGRAVITY_CODE_ASSIST_HOST (host only, no scheme).
CODE_ASSIST_HOST = (
    os.environ.get("ANTIGRAVITY_CODE_ASSIST_HOST", "").strip()
    or "cloudcode-pa.googleapis.com"
)
_BASE = f"https://{CODE_ASSIST_HOST}/v1internal"
LOAD_CODE_ASSIST_URL = f"{_BASE}:loadCodeAssist"
FETCH_MODELS_URL = f"{_BASE}:fetchAvailableModels"
QUOTA_URL = f"{_BASE}:retrieveUserQuotaSummary"

USER_AGENT = "antigravity"

IDE_METADATA = {
    "ideType": "ANTIGRAVITY",
    "platform": "PLATFORM_UNSPECIFIED",
    "pluginType": "GEMINI",
    "duetProject": "default",
}

TOKEN_RESPONSE_FILE = "oauth_token_response.json"
LOAD_CODE_ASSIST_FILE = "load_code_assist_response.json"
FETCH_MODELS_FILE = "fetch_available_models_response.json"
QUOTA_RESPONSE_FILE = "retrieve_user_quota_response.json"
TOKEN_400_RESPONSE_FILE = "oauth_token_400_response.json"

INVALID_REFRESH_TOKEN_PLACEHOLDER = (
    "1//0eINVALID-PROBE-FORCED-INVALID-REFRESH-TOKEN-FOR-ERROR-CAPTURE"
)


def _default_creds_path() -> Path:
    return Path.home() / ".gemini" / "oauth_creds_ag.json"


def _read_stored_creds(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise OSError(f"{path}: expected JSON object")
    return data


def _env(*names: str) -> str | None:
    for name in names:
        v = os.environ.get(name)
        if v:
            return v.strip()
    return None


def _iso_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _headers_to_dict(msg: Any) -> dict[str, str]:
    return {k: v for k, v in msg.items()}


def _save(out_dir: Path, name: str, record: dict[str, Any]) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / name
    path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def _jwt_aud(id_token: Any) -> str | None:
    if not isinstance(id_token, str) or id_token.count(".") != 2:
        return None
    payload = id_token.split(".")[1]
    payload += "=" * (-len(payload) % 4)
    try:
        claims = json.loads(base64.urlsafe_b64decode(payload))
    except (ValueError, json.JSONDecodeError):
        return None
    aud = claims.get("aud")
    return aud if isinstance(aud, str) and aud.strip() else None


def _candidate_clients(preferred_id: str | None) -> list[tuple[str, str]]:
    pairs: list[tuple[str, str]] = []
    if preferred_id and preferred_id in KNOWN_CLIENTS:
        pairs.append((preferred_id, KNOWN_CLIENTS[preferred_id]))
    for cid, secret in KNOWN_CLIENTS.items():
        if (cid, secret) not in pairs:
            pairs.append((cid, secret))
    return pairs


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


def probe_token(refresh_token: str, client_id: str, client_secret: str) -> dict[str, Any]:
    form = urlencode({
        "client_id": client_id,
        "client_secret": client_secret,
        "refresh_token": refresh_token,
        "grant_type": "refresh_token",
    }).encode("utf-8")
    headers = {
        "Content-Type": "application/x-www-form-urlencoded",
        "Accept": "application/json",
        "User-Agent": USER_AGENT,
    }
    status, resp_headers, body = _request_record(
        method="POST", url=TOKEN_URL, headers=headers, body=form,
    )
    return _record("oauth_token", "POST", TOKEN_URL, status, resp_headers, body)


def _post_json(endpoint: str, url: str, access_token: str, body_obj: dict[str, Any]) -> dict[str, Any]:
    payload = json.dumps(body_obj).encode("utf-8")
    headers = {
        "Authorization": f"Bearer {access_token}",
        "Content-Type": "application/json",
        "Accept": "application/json",
        "User-Agent": USER_AGENT,
    }
    status, resp_headers, body = _request_record(
        method="POST", url=url, headers=headers, body=payload,
    )
    return _record(endpoint, "POST", url, status, resp_headers, body)


def probe_load_code_assist(access_token: str) -> dict[str, Any]:
    return _post_json("load_code_assist", LOAD_CODE_ASSIST_URL, access_token, {"metadata": IDE_METADATA})


def probe_fetch_models(access_token: str, project_id: str | None) -> dict[str, Any]:
    body_obj: dict[str, Any] = {"project": project_id} if project_id else {}
    return _post_json("fetch_available_models", FETCH_MODELS_URL, access_token, body_obj)


def probe_quota(access_token: str, project_id: str | None) -> dict[str, Any]:
    body_obj: dict[str, Any] = {"project": project_id} if project_id else {}
    return _post_json("retrieve_user_quota", QUOTA_URL, access_token, body_obj)


def _extract_project_id(load_code_assist_body: Any) -> str | None:
    if not isinstance(load_code_assist_body, dict):
        return None
    direct = load_code_assist_body.get("cloudaicompanionProject")
    if isinstance(direct, str) and direct.strip():
        return direct.strip()
    tier = load_code_assist_body.get("currentTier")
    if isinstance(tier, dict):
        inner = tier.get("cloudaicompanionProject")
        if isinstance(inner, str) and inner.strip():
            return inner.strip()
    for t in load_code_assist_body.get("allowedTiers") or []:
        if isinstance(t, dict):
            inner = t.get("cloudaicompanionProject")
            if isinstance(inner, str) and inner.strip():
                return inner.strip()
    return None


def _ms_to_iso(ms: Any) -> str | None:
    try:
        seconds = int(ms) / 1000.0
    except (TypeError, ValueError):
        return None
    return datetime.fromtimestamp(seconds, tz=timezone.utc).isoformat()


def _token_expired_soon(expiry_date_ms: Any, skew_seconds: int = 300) -> bool:
    try:
        expiry = int(expiry_date_ms) / 1000.0
    except (TypeError, ValueError):
        return True
    return datetime.now(timezone.utc).timestamp() + skew_seconds >= expiry


def _looks_like_invalid_client(rec: dict[str, Any]) -> bool:
    if int(rec.get("status_code", 0)) < 400:
        return False
    body = rec.get("body_json")
    if isinstance(body, dict):
        err = str(body.get("error", "")).lower()
        return err in {"invalid_client", "unauthorized_client"}
    return False


def _refresh_with_fallback(refresh_token: str, preferred_id: str | None, out_dir: Path) -> dict[str, Any]:
    candidates = _candidate_clients(preferred_id)
    last: dict[str, Any] | None = None
    for cid, secret in candidates:
        rec = probe_token(refresh_token, cid, secret)
        last = rec
        short = cid.split("-", 1)[0]
        print(f"  tried client {short}… -> status {rec['status_code']}", file=sys.stderr)
        if int(rec["status_code"]) < 400 or not _looks_like_invalid_client(rec):
            return rec
    return last or {}


def main() -> int:
    default_out = Path(__file__).resolve().parent
    default_creds = _default_creds_path()

    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out-dir", type=Path, default=default_out,
                   help=f"Directory for JSON recordings (default: {default_out})")
    p.add_argument("--creds-file", type=Path, default=default_creds,
                   help=f"Path to Antigravity oauth_creds_ag.json (default: {default_creds})")
    p.add_argument("--no-local-state", action="store_true",
                   help="Do not read tokens from ~/.gemini; use environment only")
    p.add_argument("--force-refresh", action="store_true",
                   help="Always refresh even if stored access token has not expired")
    p.add_argument("--token-only", action="store_true",
                   help="Only probe the OAuth token endpoint")
    p.add_argument("--load-only", action="store_true",
                   help="Only probe loadCodeAssist (skip model/quota fetches)")
    p.add_argument("--quota-only", action="store_true",
                   help="Only probe the quota fetches (skip loadCodeAssist; needs --project-id or env)")
    p.add_argument("--project-id", default=None,
                   help="Override the GCP project id (skips/augments loadCodeAssist discovery)")
    p.add_argument("--simulate-bad-refresh", action="store_true",
                   help=("POST a deliberately invalid refresh_token to the OAuth token endpoint "
                         "and save the 4xx response as oauth_token_400_response.json."))
    args = p.parse_args()

    if args.simulate_bad_refresh:
        cid, secret = next(iter(KNOWN_CLIENTS.items()))
        rec = probe_token(INVALID_REFRESH_TOKEN_PLACEHOLDER, cid, secret)
        path = _save(args.out_dir, TOKEN_400_RESPONSE_FILE, rec)
        print(f"wrote {path}  (status {rec['status_code']})", file=sys.stderr)
        return 0
    out_dir: Path = args.out_dir

    exclusive = sum(1 for v in (args.token_only, args.load_only, args.quota_only) if v)
    if exclusive > 1:
        print("error: use at most one of --token-only / --load-only / --quota-only", file=sys.stderr)
        return 2

    refresh: str | None = None
    access: str | None = None
    expiry_ms: Any = None
    preferred_client: str | None = None

    if not args.no_local_state:
        try:
            creds = _read_stored_creds(args.creds_file)
            refresh = (creds.get("refresh_token") or None) if isinstance(creds.get("refresh_token"), str) else None
            access = (creds.get("access_token") or None) if isinstance(creds.get("access_token"), str) else None
            expiry_ms = creds.get("expiry_date")
            preferred_client = _jwt_aud(creds.get("id_token"))
            print(f"using creds from {args.creds_file} (expiry {_ms_to_iso(expiry_ms)})", file=sys.stderr)
            if preferred_client:
                print(f"id_token aud -> client {preferred_client.split('-', 1)[0]}…", file=sys.stderr)
        except FileNotFoundError:
            print(f"note: {args.creds_file} not found; relying on environment", file=sys.stderr)
        except OSError as e:
            print(f"error: {e}", file=sys.stderr)
            return 1

    refresh = _env("ANTIGRAVITY_REFRESH_TOKEN", "YAPCAP_ANTIGRAVITY_REFRESH_TOKEN") or refresh
    access = _env("ANTIGRAVITY_ACCESS_TOKEN", "YAPCAP_ANTIGRAVITY_ACCESS_TOKEN") or access
    project_override = args.project_id or _env("ANTIGRAVITY_PROJECT_ID", "YAPCAP_ANTIGRAVITY_PROJECT_ID")

    env_client_id = _env("ANTIGRAVITY_CLIENT_ID")
    env_client_secret = _env("ANTIGRAVITY_CLIENT_SECRET")
    if env_client_id and env_client_secret:
        KNOWN_CLIENTS.clear()
        KNOWN_CLIENTS[env_client_id] = env_client_secret
        preferred_client = env_client_id

    try:
        expiry_known = expiry_ms not in (None, "")
        should_refresh = (
            args.token_only
            or args.force_refresh
            or not access
            or (expiry_known and _token_expired_soon(expiry_ms))
        )
        if should_refresh:
            if not refresh:
                print(
                    "error: no refresh_token (log into Antigravity or set "
                    "ANTIGRAVITY_REFRESH_TOKEN / YAPCAP_ANTIGRAVITY_REFRESH_TOKEN)",
                    file=sys.stderr,
                )
                return 1
            rec = _refresh_with_fallback(refresh, preferred_client, out_dir)
            path = _save(out_dir, TOKEN_RESPONSE_FILE, rec)
            print(f"wrote {path}  (status {rec['status_code']})", file=sys.stderr)
            if isinstance(rec.get("body_json"), dict):
                at = rec["body_json"].get("access_token")
                if isinstance(at, str) and at:
                    access = at
            if args.token_only:
                return 0 if int(rec["status_code"]) < 400 else 1

        if not access:
            print("error: no access token available after refresh", file=sys.stderr)
            return 1

        project_id = project_override
        if not args.quota_only:
            lrec = probe_load_code_assist(access)
            lpath = _save(out_dir, LOAD_CODE_ASSIST_FILE, lrec)
            print(f"wrote {lpath}  (status {lrec['status_code']})", file=sys.stderr)
            if not project_id:
                project_id = _extract_project_id(lrec.get("body_json"))
                if project_id:
                    print(f"discovered project={project_id}", file=sys.stderr)
            if args.load_only:
                return 0 if int(lrec["status_code"]) < 400 else 1

        frec = probe_fetch_models(access, project_id)
        fpath = _save(out_dir, FETCH_MODELS_FILE, frec)
        print(f"wrote {fpath}  (status {frec['status_code']})", file=sys.stderr)

        qrec = probe_quota(access, project_id)
        qpath = _save(out_dir, QUOTA_RESPONSE_FILE, qrec)
        print(f"wrote {qpath}  (status {qrec['status_code']})", file=sys.stderr)

        ok = int(frec["status_code"]) < 400 or int(qrec["status_code"]) < 400
        return 0 if ok else 1
    except URLError as e:
        print(f"error: request failed: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
