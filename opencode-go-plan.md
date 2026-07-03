# Research: Adding OpenCode Go Provider to YapCap

## Status

- **Forked & cloned**: `brendanjerwin/yapcap` → `~/src/yapcap`, on `main` at `f9bb4fe`
- **Remotes**: `origin` = brendanjerwin/yapcap, `upstream` = TopiCsarno/yapcap

## OpenCode Go — What It Is

OpenCode Go is a **$5/mo (first month) → $10/mo** subscription from OpenCode (Anomaly) providing access to open coding models (GLM-5.2, Kimi K2.7, DeepSeek V4, MiniMax M3, Qwen3.7, MiMo-V2.5, etc.).

### Usage Limits

| Window | Dollar Limit |
|--------|-------------|
| 5 hour (rolling) | $12 |
| Weekly | $30 |
| Monthly | $60 |

### API Endpoints (confirmed from OpenCode source)

- `https://opencode.ai/zen/go/v1/chat/completions` — OpenAI-compatible
- `https://opencode.ai/zen/go/v1/messages` — Anthropic-compatible
- `https://opencode.ai/zen/go/v1/models` — model listing
- **No `/v1/usage`, `/v1/quota`, or `/v1/billing` endpoint exists** — confirmed by exhaustive source review

### Source Code Investigation (anomalyco/opencode)

Exhaustively searched `github.com/anomalyco/opencode`. Key findings:

| File | Finding |
|------|---------|
| `packages/core/src/database/schema.gen.ts` | Local SQLite `opencode.db` has `session` table with `cost`, `tokens_*` columns — tracks per-session spend, NOT subscription quota |
| `packages/opencode/src/cli/cmd/stats.ts` | `opencode stats` reads local SQLite for cost/token stats — NOT subscription limits |
| `packages/console/app/src/routes/workspace/[id]/go/lite-section.tsx` | Server-side rendering of Go dashboard — queries `LiteTable.rollingUsage`, `weeklyUsage`, `monthlyUsage` |
| `packages/console/app/src/routes/zen/util/handler.ts` | Zen API handler checks limits server-side, throws `GoUsageLimitError` — enforcement only, not queryable |
| `packages/console/core/src/subscription.ts` | `Subscription.analyzeRollingUsage/WeeklyUsage/MonthlyUsage` — server-side analysis from usage data |
| `packages/console/app/src/routes/zen/util/error.ts` | `GoUsageLimitError` class — returned when limits exceeded, includes `workspace`, `limitName`, `retryAfter` |
| `packages/opencode/src/session/retry.ts` | Client-side handling of `GoUsageLimitError` — parses workspace, limitName from error responses |
| `packages/opencode/test/tool/fixtures/models-api.json` | Go provider config: `api: "https://opencode.ai/zen/go/v1"`, `env: ["OPENCODE_API_KEY"]` |

**Conclusion: Dashboard scraping is the only way to get OpenCode Go usage data.** The limits are stored as an SST secret (`ZEN_LIMITS`), computed server-side, and rendered only on the dashboard HTML page. There is no secret API, no local cache of subscription quota, and no alternative data source.

## Data Source: Dashboard Scraping

### How it works

1. GET `https://opencode.ai/workspace/<workspaceId>/go` with `Cookie: auth=<authCookie>`
2. Parse the HTML for usage data — two formats exist:
   - **SolidJS SSR hydration**: `$R[N]={...usagePercent:X...resetInSec:Y...}` for `rollingUsage`, `weeklyUsage`, `monthlyUsage`
   - **HTML data-slot attributes**: `data-slot="usage-item"` blocks with `data-slot="usage-value"` and `data-slot="reset-time"`
3. Try SSR format first, fall back to data-slot if SSR finds nothing
4. Map to 3 `UsageWindow` entries: rolling (5h), weekly, monthly

### Credentials needed

| Credential | How to get | Used for |
|-----------|-----------|----------|
| Workspace ID | From dashboard URL: `https://opencode.ai/workspace/<wrk_...>/go` | Dashboard URL path |
| Auth cookie | Browser devtools → cookies for `opencode.ai` → `auth` value | Dashboard authentication |

### Limitations

- Auth cookie expires (browser session cookie) — user must periodically refresh it
- HTML format can change if OpenCode updates their dashboard
- This is the same approach `opencode-quota` and `pi-usage` use

### Reference implementations

- `slkiser/opencode-quota` — `src/lib/opencode-go.ts` (TypeScript, two-format parser)
- `timm-u/pi-usage` — `index.ts` (TypeScript, dashboard scrape + model probe fallback)

## YapCap Provider Interface — Capability Table

### ProviderAdapter trait methods

| Capability | Method for opencode_go | Details |
|-----------|----------------------|---------|
| `id()` | `ProviderId::OpencodeGo` | Return the enum variant |
| `capabilities().supports_delete` | `true` | User can delete the account |
| `capabilities().supports_reauthentication` | `false` | No OAuth; user re-pastes credentials |
| `capabilities().supports_background_status_refresh` | `false` | No host CLI to poll |
| `capabilities().requires_auth_prompt_on_auth_failure` | `false` | Cookie expiry shown as error state |
| `discover_accounts()` | Read `config.opencode_go_managed_accounts` | List managed accounts from Config |
| `delete_account()` | Remove from config + delete account dir | Same as Minimax pattern |
| `reconcile_provider_accounts()` | Discover + reconcile descriptors | No `system_active_account_id` (no host CLI) |
| `fetch_account()` | Dashboard scrape | GET `opencode.ai/workspace/<id>/go` with cookie, parse HTML for 3 usage windows |
| `refresh_account_statuses()` | No-op (returns empty vec) | No background status refresh |

### UsageSnapshot mapping

| Field | Value |
|-------|-------|
| `provider` | `ProviderId::OpencodeGo` |
| `source` | `"Dashboard"` |
| `updated_at` | `Utc::now()` at fetch time |
| `headline` | `UsageHeadline(0)` — first window (rolling/5h) |
| `windows` | 3 windows: rolling(5h), weekly, monthly — each with `used_percent` + `reset_at` from scraped data |
| `provider_cost` | `None` — limits are dollar-value, not token-count (unlike Codex credits) |
| `extra_usage` | `None` |
| `identity` | `email=None, account_id=Some(workspace_id), plan=Some("OpenCode Go"), display_name=None` |

### ManagedOpencodeGoAccountConfig struct

```rust
pub struct ManagedOpencodeGoAccountConfig {
    pub id: String,
    pub label: String,
    pub workspace_id: String,       // from dashboard URL
    pub auth_cookie_source: String, // "stored" (always — no env var)
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_authenticated_at: Option<DateTime<Utc>>,
}
```

### Storage files (per account dir)

```
~/.local/state/yapcap/opencode-go-accounts/<id>/
  workspace_id.txt  — workspace ID for dashboard URL
  auth_cookie.txt   — browser auth cookie for dashboard scraping
```

### Fetch implementation (Rust)

```rust
const DASHBOARD_URL: &str = "https://opencode.ai/workspace";

// 1. Read credentials from account dir
let workspace_id = load_workspace_id(account_root)?;
let auth_cookie = load_auth_cookie(account_root)?;

// 2. GET dashboard HTML
let url = format!("{}/{}/go", DASHBOARD_URL, workspace_id);
let response = client
    .get(&url)
    .header("Cookie", format!("auth={}", auth_cookie))
    .header("User-Agent", "Mozilla/5.0 ...")
    .send().await?;

// 3. Parse HTML — try SolidJS SSR first, then data-slot
let html = response.text().await?;

// SSR format: rollingUsage:$R[N]={...usagePercent:X...resetInSec:Y...}
let rolling = parse_ssr_window(&html, "rollingUsage")?;
let weekly = parse_ssr_window(&html, "weeklyUsage")?;
let monthly = parse_ssr_window(&html, "monthlyUsage")?;

// Fall back to data-slot HTML if SSR found nothing
if rolling.is_none() && weekly.is_none() && monthly.is_none() {
    let slots = parse_data_slot_format(&html);
    // ...
}

// 4. Build UsageSnapshot with 3 windows
```

## Change Surface (~28 files, Minimax as template)

### Layer 1: Core Model (`src/model.rs`)

| Location | Change |
|----------|--------|
| L11-18: `ProviderId` enum | Add `OpencodeGo` |
| L21-28: `ALL` array | `[Self; 6]` → `[Self; 7]`, add `Self::OpencodeGo` |
| L31-40: `label()` | Add `Self::OpencodeGo => "OpenCode Go"` |

### Layer 2: Config (`src/config.rs`)

| Location | Change |
|----------|--------|
| L13-50: `Config` struct | Add `opencode_go_enabled: bool`, `selected_opencode_go_account_ids: Vec<String>`, `opencode_go_managed_accounts: Vec<ManagedOpencodeGoAccountConfig>` |
| L60-62: default fn | Add `default_opencode_go_enabled() -> true` |
| L64-94: `Default` impl | Initialize new fields |
| L100-161: helper methods | Add `OpencodeGo` arms to all 4 match methods |
| ~L268: new struct | Add `ManagedOpencodeGoAccountConfig` (id, label, workspace_id, auth_cookie_source, timestamps) |
| L295-305: `AppPaths` | Add `opencode_go_accounts_dir: PathBuf` |
| ~L408: dir helper | Add `managed_opencode_go_account_dir()` |
| L412-436: `paths()` | Add `opencode_go_accounts_dir = state_dir.join("opencode-go-accounts")` |
| L12: version | Bump `#[version = 500]` → `#[version = 600]` |

### Layer 3: Provider Module (`src/providers/opencode_go/`) — NEW

| File | Purpose |
|------|---------|
| `mod.rs` | `sync_managed_accounts()`, `fetch()` (dashboard scrape), `parse()` (SSR + data-slot parsers), URL constants, response structs |
| `account.rs` | `OpencodeGoAccount`, `discover_accounts()`, `apply_login_account()`, `remove_managed_config_dir()` |
| `login.rs` | `OpencodeGoLoginState`, `OpencodeGoLoginStatus`, `OpencodeGoLoginEvent`, `prepare()` — 2-field login (workspace ID + auth cookie) |
| `storage.rs` | `workspace_id.txt` + `auth_cookie.txt` file I/O (extends Minimax storage pattern with 2 credential files) |

### Layer 4: Adapter (`src/providers/adapters/opencode_go_adapter.rs`) — NEW

Copy `minimax_adapter.rs`, adapt for `ProviderId::OpencodeGo`.

### Layer 5: Adapter Registry (`src/providers/adapters.rs`)

| Location | Change |
|----------|--------|
| L3-8: module declarations | Add `mod opencode_go_adapter;` |
| L16-25: `adapter()` match | Add `ProviderId::OpencodeGo => &OPENCODE_GO_ADAPTER` |
| ~L32: static | Add `static OPENCODE_GO_ADAPTER: ...` |

### Layer 6: Provider Module Registry (`src/providers/mod.rs`)

| Location | Change |
|----------|--------|
| L3-11 | Add `pub mod opencode_go;` |

### Layer 7: Registry (`src/providers/registry.rs`)

| Location | Change |
|----------|--------|
| L9: imports | Add `opencode_go` |
| L18-31: `startup_sync()` | Add `opencode_go::sync_managed_accounts(config)` |
| L90-97: `fetch_handle()` | Add `ProviderAccountHandle::OpencodeGo(_) => ProviderId::OpencodeGo` |

### Layer 8: Error Types (`src/error.rs`)

| Location | Change |
|----------|--------|
| ~L592: new error enum | Add `OpencodeGoError` (LoginRequired, UsageRequest, UsageHttp, UsageEndpoint, DecodeUsage, ParseDashboard, RateLimited, CookieExpired) |
| L51-55: `From` impl | Add `impl From<OpencodeGoError> for AppError` |
| L152-166: `ProviderError` | Add `OpencodeGo(#[from] OpencodeGoError)` |
| L170-204: methods | Add `OpencodeGo` arms |

### Layer 9: Interface (`src/providers/interface.rs`)

| Location | Change |
|----------|--------|
| L3-6: imports | Add `ManagedOpencodeGoAccountConfig` |
| L56-64: `ProviderAccountHandle` | Add `OpencodeGo(ManagedOpencodeGoAccountConfig)` |

### Layer 10: App UI (`src/app/`)

Same changes as previous plan — `mod.rs` (5 Message variants, 2 AppModel fields, PopupBodyMeasurements), `login.rs` (4 handler fns), `provider_actions.rs` (delete/enable), `provider_assets.rs` (icons), `popup_view.rs` (ProviderLoginStates), `settings/accounts.rs` (section fn), `login_controls.rs` (2-field login UI), `rows.rs` (account rows).

### Layer 11-13: Supporting files

`account_selection.rs`, `account_storage/mod.rs`, `demo_env.rs` — same as previous plan.

### Layer 14: i18n (`i18n/en/yapcap.ftl`)

```
opencode-go-accounts-title = OpenCode Go Accounts
opencode-go-accounts-empty = No OpenCode Go accounts
opencode-go-account-select-required = Select an OpenCode Go account before refreshing
opencode-go-account-reauth-tooltip = Re-authenticate this OpenCode Go account
opencode-go-login-editing = Enter your OpenCode Go credentials
opencode-go-login-saved = OpenCode Go account added
opencode-go-login-failed = OpenCode Go login failed
opencode-go-workspace-id-placeholder = Workspace ID
opencode-go-auth-cookie-placeholder = Auth Cookie (from browser)
```

### Layer 15: Resources (`resources/providers/`)

2 SVG icons: `opencode-go.svg`, `opencode-go-reversed.svg`

### Layer 16: Fixtures (`fixtures/`)

`fixtures/opencode-go/` with sample dashboard HTML for testing the parser.

### Layer 17: Documentation (`docs/spec.md`)

Add section 3.7 for OpenCode Go.

### Layer 18: Dependencies

- **Add `regex` crate** to `Cargo.toml` — needed for parsing SolidJS SSR hydration output
- yapcap does not currently depend on `regex`

## Key Design Decisions

### 1. Data Source: Dashboard Scraping

**Decision**: Scrape `https://opencode.ai/workspace/<workspaceId>/go` using workspace ID + auth cookie.

**Rationale**: Exhaustively reviewed the OpenCode source (`anomalyco/opencode`). No public usage API exists. The limits are stored as an SST secret, computed server-side, and rendered only on the dashboard HTML. OpenCode's local SQLite tracks per-session costs but NOT subscription quota. The Zen API handler checks limits server-side but doesn't expose a queryable endpoint. Dashboard scraping is the only viable method — confirmed by opencode-quota and pi-usage using the same approach.

### 2. Auth Model: Workspace ID + Auth Cookie (2 fields)

**Decision**: Login UI has 2 input fields — workspace ID and auth cookie. Both obtained from the browser.

**Rationale**: The dashboard requires a browser `auth` cookie, not the API key. The workspace ID is the `wrk_...` segment of the dashboard URL. The user must extract both from their browser.

### 3. New Crate: `regex`

**Decision**: Add `regex` to `Cargo.toml` for parsing SolidJS SSR hydration patterns.

**Rationale**: The SSR format uses patterns like `rollingUsage:$R[N]={...usagePercent:X...resetInSec:Y...}` that require regex to extract. Hand-rolled string parsing would be more fragile and harder to maintain.

### 4. Config Version Bump: 500 → 600

### 5. No Host CLI Integration

Like Minimax — no inotify watchers, no `system_active_account_id`.