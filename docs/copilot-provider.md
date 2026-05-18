# GitHub Copilot Provider — As-Built

**Status:** As-built v0.6.0
**Last updated:** 2026-05-18

YapCap's fifth provider, alongside Codex / Claude / Cursor / Gemini.
Read-only usage display, same shape as every other provider.

## Scope summary

- **In scope (v1):** Free tier, Pro+ tier, Business tier. Multi-account.
  Device-flow login. Re-auth on token revocation.
- **Out of scope:** API-key auth, GHES (GitHub Enterprise Server), Copilot
  completions/chat features, organization-list rendering.
- **Pro tier:** schema unknown — GitHub paused new Pro upgrades in May 2026
  while rolling out new billing. v1 includes Pro support via fallback
  (entitlement-based disambiguation); a real Pro fixture lands later.
- **Enterprise tier:** schema unknown — no fixture available. v1 falls back
  to a generic "Plan" badge for unknown paid SKUs; Enterprise lands when a
  fixture appears.

## Key deviations from the other four providers

Three structural differences from Codex / Claude / Cursor / Gemini, all
documented inline below where they apply:

1. **Identity by GitHub user `id`, not email.** GitHub Apps cannot read user
   email reliably (see [Identity & dedupe](#identity--dedupe)). Display label
   uses `login`; dedupe uses the numeric `id`.
2. **No Active badge.** Copilot CLI stores its token in the OS keychain, not
   a readable file — no cross-distro / Flatpak-safe path to detect host
   session. Account rows render without an Active marker.
3. **Two response schemas.** Free tier and paid tiers use entirely different
   top-level shapes; the parser branches on `access_type_sku`.

---

## v1 Implementation Scope

Each bullet is intended to be a tracer-bullet slice; ordered by dependency.

### 1. Provider scaffolding

- Add `ProviderId::Copilot` to the enum and registry.
- Create `src/providers/copilot/` module with submodules:
  `device_flow`, `parse`, `account`, `fetch`, `headers`.
- Add `Config.copilot_enabled`, `Config.copilot_managed_accounts`, and
  `Config.selected_copilot_account_ids`. Bump COSMIC config schema if
  required.
- Wire Copilot into provider registry, refresh dispatcher, popup tab list,
  and Settings category list.
- Add Copilot icons:
  - `resources/providers/copilot.svg` (currentColor, dark-panel default)
  - `resources/providers/copilot-reversed.svg` (white fill, light-panel)
  Already on disk.

### 2. Device-flow login

- Settings exposes `Add account` under the Copilot accounts card.
- `POST github.com/login/device/code` with `client_id =
  Iv1.b507a08c87ecfe98`, `scope = read:user`.
- Display the returned `user_code` and `verification_uri` to the user.
- Open `verification_uri` via system browser (Flatpak: `OpenURI` portal).
- Poll `POST github.com/login/oauth/access_token` at the response's
  `interval` until a `ghu_…` token comes back.
- A `Cancel` control aborts the polling task without committing anything.
- Show a shared "Sign in to GitHub as the account you want to add. Use a
  private browsing window to switch accounts." hint at the add-account
  point. Backport the equivalent hint to Claude's add-account UI.

### 3. Identity & dedupe

- After receiving the token, call `GET https://api.github.com/user` once to
  fetch `{ id: u64, login: String }`. This is the only secondary auth call.
- Canonical identity is the numeric `id` (immutable; survives GitHub
  username rename).
- Account directory name: `copilot-<github-user-id>` under
  `<state-root>/yapcap/copilot-accounts/`.
- Dedupe on add-account: match incoming `id` against existing managed
  accounts. Match → update tokens and refresh `login` label. No match →
  create new account directory.
- Account display label: `login`. No email field surfaced (`UsageSnapshot.identity.email`
  remains empty for Copilot).

**Why not `login`:** GitHub usernames are mutable. CodexBar's source
explicitly migrated *away* from login-based dedupe to id-based dedupe for
this reason.

### 4. Token storage

- `tokens.json` shape, minimal: `{ "access_token": "ghu_..." }`.
- No `expires_at`, no `refresh_token`, no scope. The token has no expiry
  and no rotation; revocation is the only failure mode.
- `metadata.json` stores: `github_user_id: u64`, `login: String`, plus
  standard account-storage timestamps. The `login` is overwritten on each
  re-auth, never used as identity.

### 5. Usage fetch

Single HTTP request per refresh cycle:

```
GET https://api.github.com/copilot_internal/user
Authorization: token <ghu_…>
Accept: application/json
Editor-Version: vscode/1.107.0
Editor-Plugin-Version: copilot-chat/0.35.0
User-Agent: GitHubCopilotChat/0.35.0
X-Github-Api-Version: 2026-03-10
```

- Header values are sourced from opencode-mystatus (Jan 2026), kept in a
  single named-constants block in `src/providers/copilot/headers.rs` for
  trivial bumping if GitHub tightens enforcement.
- No preflight token check (token has no expiry).
- Use the global refresh interval; no Copilot-specific override.

### 6. Response parsing (two schemas)

Parser is a flat single function in `src/providers/copilot/parse.rs` that
branches on `access_type_sku`. Each branch produces a `UsageSnapshot` with
a `Vec<UsageWindow>` and `provider_cost: None`.

**Free schema** (`access_type_sku: "free_limited_copilot"`):

- Top-level fields: `monthly_quotas`, `limited_user_quotas`,
  `limited_user_reset_date`, `login`. No `quota_snapshots` block at all.
- Render **two** `UsageWindow` entries:
  - `chat` from `monthly_quotas.chat` (total) and `limited_user_quotas.chat` (remaining)
  - `completions` from `monthly_quotas.completions` (total) and `limited_user_quotas.completions` (remaining)
- Both windows use `limited_user_reset_date` as `reset_at`.
- `UsageHeadline` points at `completions` (the premium-equivalent slot for
  Free; chat alternatives exist outside Copilot).

**Paid schema** (`quota_snapshots` present):

- Render **one** `UsageWindow`: `premium_interactions`. Skip `chat` and
  `completions` because they are always `unlimited: true` on paid tiers.
- Use `quota_reset_date` as `reset_at`.
- `UsageHeadline` points at the single window.
- Use the integer `remaining` and float `percent_remaining`. The fractional
  `quota_remaining` is informational only and not surfaced.

Parser must tolerate extra fields without failing:

- Top-level: `analytics_tracking_id`, `assigned_date`,
  `can_signup_for_limited`, `chat_enabled`, `organization_login_list`,
  `organization_list`, `quota_reset_date_utc`.
- Per-quota: `quota_id`, `timestamp_utc`, `overage_permitted`.

### 7. Plan badge mapping

| `access_type_sku` | Badge |
|---|---|
| `free_limited_copilot` | **Free** |
| `plus_monthly_subscriber_quota` | **Pro+** |
| `copilot_standalone_seat_quota` | **Business** |
| (unknown SKU with `quota_snapshots`) | **Pro** if `entitlement == 300`, **Pro+** if `entitlement == 1500`, else **Plan** |
| anything else | **Plan** |

### 8. Panel rendering — new single-bar variant

- Existing panel renders two horizontal bars per account. For paid Copilot
  (1 window), the layout must support a single bar, **vertically centered**
  within the same total height as the two-bar layout.
- Mixed selection within Copilot (one Free + one paid account, both
  selected) renders side-by-side with different bar counts per column. No
  homogenization across columns.
- Update `UsageSnapshot::applet_windows()` to return `Option<(UsageWindow,
  Option<UsageWindow>)>` or equivalent shape so the panel can detect "one
  bar vs two." Specific tuple shape decided at implementation time.

### 9. Overage rendering (paid only)

- When `quota_snapshots.premium_interactions.overage_count > 0`, render a
  single text line in the popup directly under the premium bar:
  `"+<N> over plan"`.
- No new data-model field; the parser attaches the text inside the
  `UsageWindow` (e.g. via `reset_description` or an analogous field).
- Validate visually using the `YAPCAP_DEMO` seed (see below) — there is no
  way to reproduce real overage without paying for it.

### 10. Error handling

| Condition | Treatment |
|---|---|
| HTTP 401 from `copilot_internal/user` | `auth_state = ActionRequired`. Preserve stale snapshot. Token left on disk for in-place re-auth. |
| HTTP 403 | Same as 401 (App permission revoked at GitHub side). |
| HTTP 429 | Transient. Rate-limit backoff matches Claude/Gemini pattern (`Retry-After` header → 300s × 2^(n-1) capped at 3600s). |
| HTTP 5xx, timeouts, network errors | Transient. Preserve stale snapshot. |
| Unrecognized response shape (no `quota_snapshots`, no `monthly_quotas`) | Return actionable error with response details, such as "Unrecognized Copilot response: access_type_sku=no_access". Preserve stale snapshot. |

### 11. Re-auth flow

- Account rows with `auth_state = ActionRequired` show a per-account
  re-auth icon (refresh) alongside the delete icon. Identical to Gemini.
- Clicking it runs the device flow again.
- After receiving the new `ghu_…` token, fetch `GET /user`.
- If returned `id` ≠ stored `github_user_id`: reject with "This is a
  different GitHub account. The existing account was not updated." Leave
  existing account unchanged. (Gemini's email-mismatch pattern, on `id`.)
- On match: overwrite `tokens.json`, refresh `login` label from response,
  trigger usage refresh.

### 12. Demo seeding (`YAPCAP_DEMO`)

Seed two managed Copilot accounts to validate multi-account rendering and
overage display:

- **`casey-free`** (Free tier)
  - `chat`: 350/500 remaining (~30% used)
  - `completions`: 60/300 remaining (~80% used) — headline
  - `limited_user_reset_date`: ~2 weeks out
  - Plan badge: **Free**
- **`morgan-pro`** (Pro+ tier, in overage)
  - `premium_interactions.entitlement`: 1500
  - `premium_interactions.remaining`: 0
  - `premium_interactions.percent_remaining`: 0
  - `premium_interactions.overage_count`: 42 (validates `+42 over plan`
    text rendering)
  - `quota_reset_date`: ~2 weeks out
  - Plan badge: **Pro+**

Both selected with `show_all_accounts: true` to exercise the multi-column
popup and mixed bar-count panel layout.

### 13. Packaging (Flatpak)

No new permissions. Reuses:

- `--share=network` for OAuth and the Copilot endpoint.
- `org.freedesktop.portal.OpenURI` for opening `github.com/login/device`
  during device flow.
- No `--filesystem=home:ro` needed (no host config to read).

---

## API reference

### Endpoints

| Endpoint | Purpose | Frequency |
|---|---|---|
| `POST github.com/login/device/code` | Start device flow | Per add-account / re-auth |
| `POST github.com/login/oauth/access_token` | Poll for `ghu_…` token | Per add-account / re-auth (polled) |
| `GET api.github.com/user` | Fetch `{id, login}` | Per add-account / re-auth |
| `GET api.github.com/copilot_internal/user` | Usage fetch | Per refresh cycle |

### Required request headers

Identical for `copilot_internal/user` and `/user`:

```
Authorization: token <ghu_…>
Accept: application/json
Editor-Version: vscode/1.107.0
Editor-Plugin-Version: copilot-chat/0.35.0
User-Agent: GitHubCopilotChat/0.35.0
X-Github-Api-Version: 2026-03-10
```

### Free tier response shape

```json
{
  "access_type_sku": "free_limited_copilot",
  "copilot_plan": "individual",
  "limited_user_quotas":     { "chat": 480, "completions": 4000 },
  "monthly_quotas":          { "chat": 500, "completions": 4000 },
  "limited_user_reset_date": "2026-06-03",
  "login": "TopiCsarno"
}
```

- `monthly_quotas.<field>` = entitlement
- `limited_user_quotas.<field>` = remaining
- Used percent = `1 - remaining/total`

Entitlement values vary across captures (`completions: 4000` here vs `300`
in CodexBar's test fixtures vs `2000` in Microsoft Learn docs) — GitHub
adjusts Free limits over time. Field names are stable; values aren't.

### Paid tier response shape

```json
{
  "access_type_sku": "copilot_standalone_seat_quota",
  "copilot_plan": "business",
  "quota_reset_date": "2026-01-01",
  "quota_snapshots": {
    "chat":        { "unlimited": true,  "percent_remaining": 100.0, ... },
    "completions": { "unlimited": true,  "percent_remaining": 100.0, ... },
    "premium_interactions": {
      "entitlement": 300,
      "remaining": 93,
      "percent_remaining": 31.166666666666664,
      "quota_remaining": 93.5,
      "overage_count": 0,
      "overage_permitted": true,
      "unlimited": false
    }
  },
  "login": "TopiCsarno"
}
```

`quota_remaining` is fractional and reflects per-model multipliers
(different models cost different fractions of one premium interaction).
`remaining` is the floored integer. v1 uses `remaining` and
`percent_remaining` only.

### Token format

- `ghu_…` — user-to-server token from the `Iv1.b507a08c87ecfe98` GitHub
  App. Long-lived, no expiry, no refresh token.
- Note on the App vs OAuth App distinction: `Iv1.…` is a GitHub App client
  ID. The `read:user` scope parameter is technically an OAuth App
  convention, but GitHub accepts it on the device flow for this App. The
  App's permissions are fixed at registration time and are *not*
  influenced by scopes we request.
- This is why `/user/emails` returns 403 ("Resource not accessible by
  integration") for our token: the App lacks the `emails=read` permission.
  Even on `/user`, `email` is typically null because users with private
  emails (the default) don't expose it through any App.

---

## Reference fixtures

| File | Tier | Source |
|---|---|---|
| `fixtures/copilot/copilot_user_response.json` | Free | Live capture |
| `fixtures/copilot/copilot_user_pro_plus_response.json` | Pro+ | [zed-industries/zed#44499](https://github.com/zed-industries/zed/discussions/44499) |
| `fixtures/copilot/copilot_user_business_response.json` | Business | [BerriAI/litellm#18242](https://github.com/BerriAI/litellm/issues/18242), captured 2025-12-19 |
| `fixtures/copilot/copilot_token_response.json` | — | `/v2/token` exchange. Not used at runtime; kept for future-proofing the OpenCode-style auth flow if it becomes relevant. |
| `fixtures/copilot/device_code_response.json` | — | Device flow start |
| `fixtures/copilot/oauth_token_response.json` | — | Device flow token exchange |
| `fixtures/copilot/github_user_response.json` | — | `/user` reference |
| `fixtures/copilot/github_user_emails_response.json` | — | `/user/emails` 403 reference |
| `fixtures/copilot/probe.py` | — | Probe script for regenerating captures |

No fixture exists for Pro (paused upgrades), Enterprise (no source), or
the `overage_count > 0` state on any tier (no captured real-world
overage). The demo seed (`YAPCAP_DEMO`) substitutes for overage
visualization.

---

## Forward compatibility — AI Credits migration (June 1, 2026)

GitHub is moving all Copilot plans to usage-based **AI Credits** billing on
June 1, 2026 (~2 weeks from this design's last update). Premium-request
quotas are expected to be replaced by a credit-balance shape — likely
closer to Codex's `credits.balance` than to current `quota_snapshots`.

**Strategy:**

- **No premature abstraction.** Keep the v1 parser flat in
  `src/providers/copilot/parse.rs`. When the AI Credits schema lands, add a
  new schema branch (or rewrite the file) — don't try to predict its
  shape.
- **Detection:** an unknown response shape (no `quota_snapshots`, no
  `monthly_quotas`) is treated as an actionable error (see §10), not as a
  silent recovery. Surfaces "update may be needed" to the user.
- **No cache migration.** `UsageSnapshot` is provider-agnostic
  (`Vec<UsageWindow>`); old cached snapshots remain valid for display
  until the next successful refresh under the new parser overwrites them.
- **Data-model insulation.** Don't add a `premium_remaining: u32` or
  similar field to `UsageSnapshot`. Keep Copilot's parser output in the
  existing window-based shape so the swap is parser-only.

---

## Deferred to v2 (or post-AI-Credits)

| Item | Reason for deferral |
|---|---|
| Pro SKU mapping | GitHub paused Pro upgrades; cannot capture fixture |
| Enterprise SKU mapping | No source has captured a fixture |
| Typed overage data-model field | v1's popup text is sufficient; structural model decision should wait for AI Credits |
| `organization_login_list` / `organization_list` rendering | CodexBar (peer client) explicitly ignores these; no captured populated fixture |
| GitHub username rename auto-pickup during refresh | One extra `/user` call per refresh cycle is too costly for a rare event; add-account / re-auth already refresh the label |
| GHES (Enterprise Server) custom hosts | Out of scope per Scope section; CodexBar supports it as `enterpriseHost` param if precedent helps later |
| ADRs (`docs/adr/`) | No ADR directory exists in this repo; design lives here |

---

## Peer client references (for implementation lookups)

- **CodexBar** (Swift, mature): [steipete/CodexBar](https://github.com/steipete/CodexBar).
  Notable files: `Sources/CodexBarCore/CopilotUsageModels.swift`,
  `Sources/CodexBarCore/Providers/Copilot/`, `Sources/CodexBar/Providers/Copilot/CopilotLoginFlow.swift`.
  Source of truth for: id-based dedupe, organization-field ignore decision,
  defensive parser shape, schema-branch detection.
- **opencode-mystatus** (TypeScript, Jan 2026): [vbgate/opencode-mystatus](https://github.com/vbgate/opencode-mystatus).
  Source of truth for: current working header values, overage rendering
  pattern, tier-entitlement table.
- **lobehub icons-static-svg** (assets): npm package
  `@lobehub/icons-static-svg`, file `icons/githubcopilot.svg`. Source for
  the panel icon SVG.
