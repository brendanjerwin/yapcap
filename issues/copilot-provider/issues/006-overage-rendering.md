---
status: done
type: AFK
blocked_by:
  - 004-paid-tier-parsing
---

# Overage rendering

## What to build

When a paid Copilot account is past its included `premium_interactions` allowance (`overage_count > 0`), render a single text line in the popup directly under the premium bar: `+<N> over plan`.

No new data-model field is introduced. The parser attaches the text to the `UsageWindow` itself (via `reset_description` or an analogous existing field; choose at implementation time to minimize ripple).

See [`docs/copilot-provider.md` §9](../../../docs/copilot-provider.md). Visual validation lives in #007 via the demo seed; this slice is unit-test-validated.

## Acceptance criteria

- [x] When parsing the paid schema, if `quota_snapshots.premium_interactions.overage_count > 0`, the produced `UsageWindow` carries a `+<N> over plan` text payload (exact field decided at implementation time).
- [x] When `overage_count == 0` or absent, no overage text is attached.
- [x] Popup detail card renders the overage text directly under the premium bar with appropriate spacing and theme color. Settings/popup component-background styling unchanged.
- [x] Panel applet is unaffected — the bar still shows 100% used (or 0% remaining), overage is popup-only.
- [x] Fluent string for the overage text added to `i18n/en/yapcap.ftl` so the format is localizable.
- [x] Unit tests cover: parser branch with `overage_count: 42` produces the expected text; parser branch with `overage_count: 0` produces no text; existing parser tests unaffected.
- [x] `cargo fmt`, `just check`, `cargo test` all pass.

## Blocked by

- `004-paid-tier-parsing`

## Notes

### 2026-05-18T11:23:03+02:00

Completed overage rendering using `UsageWindow.reset_description` for the Copilot premium overage payload. Parser tests cover positive, zero, and absent overage counts; popup detail renders the payload under the premium bar with warning text color.
