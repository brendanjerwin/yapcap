---
status: done
type: AFK
blocked_by: []
---

# Add Shared Runtime And Control State

## Parent

`issues/multi-process-runtime-sync/PRD.md`

## What to build

Add COSMIC-backed shared runtime and shared control state so multiple YapCap applet processes can observe the same product state. Runtime state should replace the active use of the old snapshot cache and start cleanly from empty reconciled state when missing or invalid. Shared control should provide the separate place where explicit refresh requests can be represented without letting non-owner processes write runtime snapshots/status.

This slice should be end-to-end enough that an app instance can load durable config, load shared runtime/control, subscribe to updates, reconcile local display state, and ignore the old `snapshots.json` active path.

## Acceptance criteria

- [x] YapCap defines versioned COSMIC-backed shared runtime state containing an app-runtime payload and minimal clean metadata.
- [x] YapCap defines versioned COSMIC-backed shared control state for per-provider refresh requests.
- [x] Startup loads shared runtime and falls back to empty runtime reconciled with durable config when shared runtime is missing or invalid.
- [x] App instances subscribe to shared runtime and shared control changes through COSMIC config watching.
- [x] The old `snapshots.json` path is no longer read or written during normal runtime operation, while existing files are left untouched.
- [x] Tests cover missing shared runtime, invalid shared runtime, shared runtime load, and shared control load/update behavior.

## Blocked by

None - can start immediately.

## Completion Note

Implemented shared runtime/control COSMIC config entries, switched startup and persistence away from `snapshots.json`, subscribed app instances to shared runtime/control updates, updated the spec, and added focused coverage for missing/invalid/runtime/control shared state behavior.
