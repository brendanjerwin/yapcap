---
status: done
type: AFK
blocked_by:
  - 015-live-redetection
---

# Popup header provider picker

Slice 10 of the [provider discovery PRD](../PRD.md) — the final slice. Visual reference: [plus-button sketch](../research/005-plus-button-sketch.html), variant C.

## Scope

- Permanent `+` button in the popup header next to "Refresh now" (`popup_header` in `src/app/popup_view.rs`), hidden only in the empty state (where the hero's "Open Settings" carries the journey). No appear/disappear behavior otherwise.
- Pressing it opens a provider picker menu listing **all seven providers** in two sections:
  - `provider-picker-detected-section = Detected on this machine` — detected providers with no YapCap account, on top, with accent add-account emphasis.
  - `provider-picker-all-section = All providers` — everything else, visually quieter, **including already-configured providers** (adding a second account is a real journey).
- Every entry does the same thing: deep-link to that provider's settings page (`PopupRoute::Settings(SettingsRoute::Provider(p))`) and close the menu. Detection affects ordering and emphasis only, never clickability. The menu holds no enable/disable logic.
- Accessible label/tooltip: `add-provider = Add provider`.
- New i18n keys per the PRD: `add-provider`, `provider-picker-detected-section`, `provider-picker-all-section`.

Purely additive to the tab CTA, settings hints, and empty-state hero. Update `docs/spec.md`.

## Completion

Added the header picker, detected-first provider ordering, Settings deep-links, localized labels, and focused coverage.
