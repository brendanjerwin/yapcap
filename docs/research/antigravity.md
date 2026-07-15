# Google Antigravity — usage/quota & auth research

Research date: **2026-07-14**. Antigravity changes fast; treat version-specific
values (client IDs, app version strings, model names) as snapshots and re-verify
before shipping.

Goal: gather enough to add an Antigravity provider to YapCap using Google-OAuth
auth, mirroring the existing Gemini provider (which reuses gemini-cli's public
OAuth client and calls `cloudcode-pa.googleapis.com/v1internal:*`).

Legend: **[CONFIRMED]** = seen in source code / official docs. **[INFERRED]** =
deduced or reported second-hand. **[UNKNOWN]** = not established.

---

## 0. Confirmed live findings (2026-07-14, direct API capture)

This section supersedes the second-hand research below where they conflict. It is
based on real HTTP 200 captures from a signed-in Antigravity install on this
machine, recorded via `fixtures/antigravity/probe.py`. The install is a **daily
build** (see host caveat).

**Auth / token storage — [CONFIRMED]:**

- The Antigravity **CLI (`agy`) and IDE store the OAuth token in the OS keyring**,
  not in a file. On Linux (Secret Service / libsecret) the item is
  `service=gemini, username=antigravity`, label `Password for 'antigravity' on 'gemini'`.
  The stored value is JSON: `{"token": {access_token, token_type:"Bearer",
  refresh_token, expiry (RFC3339 string)}, "auth_method": "consumer"}`.
  The legacy file `~/.gemini/oauth_creds_ag.json` exists only if an older CLI wrote
  it; current builds do **not** create or refresh it. go-keyring falls back to a
  file only if the keyring write fails ("Failed to save token to keyring, falling
  back to file").
- Auth flow (from `agy` log/binary strings): "Starting GCP OAuth authentication
  flow" → browser → "Paste the authorization code below:" (paste-code flow, like
  Claude's). Requested scope includes `https://www.googleapis.com/auth/aicode`.
- OAuth clients embedded in the `agy` binary (two, both public):
  - **`1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com`
    + secret `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` — the working pair.** Verified:
    a `refresh_token` grant with this pair against `oauth2.googleapis.com/token`
    returns **HTTP 200** with a full token payload, and the current keyring refresh
    token belongs to this client. **This is the pair YapCap should use for its own
    OAuth flow** (analogous to the Gemini provider reusing the gemini-cli client).
  - `884354919052-36trc1jjb3tguiac32ov6cod268c5blh.apps.googleusercontent.com`
    + secret `GOCSPX-9YQWpF7RWDC0QTdj-YxKMwR0Zts` — a second embedded client;
    `invalid_client` from the probe. Treat client id/secret as env-overridable
    rather than hard-relying on one pair, since the binary ships two.
- **Token-refresh response — [CONFIRMED]:** keys `access_token`, `expires_in`
  (3599), `id_token`, `scope`, `token_type` (`Bearer`). **No `refresh_token` is
  returned** on refresh — YapCap must keep reusing the stored refresh token (same
  as the Gemini/Codex providers). The granted `scope` is the 7-scope set
  `openid https://www.googleapis.com/auth/aicode
  https://www.googleapis.com/auth/cclog
  https://www.googleapis.com/auth/cloud-platform
  https://www.googleapis.com/auth/experimentsandconfigs
  https://www.googleapis.com/auth/userinfo.email
  https://www.googleapis.com/auth/userinfo.profile` — note the Antigravity-specific
  `aicode`, `cclog`, and `experimentsandconfigs` scopes beyond gemini-cli's set.

**Endpoints — [CONFIRMED]:** `POST` to `<host>/v1internal:` `loadCodeAssist`,
`fetchAvailableModels`, and **`retrieveUserQuotaSummary`** (note the `Summary`
suffix — the current name; earlier research said `retrieveUserQuota`). Headers:
`Authorization: Bearer <access_token>`, `Content-Type: application/json`,
`User-Agent: antigravity`. `loadCodeAssist` body uses metadata
`{"ideType":"ANTIGRAVITY","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}`.
`retrieveUserQuotaSummary` takes `{"project": "<cloudaicompanionProject>"}`.

> **The `project` field is NOT safely optional — [CONFIRMED, live-verified 2026-07-15].**
> Earlier research claimed an empty body works because the server resolves the
> project from the token. That is true *only for paid accounts*. On a **free**
> account an empty body silently returns a **degraded, wrong** response: a single
> `All Models` group of eight per-model buckets (`gemini-3.5-flash-low`,
> `claude-opus-4-6-thinking`, …), each with **no `window` field** and a flat
> `remainingFraction: 1` — i.e. it reports zero usage on an account that has
> really consumed ~5%. Same token, same host, same headers, `{"project": …}`:
> the normal grouped shape with real numbers (`gemini-weekly` 0.9471,
> `3p-weekly` 0.9381). Paid accounts return byte-identical responses either way,
> which is why the empty body survived the original live QA (issue 007) — it was
> only ever exercised against a Google-AI-Pro account. Matrix:
>
> | account | `{}` | `{"project": …}` |
> |---|---|---|
> | free | 1 group `All Models`, 8 per-model buckets, no `window`, all `1.0` | 2 groups × weekly, real usage |
> | paid | 2 groups × {weekly, 5h} | identical |
>
> `User-Agent: antigravity` is **required** on the quota call regardless of body —
> without it (or with `gemini-cli`) the endpoint returns 403 `PERMISSION_DENIED`.

> **Host — [CONFIRMED, prod live-verified 2026-07-14]:** the production host is
> **`cloudcode-pa.googleapis.com`** (the gemini-cli host). Verified by re-running
> the probe against it with a live YapCap-minted token (issue 001): `loadCodeAssist`,
> `fetchAvailableModels`, and `retrieveUserQuotaSummary` all return **200**, and the
> quota JSON is structurally identical to the earlier daily-build capture
> (`groups[] × buckets[] × {bucketId, displayName, remainingFraction, resetTime,
> window, description}`). The daily build talked to `daily-cloudcode-pa.googleapis.com`;
> the probe still takes `ANTIGRAVITY_CODE_ASSIST_HOST` to switch. `cloudcode-pa.googleapis.com`
> is the shipped default; the committed fixtures remain valid (prod shape matches).

**Quota shape — [CONFIRMED], supersedes §1/§4 Pro/Flash/Lite classification.**
`retrieveUserQuotaSummary` returns server-defined **groups**. It does *not* return
per-model buckets for the usage summary (given a `project` — see above), and there
is no "lite" family. **Bucket count is tier-dependent — [CONFIRMED, live-verified
2026-07-15]:** paid accounts get a **weekly** *and* a **5-hour** bucket per group
(4 bars); free accounts get **weekly only** (2 bars), consistent with free tier
having no 5-hour cap. Group `displayName`s and bucket labels are identical across
tiers, so no tier-specific client rendering is needed. Captured paid shape:

```
groups: [
  { displayName: "Gemini Models",
    description: "Models within this group: Gemini Flash, Gemini Pro",
    buckets: [
      { bucketId:"gemini-weekly", displayName:"Weekly Limit",    window:"weekly", remainingFraction, resetTime, description },
      { bucketId:"gemini-5h",     displayName:"Five Hour Limit", window:"5h",     remainingFraction, resetTime, description } ] },
  { displayName: "Claude and GPT models",
    description: "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
    buckets: [
      { bucketId:"3p-weekly", displayName:"Weekly Limit",    window:"weekly", remainingFraction, resetTime },
      { bucketId:"3p-5h",     displayName:"Five Hour Limit", window:"5h",     remainingFraction, resetTime } ] } ]
```

- `remainingFraction` is 0.0–1.0 (used_percent = `(1 - remainingFraction) * 100`).
- `resetTime` is RFC3339 UTC. `window` ∈ `weekly | 5h`. `description` is
  human-readable ("...it will fully refresh in 6 days, 23 hours.") and present on
  some buckets only.
- **Verified experimentally:** using Gemini 3.1 **Pro** decremented the *same*
  `gemini-weekly` / `gemini-5h` counters that **Flash** usage moved, with reset
  times unchanged — Pro and Flash share one bucket. The 5h counter dropped more
  for Pro, matching the response's note that quota is "consumed proportionally to
  the cost of the tokens." The Claude/GPT group stayed at 1.0 (separate bucket).

**Tier — [CONFIRMED]** from `loadCodeAssist`: `currentTier.id` observed `free-tier`
(name "Antigravity"); `allowedTiers` includes `standard-tier`; `paidTier.id`
`g1-pro-tier` (name "Google AI Pro"). `cloudaicompanionProject` present
(e.g. `mimetic-team-7tjsh`). Same tier fields the Gemini provider already reads.

> **`currentTier.id` is the wrong plan signal — [CONFIRMED, live-verified 2026-07-15].**
> Two findings, both unhandled today (tracked separately; the quota fix does not
> depend on them):
>
> 1. **`paidTier` is the real entitlement.** A Google-AI-Pro account and a genuinely
>    free account *both* report `currentTier.id: free-tier`. They differ only in
>    `paidTier`: `g1-pro-tier` ("Google AI Pro") vs `free-tier` ("Antigravity
>    Starter Quota"). The free account's bucket layout matches its `paidTier`,
>    not its `currentTier`.
> 2. **`currentTier.id` is User-Agent dependent.** Same token, same body, prod
>    host: with `User-Agent: antigravity` the Pro account reports
>    `currentTier.id: free-tier`; **without** the header it reports
>    `standard-tier`. YapCap omits the header on `loadCodeAssist` but sends it on
>    the quota call, so the current "Pro" plan badge is correct only by accident
>    and would flip to "Free" if the headers were made consistent. The quota
>    endpoint 403s without the header, so it cannot simply be dropped everywhere.

**YapCap model (implemented):**

- **One `UsageWindow` per bucket**, grouped — paid: Gemini Weekly, Gemini 5h,
  Claude+GPT Weekly, Claude+GPT 5h; free: Gemini Weekly, Claude+GPT Weekly.
  Section titles from `groups[].displayName`; per-bar labels from
  `buckets[].displayName`.
- **Panel headline (2 thin bars):** the two **5-hour** windows (Gemini 5h +
  Claude/GPT 5h) — the fast-moving ambient signal. Free tier has no 5-hour
  windows, so the applet falls back to the first two windows: the two weekly
  bars. Popup shows all bars.
- Group labels come from the server, so — unlike the Gemini provider — YapCap does
  **not** need to hard-code a model→family classifier for the usage summary.

**User-facing models — [CONFIRMED]** from `fetchAvailableModels`
`agentModelSorts` "Recommended" group (this daily build): `gemini-3.5-flash-low`,
`gemini-3-flash-agent`, `gemini-3.5-flash-extra-low`, `gemini-3.1-pro-low`,
`gemini-pro-agent`, `claude-sonnet-4-6`, `claude-opus-4-6-thinking`,
`gpt-oss-120b-medium`. `defaultAgentModelId` = `gemini-3.5-flash-low`. The
`models` object also lists ~dozens of internal models (`isInternal: true`) mostly
at `remainingFraction: 1` — **not** the usage source; use
`retrieveUserQuotaSummary` for usage. These model ids differ from the older
`gemini-3-pro-high` / `gemini-3-pro-low` names in the second-hand research below,
confirming the naming drifts between builds.

### 0.1 Reproducing the captures (for the next agent)

The probe is `fixtures/antigravity/probe.py`. Because the CLI/IDE keep the token
in the **keyring** (no creds file), the token must be pulled from Secret Service
and passed via env. On this machine (`python-dbus` is installed system-wide):

```python
# read the live Antigravity token from the OS keyring
import dbus, json
bus = dbus.SessionBus()
svc = dbus.Interface(bus.get_object('org.freedesktop.secrets', '/org/freedesktop/secrets'),
                     'org.freedesktop.Secret.Service')
session = svc.OpenSession('plain', '')[1]
# find the item whose attributes are {service:'gemini', username:'antigravity'};
# it was /collection/login/17 here — enumerate items and match attributes rather
# than hard-coding the index.
item = dbus.Interface(bus.get_object('org.freedesktop.secrets',
       '/org/freedesktop/secrets/collection/login/17'), 'org.freedesktop.Secret.Item')
tok = json.loads(bytes(item.GetSecret(session)[2]).decode())['token']
# tok = {access_token, token_type, refresh_token, expiry(RFC3339)}
```

Then run the probe (daily build → daily host; use the working client for refresh):

```sh
export ANTIGRAVITY_ACCESS_TOKEN=<tok.access_token>     # or ANTIGRAVITY_REFRESH_TOKEN
export ANTIGRAVITY_REFRESH_TOKEN=<tok.refresh_token>
export ANTIGRAVITY_CODE_ASSIST_HOST=daily-cloudcode-pa.googleapis.com   # omit for prod host
export ANTIGRAVITY_CLIENT_ID=1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com
export ANTIGRAVITY_CLIENT_SECRET=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
python3 fixtures/antigravity/probe.py --no-local-state            # uses access token directly
python3 fixtures/antigravity/probe.py --no-local-state --force-refresh   # exercises token refresh too
python3 fixtures/antigravity/probe.py --simulate-bad-refresh      # NOTE: runs before the env client
                                                                  # override; for a real invalid_grant
                                                                  # capture, POST a bogus refresh token
                                                                  # with the working client directly.
```

The access token is short-lived (~1h). Re-read the keyring each session. The probe
still also reads `~/.gemini/oauth_creds_ag.json` as a fallback if present, but
current builds do not write it.

### 0.2 Captured fixtures (redacted, in `fixtures/antigravity/`)

| File | HTTP | Purpose |
| --- | --- | --- |
| `oauth_token_response.json` | 200 | Token-refresh success shape (tokens redacted, fake id_token) |
| `oauth_token_400_response.json` | 400 | `invalid_grant` — expired/bad refresh token, re-auth path |
| `load_code_assist_response.json` | 200 | Tier + project discovery (project id/email/privacy notice redacted) |
| `fetch_available_models_response.json` | 200 | Model catalog (secondary; no usage) |
| `retrieve_user_quota_response.json` | 200 | **The usage payload YapCap parses** (groups × weekly/5h) |

Redaction matches the Gemini fixture convention: access/refresh tokens →
`redacted-*`, `id_token` → unsigned JWT for `user@example.com`, project id →
`redacted-project-id`, privacy `noticeText` → `redacted privacy notice`. Verified
no `ya29.` / `1//0` / real email / project id remains.

---

## 1. What Antigravity is today (July 2026)

- **[CONFIRMED]** Antigravity is Google's agentic IDE (a VS Code / Electron fork,
  bundle id `com.google.antigravity` / `com.google.antigravity-ide`). Source:
  CodexBar `AntigravityOAuthCredentialsStore.swift` `isAntigravityAppBundle`.
- **[CONFIRMED]** There is now also an **Antigravity CLI** (`agy` /
  `antigravity-cli`), separate from gemini-cli. CodexBar probes "either the IDE or
  the CLI (agy / antigravity-cli) language server." The CLI has its own config at
  `~/.config/antigravity/config.toml` and stores its OAuth token under `~/.gemini/`
  (see §3). Sources: CodexBar docs/antigravity.md; computingforgeeks "Install
  Antigravity CLI on Linux, macOS, and Windows"; dev.to "Antigravity CLI: A
  Hands-On Guide" (2026-05-21).
- **[INFERRED]** gemini-cli and Antigravity coexist. gemini-cli still uses the
  production `cloudcode-pa.googleapis.com` with `ideType: IDE_UNSPECIFIED`;
  Antigravity uses the same base host but identifies as `ideType: ANTIGRAVITY` and
  a distinct OAuth client. Antigravity's own agent is branded "designed by the
  Google DeepMind team" (opencode `constants.ts` `ANTIGRAVITY_SYSTEM_INSTRUCTION`).

### Models / tiers exposed to users

- **[CONFIRMED]** Model IDs verified against the live API (opencode
  `docs/ANTIGRAVITY_API_SPEC.md`, "Verified by Direct API Testing", 2025-12-13):
  `gemini-3-pro-high`, `gemini-3-pro-low`, `claude-sonnet-4-6`,
  `claude-opus-4-6-thinking`, `gpt-oss-120b-medium`.
- **[INFERRED / July 2026]** Current user-facing lineup reported as five LLMs:
  Gemini 3.1 Pro (High and Low), Gemini 3 Flash, Claude Sonnet 4.6 / Opus 4.6, and
  GPT-OSS 120B. The user's "two models I can click on, one is a flash model" maps
  to **Gemini 3 Pro** and **Gemini 3 Flash**. Source: 9to5google (2026-05),
  androidauthority, antigravity.google/docs/plans.
- **[SUPERSEDED by §0 — do not implement this]** opencode's client-side classifier
  is unnecessary: the live `retrieveUserQuotaSummary` already returns
  server-grouped buckets (Gemini Models / Claude+GPT), each with weekly + 5h
  windows. The substring classification below is kept only as historical context.
- **[CONFIRMED, historical]** opencode's bucket classifier collapses model IDs into
  three families — `claude`, `gemini-pro`, `gemini-flash` — by substring:
  `claude` if name contains "claude"; otherwise must contain "gemini-3", then
  `gemini-flash` if it contains "flash" else `gemini-pro`. Source
  `scripts/check-quota.mjs` `classifyGroup()`. Note: this is analogous to YapCap's
  existing Gemini Pro/Flash/Lite family bucketing but there is **no "lite"** family
  in Antigravity's current classifier — it's Claude / Pro / Flash.

### Quota bucketing / reset windows

- **[INFERRED, July 2026]** Free tier moved to a **weekly** rate limit; Google AI
  Pro / Ultra subscribers get quotas that **refresh every ~5 hours**, with the
  higher-end Gemini model on a stricter quota than the faster/lighter one. Pro/Ultra
  can spend purchased AI credits for overage. Limits were increased several times
  amid backlash. Sources: blog.google "new-antigravity-rate-limits-pro-ultra";
  9to5google; androidauthority.
- **[CONFIRMED]** The API returns quota as a **`remainingFraction`** (0.0–1.0
  double) per model plus a **`resetTime`** (ISO-8601 string, sometimes epoch
  seconds), not absolute request counts. See §4 for exact field names.

---

## 2. Reference implementations (read from source)

Three open-source tools implement Antigravity usage/auth. All three refresh a
Google OAuth token and hit `cloudcode-pa.googleapis.com/v1internal:*` — the **same
family of endpoints YapCap's Gemini provider already uses**, just with an
`ideType: ANTIGRAVITY` metadata and a different OAuth client.

| Tool | Lang | Repo | Best files |
|------|------|------|-----------|
| **CodexBar** (steipete) | Swift | `github.com/steipete/CodexBar` | `Sources/CodexBarCore/Providers/Antigravity/AntigravityRemoteUsageFetcher.swift`, `AntigravityOAuthCredentialsStore.swift`, `AntigravityQuotaSummaryParser.swift`, `AntigravityStatusProbe.swift`; `docs/antigravity.md` |
| **opencode-antigravity-auth** (NoeFabris) | TypeScript | `github.com/NoeFabris/opencode-antigravity-auth` | `src/constants.ts`, `src/antigravity/oauth.ts`, `scripts/check-quota.mjs`, `docs/ANTIGRAVITY_API_SPEC.md` |
| **CrossUsage** (barramee27) | — | `github.com/barramee27/crossusage` (fork of OpenUsage by Robin Ebers, MIT) | not inspected in detail — see §2.3 |

### 2.1 CodexBar (most complete; the one to model on)

Remote OAuth fetcher — `AntigravityRemoteUsageFetcher.swift`:

- **[CONFIRMED]** Base host `https://cloudcode-pa.googleapis.com`. Endpoints:
  - `POST /v1internal:loadCodeAssist` — project discovery + tier/plan.
  - `POST /v1internal:onboardUser` — provisions a managed project when
    loadCodeAssist returns none.
  - `POST /v1internal:fetchAvailableModels` — **primary quota source**; returns
    per-model `quotaInfo`.
  - `POST /v1internal:retrieveUserQuota` — fallback/verification quota source
    (used when fetchAvailableModels is 403'd, or to verify when all fractions read
    ~1.0).
- **[CONFIRMED]** Auth header `Authorization: Bearer <access_token>`,
  `Content-Type: application/json`, `User-Agent: antigravity`.
- **[CONFIRMED]** `loadCodeAssist` body:
  ```json
  {"metadata":{"ideType":"ANTIGRAVITY","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}}
  ```
- **[CONFIRMED]** `fetchAvailableModels` / `retrieveUserQuota` body: `{"project":"<id>"}`
  (or `{}` when no project id known).
- **[CONFIRMED]** `onboardUser` body:
  `{"tierId":"<tier>","metadata":{...same ideType ANTIGRAVITY...}}`.
- **[CONFIRMED]** Token refresh: `POST https://oauth2.googleapis.com/token`,
  `application/x-www-form-urlencoded`, form
  `client_id, client_secret, refresh_token, grant_type=refresh_token`. Refresh
  triggered when `expiryDate - now <= 60s`.
- **[CONFIRMED]** Status handling: 401 → not-logged-in; 403 → permission denied
  (triggers fallback path); else error. Email/plan derived from decoding the
  `id_token` JWT payload (`email`, `hd` hosted-domain claims).

### 2.2 opencode-antigravity-auth (has hardcoded public client + endpoints)

- **[CONFIRMED]** Hardcoded **Antigravity OAuth client** (`src/constants.ts`):
  - client_id: `1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com`
  - client_secret: `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`
  - This is a **different** client from gemini-cli's
    (`681255809395-...` / `GOCSPX-4uHgMPm-...`, hardcoded in YapCap's
    `fixtures/gemini/probe.py`).
- **[CONFIRMED]** Scopes (`ANTIGRAVITY_SCOPES`):
  ```
  https://www.googleapis.com/auth/cloud-platform
  https://www.googleapis.com/auth/userinfo.email
  https://www.googleapis.com/auth/userinfo.profile
  https://www.googleapis.com/auth/cclog
  https://www.googleapis.com/auth/experimentsandconfigs
  ```
  (CodexBar uses only the first two — `cloud-platform` + `userinfo.email` — for its
  own login flow; `AntigravityOAuthConfig.scopes`.)
- **[CONFIRMED]** OAuth endpoints: auth `https://accounts.google.com/o/oauth2/v2/auth`
  (PKCE S256, `access_type=offline`, `prompt=consent`), token
  `https://oauth2.googleapis.com/token`, userinfo
  `https://www.googleapis.com/oauth2/v1/userinfo?alt=json` (also
  `/oauth2/v2/userinfo`). Loopback redirect `http://localhost:51121/oauth-callback`.
- **[CONFIRMED]** Three API hosts with fallback order (`constants.ts`):
  - daily sandbox `https://daily-cloudcode-pa.sandbox.googleapis.com`
  - autopush sandbox `https://autopush-cloudcode-pa.sandbox.googleapis.com`
  - **prod `https://cloudcode-pa.googleapis.com`** ← use this for quota; it's what
    `check-quota.mjs` and CodexBar use.
- **[CONFIRMED]** Rich header set (`getAntigravityHeaders()`), when mimicking the
  desktop app precisely:
  - `User-Agent: antigravity/<version> <platform>` (e.g.
    `antigravity/1.15.8 windows/amd64`; version fallback `1.18.3`), or a full
    Electron UA string. `check-quota.mjs` just uses `User-Agent: antigravity/windows/amd64`.
  - `X-Goog-Api-Client: google-cloud-sdk vscode_cloudshelleditor/0.1`
  - `Client-Metadata: {"ideType":"ANTIGRAVITY","platform":"WINDOWS|MACOS","pluginType":"GEMINI"}`
  - **[INFERRED]** These extra headers appear optional for quota reads — CodexBar's
    remote fetcher sends only `User-Agent: antigravity` and succeeds. Start minimal
    (like CodexBar); add `Client-Metadata`/`X-Goog-Api-Client` only if a bare
    request is rejected.
- **[CONFIRMED]** Hardcoded fallback project ids when the API returns none
  (workspace/business accounts): `rising-fact-p41fc` (constants.ts),
  `bamboo-precept-lgxtn` (check-quota.mjs). These are shared throwaway GCP projects;
  don't rely on a specific value — prefer the id from `loadCodeAssist`/`onboardUser`.

### 2.3 CrossUsage

- **[CONFIRMED]** `github.com/barramee27/crossusage`, MIT, fork of OpenUsage by
  Robin Ebers. Site (crossusage.dev) lists "Antigravity", "Antigravity CLI",
  "Antigravity IDE" among 27 plugins; Linux + Windows tray/panel; local-first;
  credentials encrypted AES-256-GCM with key in OS keychain.
- **[UNKNOWN]** Per-provider Antigravity source not inspected here. Its approach is
  expected to overlap CodexBar's (read local session / OAuth token, call the same
  cloudcode-pa endpoints). Inspect `crossusage` (or upstream OpenUsage) plugin
  sources if a second reference is needed. **Not required** — CodexBar +
  opencode already fully specify the probe.

---

## 3. Where Antigravity stores credentials locally (Linux)

Two distinct credential surfaces exist:

1. **Antigravity OAuth token — [CONFIRMED on a live Linux install, 2026-07-14]**
   `~/.gemini/oauth_creds_ag.json`, sitting directly beside gemini-cli's
   `~/.gemini/oauth_creds.json`. It has the **same JSON shape** as the gemini-cli
   creds file that `fixtures/gemini/probe.py` already reads:
   `access_token`, `refresh_token`, `scope`, `token_type` (`"Bearer"`),
   `id_token`, `expiry_date` (Unix **ms**). Observed on this machine:
   - `scope`: `openid https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cloud-platform`
   - `id_token` `aud` (= the Antigravity OAuth **client id**):
     `884354919052-36trc1jjb3tguiac32ov6cod268c5blh.apps.googleusercontent.com`
     — note this differs from **both** gemini-cli's `681255809395-…` **and** the
     opencode-reported `1071006060591-…` client below, so there are at least two
     Antigravity clients in the wild (IDE vs CLI, or version drift). The matching
     client **secret** is not in the creds file; it must come from the app binary
     or a known public constant. Treat client id/secret as env-overridable.

   A parallel CLIProxyAPI capture also exists at
   `~/.cli-proxy-api/antigravity-<email>.json` with
   `{access_token, refresh_token, expired, expires_in, project_id, email, type:"antigravity"}`
   — confirms Antigravity carries a GCP `project_id` (here `mimetic-team-7tjsh`),
   analogous to Gemini's `cloudaicompanionProject`.

   Broader CLI state (conversations, brain, installation_id) lives under
   `~/.gemini/antigravity/` and `~/.gemini/antigravity-cli/`. CLI config:
   `~/.config/antigravity/config.toml` (reported, not verified). The earlier-inferred
   `~/.gemini/antigravity-cli/antigravity-oauth-token` path was **not** present on
   this install — `oauth_creds_ag.json` is the actual token file.
2. **IDE / language-server** — the IDE runs a local Codeium-style language server;
   CodexBar talks to it over `127.0.0.1:<port>` rather than reading a token file.
   No stable on-disk OAuth JSON path for the IDE is documented; the OAuth client
   id/secret are embedded in the app binary/`oauthClient.js` (CodexBar extracts
   them from `Contents/Resources/app/.../language_server_*` and
   `out/main.js`). **[UNKNOWN]** exact Linux install path of the IDE bundle.

CodexBar itself does **not** read Antigravity's own credential file for the remote
path — after its own Google login it writes to
`~/.codexbar/antigravity/oauth_creds.json` (0600), format below.

### Credential JSON shape (CodexBar `AntigravityOAuthCredentials`)

Accepts snake_case or camelCase; writes snake_case. Fields:
`access_token`, `refresh_token`, `expiry_date` (Unix **ms**) / `expiresAt`,
`id_token`, `email`, `project_id`, `client_id`, `client_secret`. This matches
YapCap's Gemini `~/.gemini/oauth_creds.json` convention closely (`access_token`,
`refresh_token`, `expiry_date` ms, `id_token`).

**[RECOMMENDATION for YapCap probe]** Rather than reverse-engineer the CLI token
file format first, take the same pragmatic route as CodexBar/opencode: read a
`refresh_token` (from env or a small creds file), refresh against
`oauth2.googleapis.com/token` with the Antigravity client, then hit the quota
endpoints. Confirm the CLI token file's real JSON keys on a live install and add a
loader once known.

---

## 4. Response shapes (for parsing / fixtures)

### `fetchAvailableModels` (primary) — CodexBar `FetchAvailableModelsResponse`, opencode `check-quota.mjs`

```json
{
  "models": {
    "gemini-3-pro-high": {
      "displayName": "Gemini 3 Pro (High)",
      "label": "Gemini 3 Pro",
      "quotaInfo": {
        "remainingFraction": 0.62,
        "resetTime": "2026-07-14T18:00:00Z"
      }
    },
    "gemini-3-flash": { "quotaInfo": { "remainingFraction": 0.95, "resetTime": "..." } },
    "claude-sonnet-4-6": { "quotaInfo": { "remainingFraction": 1.0, "resetTime": "..." } }
  }
}
```
- `models` is an **object keyed by model id**. Each value: `displayName?`, `label?`,
  `quotaInfo{ remainingFraction?: double, resetTime?: string }`. Label fallback:
  `displayName || label || <modelId>`. **[CONFIRMED]** field names.

### `retrieveUserQuota` (fallback) — CodexBar `RetrieveUserQuotaResponse`

```json
{ "buckets": [ { "modelId": "gemini-3-pro-high", "remainingFraction": 0.62, "resetTime": "..." } ] }
```
- `buckets[]` with `modelId`, `remainingFraction`, `resetTime`. Multiple buckets
  per model → CodexBar keeps the **minimum** `remainingFraction`. **[CONFIRMED]**.
  (Note: this differs from YapCap's Gemini `retrieveUserQuota`, whose fixture you
  should compare — capture a real one.)

### `loadCodeAssist` — project + tier/plan

- `cloudaicompanionProject` (string, or object with `.id`) → project id.
- `currentTier{ id, name }`, `paidTier{ id }`, `allowedTiers[]{ id, isDefault }`,
  `planInfo{ planType }`. Plan mapping in CodexBar `resolvePlan`:
  `standard-tier`→Paid, `free-tier`+hosted-domain→Workspace, `free-tier`→Free,
  `legacy-tier`→Legacy. **[CONFIRMED]** field names; **[INFERRED]** exact tier-id
  strings (capture a real response to confirm).

### Local language-server quota summary (alternative, no cloud call)

If reading the IDE/CLI local server instead of the cloud API — CodexBar
`AntigravityQuotaSummaryParser` + `docs/antigravity.md`:
- `POST https://127.0.0.1:<port>/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary`
  body `{"forceRefresh": true}`; headers `Content-Type: application/json`,
  `Connect-Protocol-Version: 1`, `X-Codeium-Csrf-Token: <token>` (IDE requires it,
  CLI does not).
- Response: `response.groups[]{ displayName ("Gemini Models" / "Claude and GPT
  models"), description, buckets[]{ bucketId, displayName, remaining.remainingFraction
  (or remainingFraction), resetTime, disabled } }`.
- `GetUserStatus` (body `{"metadata":{"ideName":"antigravity","extensionName":"antigravity","ideVersion":"unknown","locale":"en"}}`)
  yields `accountEmail`, `planName`, and legacy
  `cascadeModelConfigData.clientModelConfigs[].quotaInfo.{remainingFraction,resetTime}`.
- Port discovery: find the `language_server*` process, read its
  `--extension_server_port` and `--csrf_token` flags; enumerate listening ports.

**Recommendation for YapCap:** prefer the **cloud OAuth path** (§2.1) — it matches
the existing Gemini provider architecture and needs no local process scraping.
Keep the local-server path noted as a fallback only.

---

## 5. Concrete probe plan (mirror `fixtures/gemini/probe.py`)

Build `fixtures/antigravity/probe.py` structurally identical to the Gemini probe,
changing:

1. **OAuth client** — Antigravity's public client:
   `1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com` /
   `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`. **[CONFIRMED]** but version-dated — the
   IDE may rotate; CodexBar extracts it dynamically from the installed app. Allow
   env override (`ANTIGRAVITY_OAUTH_CLIENT_ID/SECRET`, matching CodexBar).
2. **Creds source** — env `ANTIGRAVITY_REFRESH_TOKEN` (+ `YAPCAP_` variant) first;
   optionally read `~/.gemini/antigravity-cli/antigravity-oauth-token` once its real
   JSON shape is confirmed on Linux. Same expiry-ms + 5-min-skew refresh logic as
   the Gemini probe.
3. **Token endpoint** — unchanged: `https://oauth2.googleapis.com/token`,
   form-encoded refresh grant.
4. **Metadata** — `ideType: ANTIGRAVITY`, `platform: PLATFORM_UNSPECIFIED`,
   `pluginType: GEMINI` (vs Gemini probe's `IDE_UNSPECIFIED`).
5. **Endpoints to record** (host `https://cloudcode-pa.googleapis.com`):
   - `oauth_token_response.json` — token refresh.
   - `load_code_assist_response.json` — `POST /v1internal:loadCodeAssist`, body
     `{"metadata":{ideType:ANTIGRAVITY,...}}`; extract `cloudaicompanionProject`.
   - `fetch_available_models_response.json` — `POST /v1internal:fetchAvailableModels`,
     body `{"project":"<id>"}`. **Primary quota fixture.**
   - `retrieve_user_quota_response.json` — `POST /v1internal:retrieveUserQuota`,
     body `{"project":"<id>"}`. **Fallback quota fixture.**
   - (optional) `onboard_user_response.json` if loadCodeAssist yields no project.
6. **Headers** — `Authorization: Bearer`, `Content-Type: application/json`,
   `User-Agent: antigravity`. If a bare request 4xx's, add
   `X-Goog-Api-Client: google-cloud-sdk vscode_cloudshelleditor/0.1` and
   `Client-Metadata: {"ideType":"ANTIGRAVITY","platform":"MACOS","pluginType":"GEMINI"}`.
7. **Redaction** — same as Gemini: output contains access/refresh tokens, id_token
   (PII email/hd), project ids — do not commit unredacted.

### Bucket mapping for YapCap classifier (from opencode `classifyGroup`)

```
name contains "claude"        -> Claude family
else must contain "gemini-3":
  contains "flash"            -> Gemini Flash family
  else                        -> Gemini Pro family
(gpt-oss-* -> currently unclassified by opencode; decide: own "GPT-OSS" bucket)
```
Aggregate per family by **min `remainingFraction`** and **earliest `resetTime`**
(matches both CodexBar `retrieveUserQuota` merge and opencode `updateGroup`).

---

## 6. Open items to confirm on a live Linux install

- **[UNKNOWN]** Exact JSON keys/format of `~/.gemini/antigravity-cli/antigravity-oauth-token`
  (is it `{access_token, refresh_token, expiry_date, id_token}` like gemini-cli, or
  a raw token string?). Blocking for a file-based loader; not blocking for an
  env-var probe.
- **[UNKNOWN]** Whether the current IDE ships the same public client id above or a
  rotated one — verify by grepping the installed Linux bundle for
  `apps.googleusercontent.com` / `GOCSPX-` (CodexBar's extraction approach).
- **[RESOLVED 2026-07-15]** Prod `cloudcode-pa.googleapis.com` returns full quota
  for a free-tier account — no daily-sandbox host needed — **provided the
  `project` field is sent**. See the `project` box in §2; free tier returns two
  weekly buckets, no 5h.
- **[RESOLVED 2026-07-15]** Real tier-id strings live-verified against two
  accounts: `currentTier.id` ∈ {`free-tier`, `standard-tier`} (User-Agent
  dependent — see the tier box), `paidTier.id` ∈ {`free-tier` ("Antigravity
  Starter Quota"), `g1-pro-tier` ("Google AI Pro")}.
- Confirm `gpt-oss-120b-medium` handling and the exact July-2026 Gemini model ids
  (`gemini-3-pro-high/low` vs a `gemini-3.1-pro` rename).

---

## Sources

Primary source code:
- CodexBar (steipete), `github.com/steipete/CodexBar`, files under
  `Sources/CodexBarCore/Providers/Antigravity/` (`AntigravityRemoteUsageFetcher.swift`,
  `AntigravityOAuthCredentialsStore.swift`, `AntigravityQuotaSummaryParser.swift`,
  `AntigravityStatusProbe.swift`) and `docs/antigravity.md`, read from
  `raw.githubusercontent.com/steipete/CodexBar/main/...` on 2026-07-14.
- opencode-antigravity-auth (NoeFabris), `github.com/NoeFabris/opencode-antigravity-auth`,
  `src/constants.ts`, `src/antigravity/oauth.ts`, `scripts/check-quota.mjs`,
  `docs/ANTIGRAVITY_API_SPEC.md` (dated 2025-12-13/14).
- CrossUsage (barramee27), `github.com/barramee27/crossusage` (fork of OpenUsage),
  and `crossusage.dev`.

Secondary (state/tiers, dated):
- blog.google/feed/new-antigravity-rate-limits-pro-ultra-subsribers/
- 9to5google.com (2026-05-21) tripled Gemini usage limits for Antigravity
- androidauthority.com gemini-antigravity-limits-increased
- antigravity.google/docs/plans
- computingforgeeks.com "Install Antigravity CLI on Linux, macOS, and Windows"
  (CLI token path `~/.gemini/antigravity-cli/antigravity-oauth-token`)
- dev.to/arindam_1729 "Antigravity CLI: A Hands-On Guide" (2026-05-21)

Local reference:
- YapCap `fixtures/gemini/probe.py` (gemini-cli OAuth client, `cloudcode-pa`
  loadCodeAssist/retrieveUserQuota convention).
