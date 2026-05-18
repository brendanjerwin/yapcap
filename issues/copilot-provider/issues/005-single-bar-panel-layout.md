---
status: done
type: AFK
blocked_by:
  - 004-paid-tier-parsing
---

# Single-bar panel layout

## What to build

Extend the panel applet to render either one or two bars per account, depending on the account's snapshot. Paid Copilot accounts (one `premium_interactions` window) render a single bar **vertically centered** within the same total height as the two-bar layout. Free Copilot accounts continue to render two bars.

Mixed selection within Copilot (one Free + one paid account, both selected) renders each account's own honest bar count side-by-side — no homogenization across columns.

See [`docs/copilot-provider.md` §8](../../../docs/copilot-provider.md).

## Acceptance criteria

- [x] `UsageSnapshot::applet_windows()` (and the panel applet rendering math) detects 1-bar vs 2-bar shape per account. Suggested shape change: return `(UsageWindow, Option<UsageWindow>)` instead of `(Option<&UsageWindow>, Option<&UsageWindow>)`, or a similar API that makes "single bar" explicit. Final API decided at implementation time.
- [x] When the second window is absent, the single bar renders vertically centered within the same vertical extent as the two-bar layout. Width matches the two-bar layout's bar width.
- [x] Existing two-bar accounts (Codex, Claude, Cursor, Gemini, Copilot Free) are unaffected — visual regression tests or screenshots confirm.
- [x] Mixed-account Copilot panel: with both Free and paid accounts selected and `show_all_accounts: true`, the panel renders 2 bars next to 1 centered bar; columns are separated by `APPLET_PERCENT_ACCOUNT_GAP` (or the existing bar-style equivalent). No alignment across columns is enforced.
- [x] `logo_and_percent` and `percent_only` panel styles still work for paid Copilot accounts (single percentage column unchanged).
- [x] Native and Flatpak both autosize the panel surface correctly for the new layout.
- [x] Unit tests cover the single-bar branch in `applet_windows()` and the panel rendering math.
- [x] `cargo fmt`, `just check`, `cargo test` all pass.

## Blocked by

- `004-paid-tier-parsing`

## Completion note

Implemented explicit one-bar/two-bar applet window layout, centered single-window bar rendering, mixed Copilot account layout coverage, and updated panel spec wording.
