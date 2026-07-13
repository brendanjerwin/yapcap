---
status: done
type: AFK
blocked_by:
  - 001-shared-runtime-control-state
---

# Elect A Single Refresh Owner

## Parent

`issues/multi-process-runtime-sync/PRD.md`

## What to build

Add a refresh ownership mechanism using an operating-system file lock in YapCap's state directory. The first process to acquire the lock becomes refresh owner for its lifetime. Non-owner processes should run in read/display mode and block-wait for takeover when the owner exits and the OS releases the lock.

This slice should establish ownership state and logging, but does not need to route every refresh path through ownership yet.

## Acceptance criteria

- [x] YapCap creates and locks a refresh-owner lock file under the app state directory.
- [x] The first process to acquire the lock records itself as refresh owner for its lifetime.
- [x] A process that cannot acquire the lock records itself as non-owner and does not treat lock contention as an error.
- [x] Non-owner processes start a background blocking wait for ownership takeover.
- [x] When ownership is acquired after waiting, the process transitions to owner behavior and clears existing shared refresh requests.
- [x] Lock acquisition errors are logged loudly and leave the process in read-only/non-owner behavior.
- [x] Startup and ownership logs include PID, generated process id, COSMIC panel output, owner status, Flatpak status, and lock path.
- [x] Tests cover first-owner acquisition, second-process non-owner behavior, lock release on dropped owner handle, and takeover by a waiting process or equivalent test seam.

## Blocked by

- `001-shared-runtime-control-state`

## Completion note

Implemented owner election with a Unix file lock at `refresh-owner.lock` under the YapCap state directory. App startup now records owner/non-owner/read-only state, non-owners wait for takeover in a blocking background task, takeover stores the owner handle and clears pending shared refresh requests, and the owner-lock contract is covered by focused tests.
