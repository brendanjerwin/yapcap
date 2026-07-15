---
status: done
type: AFK
blocked_by:
  - 009-panel-app-icon-fallback
---

# Popup empty-state hero

Slice 4 of the [provider discovery PRD](../PRD.md). Visual reference: [discoverability sketch](../research/002-discoverability-sketch.html), empty state variant A.

## Scope

Upgrade the existing `no-providers` view (`src/app/popup_view/detail.rs:232`) to the settled hero:

- Centered layout: provider/app logo cloud, title, one-line explanation, suggested-style "Open Settings" button navigating to `PopupRoute::Settings(SettingsRoute::General)`.
- Hide the header "Refresh now" button while the empty state is active (`popup_header` in `src/app/popup_view.rs`).
- Centralize the empty-state condition in one function — currently "no enabled provider tabs to render"; later slices extend it. The provider nav row is also suppressed in this state.
- i18n: update existing keys' copy per the PRD (`no-providers = No providers set up yet`, `no-providers-detail = Open Settings to connect a provider and start tracking usage.`, `no-providers-open-settings = Open Settings`).
- Popup sizing must accommodate the hero (measure machinery in `popup_view.rs`).

Update `docs/spec.md`. Copy rule: "detected", never "installed", if detection is ever mentioned.

## Notes

Completed 2026-07-15: added the centered empty-state hero, suppressed provider chrome and refresh when no tabs exist, and wired measured popup sizing for the hero.
