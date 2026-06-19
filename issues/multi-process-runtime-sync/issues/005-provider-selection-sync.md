---
status: done
type: AFK
blocked_by:
  - 001-shared-runtime-control-state
  - 004-user-refresh-requests
---

# Sync Provider Selection And Stale-On-Select Refresh

## Parent

`issues/multi-process-runtime-sync/PRD.md`

## What to build

Keep selected provider as immediate shared product state through durable config. Selecting a provider on one display should update all YapCap applet processes. If the selected enabled provider has missing or stale runtime state, the selecting process should create a user-driven refresh request for that provider so the owner refreshes it promptly.

## Acceptance criteria

- [x] Selecting a provider writes durable shared config immediately.
- [x] All applet processes apply selected-provider config updates and switch their selected provider.
- [x] Selecting an enabled provider with missing or stale runtime state creates a shared control refresh request.
- [x] Selecting a disabled provider does not request refresh.
- [x] Provider selection does not share popup open/closed state, popup route, focus, hover, or other transient surface state.
- [x] Tests cover cross-process selected-provider config application, stale-on-select request creation, disabled-provider exclusion, and preservation of local transient surface state.

## Blocked by

- `001-shared-runtime-control-state`
- `004-user-refresh-requests`

## Completion Notes

Completed 2026-06-15. Provider selection now persists the durable selected provider first, then writes a provider-selected shared refresh request when the selected enabled provider has missing or stale selected-account runtime data. Config-update and provider-selection tests cover cross-process selection convergence, disabled-provider exclusion, and preservation of local popup state.
