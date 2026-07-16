# YapCap QA Plan

Manual test plan for v0.6.0. Run against both Native (`just install`) and Flatpak (`just flatpak-install`) builds unless noted.

Paths used below:

**Native** (default XDG layout on typical Linux installs):

- Config: `~/.config/cosmic/io.github.TopiCsarno.YapCap/v600/`
- Former snapshot cache, no longer active runtime state: `~/.cache/yapcap/snapshots.json`
- Accounts + logs: `~/.local/state/yapcap/` (e.g. `…/logs/yapcap.log`)

**Flatpak** (app id `io.github.TopiCsarno.YapCap`; paths use passwd `pw_dir` as `~`):

- Config: same COSMIC config schema `v600` dir (manifest mounts `~/.config/cosmic`)
- Former snapshot cache, no longer active runtime state: `~/.var/app/io.github.TopiCsarno.YapCap/cache/yapcap/snapshots.json`
- Accounts + logs: `~/.var/app/io.github.TopiCsarno.YapCap/data/yapcap/`

Do not expect the Flatpak build to use `~/.local/state/yapcap/` for YapCap data—that is native-only.

---

## 1. Fresh install

- For an isolated Native check, run `just run-empty-discovery`. It clears and
  uses `/tmp/yapcap-empty-home`, `/tmp/yapcap-empty-config`, and
  `/tmp/yapcap-empty-state`; do not use `just clear-all-data` unless wiping
  your real YapCap accounts and settings is intentional.
- With no detection markers and no YapCap accounts, the panel shows only the
  YapCap app icon. The popup shows the centered "No providers set up yet" hero
  and its `Open Settings` action; it has no provider tabs and hides both
  `Refresh now` and `Add provider` (`+`).
- The hero's `Open Settings` action opens Settings → General. From there, every
  provider remains reachable through the settings categories.
- Add an account from Settings with no detection marker. Its provider becomes
  visible and normal panel rendering replaces the app-icon fallback.
- Existing `v503` COSMIC settings are not loaded after the `v600` schema boundary; users must re-add accounts.
- Existing account directories, old snapshot caches, and logs are not automatically deleted by the schema boundary and may remain orphaned.
- Settings → General → About shows correct version and dist label ("Native" or "Flatpak").
- Panel icon renders without clipping or overflow.

### 1.1 Provider discovery

- Start `just run-empty-discovery`, then create and remove the marker paths
  below while YapCap is running. The matching provider tab, detected hint, and
  effective enablement should change without a restart; unrelated filesystem
  changes must not affect the UI.

  | Provider | Marker path | Expected kind |
  | --- | --- | --- |
  | Codex | `~/.codex` | directory |
  | Claude | `~/.claude` or `~/.claude.json` | directory or file |
  | Cursor | `~/.config/Cursor` | directory |
  | Gemini | `~/.gemini/settings.json` | file |
  | Antigravity | `~/.config/Antigravity` | directory |
  | Copilot | `~/.config/github-copilot` or `~/.copilot` | directory |
  | Minimax | `~/.mmx` | directory |

- A bare `~/.gemini/` directory must not detect Gemini; neither may a directory
  named `settings.json`. File/dir kind mismatches for the other markers must
  also remain undetected.
- A detected provider with no YapCap account has a normal provider tab. Its
  detail view shows an accent `Detected` chip and an add-account action that
  opens that provider's Settings category.
- The same detected-and-unconfigured provider has an accent dot on its Settings
  tab and a `Detected on this machine` caption on its Settings page. Explicitly
  disabling it hides its provider tab but does not hide those Settings hints.
- On a non-empty popup, `Add provider` (`+`) opens a picker containing all
  providers. Detected providers without accounts appear first with add-account
  emphasis; every entry opens the matching Settings category, including already
  configured and undetected providers.
- Removing a detection marker hides an `Auto` provider with no account. Add an
  account first, then remove its marker: the provider must remain visible,
  because account presence wins over detection.
- Change a detected provider's settings toggle off and on. It must write an
  explicit disabled/enabled choice; detection does not override either choice.

---

## 2. Panel icon styles

In Settings → General, cycle through all four panel icon styles and verify the panel updates immediately each time:

- `Logo and bars` — provider logo + two usage bars visible.
- `Bars only` — no logo, just bars.
- `Logo and percent` — logo + one percentage number.
- `Percent only` — only percentage, no logo. Tooltip in Settings explains it shows the first usage window.

---

## 3. General settings

- Autorefresh interval buttons — set each value, restart, confirm the interval persisted.
- Reset time format `relative` — usage windows show "Resets in Xd Xh".
- Reset time format `absolute` — windows show "Resets tomorrow at …" or day + time.
- Usage amount format `used` — bars and labels show consumed quota.
- Usage amount format `left` — bars and labels flip to remaining quota.
- Select a non-Codex provider tab, restart, and confirm YapCap opens on the same provider.
- Settings survive an app restart (kill and re-open).

---

## 4. Theme

- Flatpak permissions include `--talk-name=com.system76.CosmicSettingsDaemon.Config.*` so libcosmic can subscribe to per-config COSMIC theme watchers.
- Native: switch COSMIC to dark theme — provider icons switch to dark-panel variant without restart.
- Native: switch COSMIC to light theme — provider icons switch to reversed/light variant without restart.
- Native: change COSMIC accent colour — accent fill on selected tabs and rows updates without restart.
- Flatpak: switch COSMIC to dark theme — provider icons switch to dark-panel variant without restart.
- Flatpak: switch COSMIC to light theme — provider icons switch to reversed/light variant without restart.
- Flatpak: change COSMIC accent colour — accent fill on selected tabs and rows updates without restart.

---

## 5. Update checker

- About section shows "Checking for updates…" briefly on startup.
- If up to date, shows "Up to date".
- Simulate update available: `YAPCAP_DEBUG_UPDATE_AVAILABLE=1 cargo run` — red dot on Settings gear, General tab, and About title. Hovering dots shows "Update available".
- "Check again" appears and works when update check fails.

---

## 6. Codex

### 6.1 Add account

- Settings → Codex → Add account opens browser OAuth flow.
- Cancel during login returns to normal add-account state with no partial account stored.
- Successful login stores account under native `~/.local/state/yapcap/codex-accounts/` or Flatpak `~/.var/app/io.github.TopiCsarno.YapCap/data/yapcap/codex-accounts/`.
- Stored directory contains `metadata.json` and `tokens.json`; `metadata.json` has `email` and `provider_account_id`; `tokens.json` has `access_token`, `refresh_token`, and `expires_at`.
- Duplicate login (same email) updates the existing account directory, not a second entry.
- New account is selected immediately in single-account mode.

### 6.2 Usage display

- Session window (5h) shows used/left percent and reset time.
- Weekly window (7d) shows used/left percent and reset time.
- If credits balance present, cost card is visible.
- Pace indicator marker visible on bars with both `reset_at` and `window_seconds`.

### 6.3 Token refresh

- Corrupt `tokens.json` → `access_token` only, remove `refresh_token`. Verify "Login required" state after one failed refresh.
- Set `expires_at` to one minute in the past with a valid `refresh_token`. On next refresh, YapCap should transparently renew the token and fetch usage without showing an error. Verify `tokens.json` `expires_at` is updated.
- Set `expires_at` far in the past and set `refresh_token` to a junk value. Verify `ActionRequired` state ("Login" badge) and re-auth prompt in Settings.

### 6.4 Remove account

- Remove account from Settings — account directory deleted, provider shows empty state.

### 6.5 Active account badge

- switch accounts through CLI, active badge should update

---

## 7. Claude

### 7.1 Add account

- Settings → Claude → Add account opens browser OAuth flow and prompts for authentication code paste.
- Pasting a wrong or malformed code shows an explicit plain-language error ("paste the authentication code"); existing accounts are untouched.
- Pasting a full callback URL or raw query string is rejected with the same authentication-code guidance.

- Successful add stores account under native `~/.local/state/yapcap/claude-accounts/` or Flatpak `~/.var/app/io.github.TopiCsarno.YapCap/data/yapcap/claude-accounts/`.
- Stored directory contains `metadata.json` and `tokens.json`; `tokens.json` has `access_token`, `refresh_token`, and `expires_at`.
- Duplicate email upserts the existing account rather than creating a second entry.
- New account is selected immediately in single-account mode.

### 7.2 Usage display

- 5h session window and 7d weekly window visible.
- Max plan accounts: Sonnet, Opus, and Cowork model-specific windows visible.
- Pro plan accounts: model-specific windows absent.
- Extra usage / credits cost card visible when present.
- `utilization=0` + `resets_at=null` on the 5h window shows "Reset" label, not an error.

### 7.3 Token refresh

- Set `expires_at` to one minute in the past with a valid `refresh_token`. Verify silent refresh on next cycle. Verify `tokens.json` `expires_at` is updated.
- Replace `refresh_token` with junk. Verify `ActionRequired` badge and re-auth icon in Settings.
- Per-account re-auth: click re-auth icon → complete OAuth with the same email → usage refreshes immediately.
- Per-account re-auth with a different email → rejected with error, existing account unchanged.

### 7.4 Rate limiting

- Observe `RateLimited` behaviour: provider shows rate-limited message; if `Retry-After` header present, "(retry in Xm)" appended.
- After the backoff window passes, the next refresh clears `rate_limit_until`.

### 7.5 Change active account

- Native: switch accounts through `claude auth login`; Active badge updates from `~/.claude.json` without restart.
- Flatpak: switch accounts through `claude auth login`; Active badge updates from host `~/.claude.json` without restart. The Flatpak manifest grants read-only home access so the app can watch the home directory for `.claude.json` replacement events.
- Flatpak fallback: if the badge does not update automatically after `claude auth login`, click manual refresh. Active badge must reread host `~/.claude.json` and update.

### 7.6 Remove account

- Remove from Settings — account directory deleted, provider shows empty state.

---

## 8. Cursor

### 8.1 Add account (SQLite scan flow)

- Settings → Cursor → Add account triggers a scan of `~/.config/Cursor/User/globalStorage/state.vscdb`.
- If Cursor is not installed or the state DB is absent, YapCap reports that no Cursor account was detected and no account is stored.
- If Cursor IDE is installed but logged out, YapCap asks the user to log into Cursor IDE and does not expose internal `cursorAuth` key names.
- Successful scan stores account under native `~/.local/state/yapcap/cursor-accounts/<opaque-id>/` or Flatpak `~/.var/app/io.github.TopiCsarno.YapCap/data/yapcap/cursor-accounts/<opaque-id>/`.
- Stored `tokens.json` contains `access_token`, `token_id`, `expires_at`, and `refresh_token`.
- Stored `metadata.json` contains `email` (non-empty), display name, and plan.
- Directory name is opaque (`cursor-<millis>-<pid>` format) and does not embed the email.
- Duplicate scan for the same email replaces the existing managed account directory rather than creating a second entry.
- New account is selected immediately in single-account mode.
- Config `cursor_managed_accounts` entry has `id`, `email`, and `managed_account_root`; no bearer tokens.

### 8.2 Usage display

- Total and API windows shown on the thin panel bars; Auto + Composer windows are skipped on the panel.
- Full popup shows all usage windows.
- Billing cycle end date drives reset time.
- Membership type shown in identity/plan badge.

### 8.3 Token refresh

- Set `expires_at` in `tokens.json` to one minute in the past with a valid `refresh_token`. On next usage cycle, YapCap calls the refresh endpoint, writes rotated tokens, and fetches usage without showing an error. Verify `expires_at` updated in `tokens.json`.
- Replace `refresh_token` with a junk value and set `expires_at` in the past. Verify the stale usage snapshot remains visible and the account shows `Re-auth needed`.
- Verify provider status tells the user to log into that Cursor account in Cursor IDE and rescan.
- Re-scan after logging into Cursor IDE. Verify YapCap updates `tokens.json`, clears `Re-auth needed`, and triggers a fresh usage fetch.
- HTTP 429 or network error during refresh → transient; stale snapshot stays visible with error status and no re-auth badge is shown.

### 8.4 Remove account

- Remove from Settings — YapCap-owned account directory deleted, Cursor's own `~/.config/Cursor` files are untouched, provider shows empty state.

---

## 9. Gemini

### 9.1 Detection and Login required

- With no Gemini accounts configured, `~/.gemini/settings.json` detects Gemini
  and makes its provider tab visible. The tab shows the detected call to action
  pointing to Settings → Gemini → Add account.
- Without that marker, Gemini remains available through Settings and the
  `Add provider` picker; explicitly enabling it makes its tab visible and shows
  the normal **Login required** state.
- Pre-existing host `~/.gemini/oauth_creds.json` is **not** imported. YapCap does not read host tokens.

### 9.2 Add account (Native and Flatpak)

- Settings → Gemini → Add account opens the system browser (Native: directly; Flatpak: via `org.freedesktop.portal.OpenURI`) at Google's sign-in page.
- The browser redirects back to a loopback `127.0.0.1:<port>/?code=…&state=…` callback served by YapCap; the success page reads "Signed in to Gemini — you can close this tab and return to YapCap."
- Cancel during login aborts cleanly with no partial account stored.
- Successful login stores the account under native `~/.local/state/yapcap/gemini-accounts/<id>/` or Flatpak `~/.var/app/io.github.TopiCsarno.YapCap/data/yapcap/gemini-accounts/<id>/`.
- Stored directory contains `metadata.json` (email, sub, optional `hd`, last tier id, last `cloudaicompanionProject`) and `tokens.json` (`access_token`, `refresh_token`, `expires_at`, `scope`).
- New account is selected immediately in single-account mode.

### 9.3 Multi-account dedupe

- Add a second Gemini account with a different Google identity — both accounts appear in Settings and the popup.
- Re-running Add account with an already-stored Google account updates the existing managed directory by normalized email; no second entry is created.

### 9.4 Usage display

- Free-tier account: popup shows two bars (**Flash**, **Lite**); the Pro bar is hidden.
- Standard-tier (AI Pro) account: popup shows three bars (**Pro**, **Flash**, **Lite**); panel bars show Pro + Flash.
- Workspace account (id_token `hd` present, `currentTier.id = standard-tier`): plan badge reads **Workspace**.
- Each bucket reset follows the YapCap-wide `reset_time_format` preference.

### 9.5 Tier transitions

- Upgrade a free-tier account to AI Pro (or downgrade). On the next refresh cycle the Pro bar appears or disappears and the plan badge updates from **Free** to **Pro**/**Workspace** (or back), without restarting YapCap.

### 9.6 Active account hint

- With YapCap running, `gemini auth login` to a Gemini account YapCap is tracking — the **Active** badge follows the new active email written to `~/.gemini/google_accounts.json`.
- Switching to a Google account that YapCap does not track removes the Active badge from all tracked accounts.
- Deleting `~/.gemini/google_accounts.json` clears the Active badge; recreating it (e.g. via another `gemini auth login`) restores it without a YapCap restart.
- Flatpak: same behaviour through the read-only home mount; click **Refresh now** as a fallback if file watching misses an atomic replace.

### 9.7 Token refresh and re-auth

- Set `expires_at` to one minute in the past with a valid `refresh_token`. Verify silent refresh on the next cycle and updated `expires_at` in `tokens.json`.
- Replace `refresh_token` with junk. Verify `ActionRequired` badge ("Login") on the account, plus a per-account re-auth icon in Settings.
- Per-account re-auth: click re-auth icon → complete OAuth in the browser with the same Google account → usage refreshes immediately.
- Per-account re-auth with a different Google account (different `id_token.email`) → rejected with error, existing account left unchanged.

### 9.8 Remove account

- Remove from Settings — only the YapCap-owned account directory is deleted. Host `~/.gemini/` files (`oauth_creds.json`, `google_accounts.json`, `settings.json`) are not touched.
- If it was the last Gemini account, the provider returns to the Login required empty state.

### 9.9 Host CLI configurations that don't interfere

- Pre-existing `~/.gemini/settings.json` with `selectedAuthType: gemini-api-key` or `vertex-ai`: YapCap still runs OAuth login and stores its own tokens; the absence of an Active badge for these accounts is **expected**, not a bug.
- A `GEMINI_API_KEY` environment variable on the host shell has no effect on YapCap.

### 9.10 `cloudresourcemanager` fallback

- For accounts where `loadCodeAssist` returns no `cloudaicompanionProject` (common when the user has a paid GCP project but no auto-assigned Code Assist project), YapCap calls `cloudresourcemanager.googleapis.com/v1/projects` and picks the first `ACTIVE` project whose id begins with `gen-lang-client-`. Verify the discovered project id is persisted to `metadata.json` (`gemini_last_cloudaicompanion_project`) and the next refresh re-uses it directly.
- For accounts where neither path yields a project, the provider surfaces the actionable `NoCloudaicompanionProject` error in the popup.

---

## 10. Copilot

### 10.1 Add account

- Settings -> Copilot -> Add account starts GitHub device flow.
- Browser opens `https://github.com/login/device`; entering the displayed user code completes successfully.
- Cancel during polling leaves account storage and selected accounts unchanged.
- Adding the same GitHub account a second time refreshes the existing entry; no duplicate row appears.

### 10.2 Login hint

- The shared "Sign in to your browser as the account you want to add" private-window hint is visible at the add-account point.

### 10.3 Multi-account add

- Add a second GitHub account using private browsing or a different browser session.
- The second account creates a separate `copilot-<github-user-id>/` directory.
- Both accounts are visible in Settings.

### 10.4 Free tier display

- Free account popup renders Chat and Completions windows.
- Completions is the headline percentage.
- Panel shows two bars.
- Bar fills reflect the entitlements in the API response; do not assert fixed
  Free entitlement numbers (GitHub adjusts them and the response is authoritative).
- Reset time follows `quota_reset_date_utc` (falling back to `quota_reset_date`).
- No cost card is shown for the Free account.

### 10.5 Paid tier display

- Paid account popup renders one **Credits** window (token-based accounts).
- A dollar cost card shows used and included credits, e.g. `$28.00 / $70.00`.
- Panel shows one bar vertically centered within the two-bar height.
- Plan badge reads **Pro** for the Pro credit entitlement.
- Plan badge reads **Pro+** for `plus_monthly_subscriber_quota` / the Pro+ entitlement.
- Plan badge reads **Max** for the Max credit entitlement.
- Plan badge reads **Business** for `copilot_standalone_seat_quota`.
- An unknown SKU with no recognizable entitlement range falls back to **Plan**.
- Reset time follows `quota_reset_date_utc` (falling back to `quota_reset_date`).

### 10.6 Mixed bar counts

- Select a Free account and a paid account side by side.
- Panel shows a two-bar Free group beside a one-bar paid group.
- The one-bar paid group remains vertically centered; the Free group keeps two bars.

### 10.7 Overage rendering

- Run with `YAPCAP_DEMO=1`.
- Verify the `morgan-pro` Copilot account shows `+42 over plan` under the Credits bar.

### 10.8 `YAPCAP_DEMO`

- Run with `YAPCAP_DEMO=1`.
- Verify all seven provider tabs appear with demo accounts, including one
  Antigravity account with grouped Gemini Models / Claude and GPT models cards.
- Verify `casey-free` and `morgan-pro` Copilot accounts are both present.
- Verify `casey-free` shows Chat and Completions windows in the new Free shape
  and no cost card.
- Verify `morgan-pro` shows a Credits window, a dollar cost card, a **Pro+** badge,
  and `+42 over plan`.
- Verify both accounts are selected and Copilot `Show all accounts` is on.

### 10.9 Re-auth flow

- Revoke the YapCap GitHub App token at `github.com/settings/applications`.
- Trigger refresh and verify account badges flip to `Re-auth needed`.
- Verify the re-auth icon appears in Settings.
- Re-auth with the same GitHub account and verify the account refreshes successfully.
- Re-auth with a different GitHub account and verify YapCap rejects it with a different-account error without replacing the stored account.

### 10.10 Transient errors

- Disable network during refresh.
- Verify stale snapshot remains visible with the "No internet connection" message.
- Reconnect and click **Refresh now**; fresh data should restore.

### 10.11 Account removal

- Remove a Copilot account from Settings.
- Verify only the matching `copilot-<github-user-id>/` directory is deleted.
- Verify no host GitHub config is touched.

### 10.12 Native + Flatpak parity

- Repeat add, refresh, re-auth, and remove in Native and Flatpak builds.
- Under Flatpak, verify device flow opens the browser via the OpenURI portal.

---

## 11. Multi-account

- Add a second account for any provider.
- `Show all accounts` toggle appears only when the provider has more than one account.
- `Show all accounts` off — single active account column in popup.
- `Show all accounts` on — one column per selected account side by side. Popup width expands by 420 px per additional column.
- Panel bars expand horizontally: one two-bar group per selected account.
- Unloaded accounts show 0% fill in panel until their snapshot arrives.
- Switching the active account in single-account mode triggers a refresh for only that provider, not a global refresh.

---

## 12. Stale / error states

- Kill network (`nmcli networking off`). Trigger a refresh. Verify "No internet connection. Showing cached data; information is not up to date." message. Cached usage data still visible. Re-enable network, verify Live badge returns.
- Wait 11 minutes without refreshing (or set refresh interval to max and advance clock). Verify account badge switches from Live to Stale. Status line appends "(stale)".
- Cold start with shared runtime state older than 10 minutes. Verify Stale badge on startup, not "Live · Updated 21 hours ago".
- Corrupt or delete the old `~/.cache/yapcap/snapshots.json` file. Verify current builds ignore it and continue from shared runtime/config without crashing.

---

## 13. Provider enable/disable

- Disable a provider via its settings toggle — provider tab disappears from popup nav.
- All provider-specific settings below the toggle are dimmed and non-interactive when disabled.
- Re-enable — tab reappears and a refresh is triggered.
- On a fresh config, all provider enablement values start as `Auto`: only
  detected providers and providers with YapCap accounts are visible.
- Upgrade an existing config containing legacy `<provider>_enabled` booleans.
  Restart twice and verify each legacy value is migrated once to the equivalent
  explicit `<provider>_enablement` value, preserves its visible/hidden state,
  and does not change again on the second start.

---

## 14. Popup sizing

- Single-account provider: popup is 420 px wide.
- Two-account provider: popup is 840 px wide.
- Switching from a two-account tab to a one-account tab shrinks popup immediately.
- Switching from provider view to Settings shrinks to settings width.
- Content taller than 1080 px: body scrolls, header/nav/footer stay fixed.
- Header, nav, and footer stay centred at 420 px even in wide multi-account popup.

---

## 15. Accounts removed from filesystem

- Manually delete a provider account directory from the YapCap data tree (`~/.local/state/yapcap/<provider>-accounts/` native, or `~/.var/app/io.github.TopiCsarno.YapCap/data/yapcap/<provider>-accounts/` Flatpak). Trigger a refresh. Verify the provider surfaces "Login required" or empty state rather than showing a stale snapshot indefinitely.

---

## 16. Config state file manipulation

- Delete old cached snapshots (native `~/.cache/yapcap/snapshots.json`, Flatpak `~/.var/app/io.github.TopiCsarno.YapCap/cache/yapcap/snapshots.json`). Restart. Verify runtime comes from shared COSMIC runtime state, not the old file.
- Delete the COSMIC config dir (`just clear-config`). Restart. Verify defaults
  apply: all provider enablement values are `Auto`, refresh interval is 300s,
  reset time is relative, and usage amount is used.
- Leave an older `~/.config/cosmic/io.github.TopiCsarno.YapCap/v503/` config in
  place. Restart the current build and verify `v600` defaults are used instead.
- Manually edit config to add a non-existent account id to `selected_codex_account_ids`. Restart. Verify graceful fallback to first valid account or Login Required — no crash.
- Set `refresh_interval_seconds = 5` in config. Verify it is clamped to 10s at runtime (not 5s).

---

## 17. Multi-process runtime sync

Use a COSMIC panel configured on two displays so two YapCap applet processes run
at the same time. For native builds, watch
`~/.local/state/yapcap/logs/yapcap.log`; for Flatpak builds, watch
`~/.var/app/io.github.TopiCsarno.YapCap/data/yapcap/logs/yapcap.log`.

- Startup: launch YapCap on both displays. Verify one process logs `refresh ownership acquired at startup` and another logs `refresh ownership held by another process; waiting for takeover`. Startup entries should include `process_id`, `pid`, `panel_output`, `lock_path`, `flatpak`, config version, shared runtime version/generation, and shared control version/generation.
- Provider selection sync: select a different provider tab on one display. Verify the other display switches to the same selected provider without sharing popup route/open state. If that provider is enabled and stale or missing data, verify a `provider_selected` shared refresh request is written and the owner observes it.
- Refresh now from owner display: click **Refresh now** on the display whose process is owner. Verify `manual refresh requested` includes its process id and control generation, and `owner evaluated shared refresh requests` includes the same generation and requester. Verify provider/account refresh start logs, `provider refresh finished`, `shared runtime written`, and `shared refresh request consumed`.
- Refresh now from non-owner display: click **Refresh now** on the other display. Verify its process id appears as the requester in the owner's compact evaluation, the non-owner does not write shared runtime, and both displays observe the final runtime generation.
- Automatic refresh: set a short refresh interval and wait for a stale or missing enabled provider. Verify only the owner logs provider refresh start/finish and `shared runtime written`; the non-owner does not run timer refresh work.
- After a successful refresh, verify shared runtime generations settle. A short burst for refreshing/final state is expected; continuous `shared runtime written` lines without provider refresh, config, account, or host-session changes are a bug.
- Owner takeover: identify the owner process from logs and terminate it, or remove the output that owns it. Verify a waiting process logs `refresh ownership acquired after waiting`, clears shared refresh requests, and resumes owner refresh behavior.
- Login, re-auth, account deletion: from the non-owner display, add or re-authenticate an account, then delete it. Verify account rows/settings update across displays through config/account storage immediately, while shared runtime refresh or cleanup is written only by the owner.
- UI attribution: alternate popup, navigation, provider-tab, and settings actions between displays. Verify every user-action event includes the initiating `process_id` and no inference from adjacent watcher events is required.
- Disabled provider: disable a provider, then click **Refresh now** from either display. Verify the disabled provider is absent from the compact evaluation outcomes and no provider refresh starts for it.
- Missing shared runtime: clear the shared runtime COSMIC config entry or make it invalid, then restart both displays. Verify logs include `shared runtime missing; using empty runtime fallback` or `shared runtime invalid; using empty runtime fallback`, credentials/account config remain intact, and the owner repopulates shared runtime on the next successful refresh.
- Old snapshot cache: create or corrupt native/Flatpak `snapshots.json` files and restart. Verify no active runtime data is loaded from those files and existing files remain on disk.

Expected diagnostic log patterns for this section:

- Ownership: `refresh ownership acquired at startup`, `refresh ownership held by another process; waiting for takeover`, `refresh ownership acquired after waiting`, `failed to acquire refresh ownership lock`.
- Shared control: `manual refresh requested`, `shared control observed`, `owner evaluated shared refresh requests`, `shared refresh request consumed`.
- Shared runtime: `shared runtime loaded`, `shared runtime missing; using empty runtime fallback`, `shared runtime invalid; using empty runtime fallback`, `shared runtime written`, `shared runtime observed`.
- Refresh lifecycle: `provider account refresh started`, `provider refresh finished`, provider refresh error logs.

---

## 18. Logging

- Native: verify `~/.local/state/yapcap/logs/yapcap.log`. Flatpak: verify `~/.var/app/io.github.TopiCsarno.YapCap/data/yapcap/logs/yapcap.log`. Each is written during a normal session for that build.
- Verify no bearer tokens, access tokens, cookie values, or refresh tokens appear in the log.
- `RUST_LOG=debug just run` — debug output in terminal, still no credentials in log file.

---

## 19. Flatpak-specific

- Install via `just flatpak-install`. YapCap appears in COSMIC applet list.
- Install from the COSMIC Store. YapCap appears in the COSMIC panel applet picker after installation, uses the `io.github.TopiCsarno.YapCap` Flatpak id, appears under the applet category/filter, and shows "Place on desktop" rather than "Open".
- COSMIC Store details page shows developer `Tamás Csarnó`, version `0.6.0`, description paragraphs without manual line-break wrapping, and screenshots in this order: detail popup, Codex zoom, Claude Code zoom, Cursor zoom, Gemini zoom, Copilot zoom.
- About section shows "Flatpak" dist label.
- OAuth flows (Codex, Claude, Gemini, Copilot) open the system browser correctly from inside the sandbox.
- COSMIC dark/light theme and accent colour updates are observed immediately through the settings config watcher.
- Cursor add-account: Flatpak sandbox can read `~/.config/Cursor/User/globalStorage/state.vscdb` through the read-only home permission. Scan succeeds and account is stored.
- Flatpak permissions include `--filesystem=home:ro`, not writable home or `--filesystem=host`.
- Account state for the Flatpak build lives under `~/.var/app/io.github.TopiCsarno.YapCap/data/yapcap/` (not `~/.local/state/yapcap/`).
- `just flatpak-run` launches the installed Flatpak version.
- Native install (`just install`) About section shows "Native".
