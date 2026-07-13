---
status: done
type: AFK
blocked_by:
  - 001-shared-runtime-control-state
  - 002-refresh-owner-lock
---

# Route Automatic Refresh Through The Owner

## Parent

`issues/multi-process-runtime-sync/PRD.md`

## What to build

Make automatic timer refresh owner-only. The owner evaluates enabled providers for staleness, refreshes selected accounts by default, and writes shared runtime state. Non-owner timer ticks should not create automatic refresh requests and should never execute provider refresh work.

The owner should publish the existing refreshing state before provider refresh begins and publish final provider/account runtime state after refresh completes.

## Acceptance criteria

- [x] Automatic timer refresh is executed only by the refresh owner.
- [x] Non-owner timer ticks do not refresh providers and do not create automatic refresh requests.
- [x] The owner refreshes enabled providers that are missing or stale according to the configured refresh interval.
- [x] Disabled providers are skipped by automatic refresh.
- [x] Before refreshing a provider, the owner writes shared runtime with that provider in the existing refreshing state.
- [x] After refresh completes, the owner writes final shared runtime with snapshots, health, auth state, errors, and refreshing cleared.
- [x] Shared runtime updates are observed by all app instances through COSMIC config watching.
- [x] Tests prove owner timer refreshes, non-owner timer does not refresh, disabled providers are skipped, and shared `is_refreshing` drives the existing UI state.

## Blocked by

- `001-shared-runtime-control-state`
- `002-refresh-owner-lock`

## Completion Notes

2026-06-15T16:30:40+02:00 - Routed startup/timer refresh through the refresh owner, filtered automatic refresh to missing or stale selected accounts, published refreshing state before provider work, and added focused owner/non-owner timer tests.
