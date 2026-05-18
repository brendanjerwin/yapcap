---
status: done
type: AFK
blocked_by:
  - 001-provider-scaffolding
  - 002-device-flow-login
  - 003-free-tier-fetch-and-render
  - 004-paid-tier-parsing
  - 005-single-bar-panel-layout
  - 006-overage-rendering
  - 007-demo-seed
  - 008-reauth-flow
---

# Documentation updates (spec + README + qa.md)

## What to build

Update the project's canonical docs to reflect Copilot as a first-class fifth provider. This is the final slice — runs after all behavior is implemented.

## Acceptance criteria

**`docs/spec.md`:**
- [ ] §1.2 Supported Sources table: add Copilot row (primary: managed account from `copilot-accounts/<id>/`; fallback: none — token is long-lived and re-auth is user-driven).
- [ ] §2.1 System Context Mermaid diagram: add Copilot module and `api.github.com/copilot_internal/user` endpoint.
- [ ] §2.2 Crate Layout: add `providers::copilot` row describing the module's scope (device flow login, id-based dedupe, single-call usage fetch, Free + paid schema branches).
- [ ] New §3.5 Copilot section mirroring the existing §3.x pattern: account model, managed login flow (device flow), identity by GitHub numeric `id`, no Active badge, single-call usage fetch with header constants, Free schema branch, paid schema branch with SKU table, overage handling, error classification, rate-limit backoff, re-auth flow.
- [ ] §4.1 OAuth Credential Files: note that Copilot OAuth material lives under YapCap-owned `copilot-accounts/<id>/tokens.json`, no host config read.
- [ ] §4.3 Configuration: add `copilot_enabled`, `copilot_managed_accounts`, `selected_copilot_account_ids` field descriptions.
- [ ] §6 Persistence: add `copilot-accounts/` directory path under both Native and Flatpak layouts.
- [ ] §7.1 Panel: note the new single-bar layout variant for paid Copilot accounts (vertically centered within the two-bar height) and mixed bar-count rendering.
- [ ] §7.2 Popup: add Copilot to all provider enumerations.
- [ ] §10 Testing: add Copilot fixtures and parser-branch coverage to the test list.

**`README.md`:**
- [ ] Replace "Four providers" / "four providers" with "Five providers" / "five providers" in feature list and Limitations section.
- [ ] Add Copilot to per-provider feature descriptions, noting: browser device flow login, identity by GitHub username, no Active badge.
- [ ] If provider screenshots are referenced inline, add a Copilot screenshot (or note it's pending if not yet captured).
- [ ] Update `resources/app.metainfo.xml` release notes for the version that ships Copilot (e.g. `0.6.0`).

**`docs/qa.md`:** add a new `## N. Copilot` section (after Gemini) covering manual verification of every feature:
- [ ] **Add account** — Settings → Copilot → Add account runs the device flow; visiting `github.com/login/device` with the user code completes successfully; cancelling mid-flow leaves state unchanged; adding the same GitHub account a second time refreshes the existing entry (no duplicate).
- [ ] **Login hint** — shared "Sign in to your browser as the account…" text visible at the add-account point.
- [ ] **Multi-account add** — adding a second account requires private browsing or a different GitHub session; the second add creates a separate `copilot-<id>/` directory; both accounts visible in Settings.
- [ ] **Free tier display** — chat + completions windows render in popup; completions is the headline; panel shows two bars; reset time matches `limited_user_reset_date`.
- [ ] **Paid tier display** — `premium_interactions` single window; panel shows one vertically-centered bar; plan badge matches the SKU (Pro+ for `plus_monthly_subscriber_quota`, Business for `copilot_standalone_seat_quota`); reset time matches `quota_reset_date`.
- [ ] **Mixed bar counts** — Free + paid accounts side-by-side in panel show 2 + 1 bars respectively; no homogenization.
- [ ] **Overage rendering** — `YAPCAP_DEMO=1` shows `morgan-pro` account with `+42 over plan` text under the premium bar.
- [ ] **`YAPCAP_DEMO`** — both `casey-free` and `morgan-pro` accounts present, both selected, `show_all_accounts: true`.
- [ ] **Re-auth flow** — revoke at `github.com/settings/applications`; account badges flip to "Re-auth needed"; re-auth icon appears in Settings; re-auth completes successfully when logging in as the same GitHub account; re-auth rejected when logging in as a different GitHub account (id mismatch shows "different account" error).
- [ ] **Transient errors** — disable network mid-refresh; stale snapshot preserved with "No internet connection" message; reconnect and **Refresh now** restores fresh data.
- [ ] **Account removal** — deletes only `copilot-<id>/`; no host GitHub config touched.
- [ ] **Native + Flatpak parity** — repeat critical paths (add, refresh, re-auth, remove) in both builds; device flow opens browser via `OpenURI` portal under Flatpak.

**`docs/copilot-provider.md`:**
- [ ] Update the status header from "Design complete, awaiting v1 implementation" to "As-built v0.6.0" (or whichever version actually ships).

**Verification:**
- [ ] `cargo fmt`, `just check`, `cargo test` all pass (docs-only changes shouldn't break anything, but run as a guard).
- [ ] `git diff docs/spec.md README.md docs/qa.md docs/copilot-provider.md` shows comprehensive, coherent updates.

## Blocked by

- `001-provider-scaffolding`
- `002-device-flow-login`
- `003-free-tier-fetch-and-render`
- `004-paid-tier-parsing`
- `005-single-bar-panel-layout`
- `006-overage-rendering`
- `007-demo-seed`
- `008-reauth-flow`

## Completion note

Completed the final Copilot documentation pass across the architecture spec,
README, QA plan, Copilot provider status document, and AppStream release
metadata. Checks passed: `cargo fmt`, `cargo test`, `cargo check`, `just check`.
