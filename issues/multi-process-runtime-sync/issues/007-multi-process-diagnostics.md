---
status: done
type: AFK
blocked_by:
  - 002-refresh-owner-lock
  - 003-owner-automatic-refresh
  - 004-user-refresh-requests
---

# Improve Multi-Process Diagnostics

## Parent

`issues/multi-process-runtime-sync/PRD.md`

## What to build

Improve logging so multi-process YapCap behavior can be diagnosed from logs without manual process-table inspection. Logs should make it clear which process and panel output owns refresh, which process created a request, when ownership changes, and when shared runtime/control state changes are observed.

## Acceptance criteria

- [x] Each process generates and logs a short process id at startup.
- [x] Startup logs include PID, process id, COSMIC panel output, owner/non-owner status, Flatpak status, lock path, config version, shared runtime version, and shared control version.
- [x] Ownership logs cover acquired ownership, waiting as non-owner, takeover, and lock errors.
- [x] Shared control logs cover refresh request creation and owner observation of requests.
- [x] Refresh logs cover provider refresh start, provider refresh finish, skipped duplicate requests, skipped disabled providers, and refresh errors.
- [x] Shared runtime logs cover writes, observed generation/state changes, missing runtime fallback, and invalid runtime parse fallback.
- [x] Logging additions do not add user-visible owner/non-owner UI.
- [x] Tests or helper-level assertions cover process identity formatting where practical without brittle full-log matching.

## Blocked by

- `002-refresh-owner-lock`
- `003-owner-automatic-refresh`
- `004-user-refresh-requests`

## Completion Notes

2026-06-16T09:00:18+02:00 - Added short process ids with focused formatter coverage and expanded structured diagnostics for startup ownership, takeover, shared runtime/control load/write/watch events, refresh request creation/observation/skips, provider refresh start/finish, and runtime fallback paths.
