---
status: done
type: AFK
blocked_by:
  - 001-shared-runtime-control-state
  - 003-owner-automatic-refresh
  - 004-user-refresh-requests
  - 005-provider-selection-sync
  - 006-login-account-actions-safe
  - 007-multi-process-diagnostics
---

# Document Multi-Process Runtime Sync And QA Plan

## Parent

`issues/multi-process-runtime-sync/PRD.md`

## What to build

Update product/spec documentation and the QA plan to describe YapCap's multi-process runtime model and how to validate it. The docs should explain COSMIC's one-process-per-output behavior, shared product state, local transient UI state, owner-only refresh, shared control requests, shared runtime state, and the fact that `snapshots.json` is no longer active runtime state.

The QA plan should include concrete manual verification steps for a two-display setup and expected log evidence for owner/non-owner behavior.

## Acceptance criteria

- [x] Product/spec docs describe COSMIC per-output applet processes and YapCap's explicit support for that model.
- [x] Product/spec docs describe which state is shared and which transient surface state remains local.
- [x] Product/spec docs describe durable config, shared control, shared runtime, and refresh owner responsibilities.
- [x] Product/spec docs state that `snapshots.json` is no longer used as active runtime state and existing files are left untouched.
- [x] QA docs include manual two-display verification for provider selection sync.
- [x] QA docs include manual verification for Refresh now from owner and non-owner displays.
- [x] QA docs include manual verification for automatic refresh running once through the owner.
- [x] QA docs include manual verification for owner takeover after the owner process/output exits.
- [x] QA docs include manual verification for login/reauth/account deletion behavior across displays.
- [x] QA docs include manual verification for disabled providers not refreshing.
- [x] QA docs include manual verification for missing/invalid shared runtime startup behavior.
- [x] QA docs include expected diagnostic log patterns for startup, ownership, requests, refresh lifecycle, and runtime updates.

## Blocked by

- `001-shared-runtime-control-state`
- `003-owner-automatic-refresh`
- `004-user-refresh-requests`
- `005-provider-selection-sync`
- `006-login-account-actions-safe`
- `007-multi-process-diagnostics`

## Completion Note

Completed 2026-06-16. Added the multi-process applet model to `docs/spec.md`,
expanded `docs/qa.md` with two-display runtime sync checks and expected log
evidence, and isolated refresh scheduling tests from demo-mode environment
leakage so the full suite is deterministic.
