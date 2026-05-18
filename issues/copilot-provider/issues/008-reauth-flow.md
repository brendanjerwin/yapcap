---
status: done
type: AFK
blocked_by:
  - 002-device-flow-login
  - 003-free-tier-fetch-and-render
---

# Re-auth flow + 401/403 + id-mismatch protection

## What to build

Add the full error-classification path for Copilot fetches, plus the per-account re-auth UI and flow. Modeled on Gemini's email-mismatch protection, but on the GitHub numeric `id` instead of email.

See [`docs/copilot-provider.md` §10 and §11](../../../docs/copilot-provider.md).

## Acceptance criteria

**Error classification (in fetch path):**
- [x] HTTP 401 from `copilot_internal/user` → `auth_state = ActionRequired`. Preserve stale snapshot. Token left on disk for in-place re-auth.
- [x] HTTP 403 from `copilot_internal/user` → same as 401 (App permission revoked at GitHub side).
- [x] HTTP 429 → transient. Rate-limit backoff: parsed from `Retry-After` header when present; otherwise `300s × 2^(consecutive-1)` capped at 3600s. Matches Claude/Gemini pattern. Per-account `rate_limit_until` tracked. Logged at `warn`, not `error`.
- [x] HTTP 5xx, timeouts, network errors → transient. Preserve stale snapshot. Network-down case surfaces "No internet connection. Showing cached data; information is not up to date." (consistent with existing providers).

**Re-auth UI:**
- [x] Account rows with `auth_state = ActionRequired` show a per-account re-auth icon (refresh glyph) in Settings alongside the delete icon. Same iconography and behavior as Gemini's re-auth icon.
- [x] Per-account status badge in the popup reflects the re-auth state (matches the existing `Re-auth needed` / `Login` badge convention).

**Re-auth flow:**
- [x] Clicking the re-auth icon runs the device flow again (reusing the code from #002).
- [x] After the new `ghu_…` token comes back, `GET /user` fetches `{id, login}`.
- [x] If returned `id` ≠ stored `github_user_id`: reject with an actionable error ("This is a different GitHub account. The existing account was not updated."). Existing account directory and tokens are left unchanged. Display the error inline in Settings; do not commit anything.
- [x] On `id` match: overwrite `tokens.json`, refresh `login` label from the response, immediately trigger a usage refresh for that account.
- [x] Successful re-auth clears `auth_state` and `error` on the account.

**Tests:**
- [x] Unit tests cover error classification for 401, 403, 429 (with and without `Retry-After`), 500, network/timeout.
- [x] Unit tests cover the re-auth match/mismatch logic.
- [x] `cargo fmt`, `just check`, `cargo test` all pass.

## Blocked by

- `002-device-flow-login`
- `003-free-tier-fetch-and-render`

## Notes

### 2026-05-18T11:33:06+02:00

Completed Copilot re-auth classification coverage, id-mismatch guard tests, immediate refresh after successful re-auth, and spec documentation. Full checks passed.
