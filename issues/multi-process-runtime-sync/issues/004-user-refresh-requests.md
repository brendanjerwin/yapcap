---
status: done
type: AFK
blocked_by:
  - 001-shared-runtime-control-state
  - 002-refresh-owner-lock
  - 003-owner-automatic-refresh
---

# Handle User Refresh Requests Across Processes

## Parent

`issues/multi-process-runtime-sync/PRD.md`

## What to build

Make explicit user refresh actions work from any YapCap process without breaking owner-only runtime writes. Clicking Refresh now in any applet process should write shared control refresh requests for all enabled providers. The refresh owner should observe those requests, ignore duplicate requests for providers already refreshing, execute the refresh, and publish shared runtime updates.

## Acceptance criteria

- [x] Refresh now from any process writes per-provider shared control requests for enabled providers.
- [x] Refresh now does not request disabled providers.
- [x] Non-owner Refresh now does not execute provider refresh directly.
- [x] The owner observes shared control requests and executes requested provider refreshes.
- [x] Duplicate requests for a provider already refreshing are ignored or coalesced without starting a second refresh.
- [x] The owner clears or consumes handled requests after publishing runtime results.
- [x] All app instances show refreshing and final runtime updates from shared runtime after a user refresh request.
- [x] Tests cover non-owner request creation, owner request handling, disabled provider exclusion, duplicate request behavior, and request cleanup.

## Blocked by

- `001-shared-runtime-control-state`
- `002-refresh-owner-lock`
- `003-owner-automatic-refresh`

## Completion Notes

2026-06-15T16:37:27+02:00 - Routed Refresh now through shared control requests, let only the owner execute observed requests, skipped duplicate in-flight provider requests, consumed handled requests after provider runtime publication, updated the spec, and added focused multi-process refresh request tests.
