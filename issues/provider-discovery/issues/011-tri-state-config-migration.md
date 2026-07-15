---
status: done
type: AFK
blocked_by:
  - 010-popup-empty-state
---

# Tri-state enablement fields + migration (dark)

Slice 5 of the [provider discovery PRD](../PRD.md). Dark-shippable: the new fields are written and migrated, but effective enablement still reads the legacy bools. Full mechanics in the [serialization asset](../research/003-tri-state-serialization.md).

## Scope

- Add `ProviderEnablement { Auto (default), Enabled, Disabled }` (`snake_case` serde, same shape as `PanelIconStyle`) to `src/config.rs`.
- Add seven `#[serde(default)]` fields `codex_enablement` … `antigravity_enablement` to `Config` at the **same version 503**. Keep the legacy `*_enabled` bools untouched for now.
- Startup migration in init, before any `write_entry`, using raw `config.get` (not the derived entry): for each provider, if `<p>_enablement` is missing and `<p>_enabled` reads as a bool, set the enablement to `Enabled`/`Disabled` per the bool. Neither key present → stays `Auto`. Idempotent and self-healing (re-runs every startup, only fills missing keys). Legacy `*_enabled` files on disk are never deleted or rewritten.
- While this slice is live, `set_provider_enabled` keeps both representations in sync (bool + explicit `Enabled`/`Disabled`), so the two fields cannot diverge before the switch-over.
- Tests: migration from bools, fresh-install `Auto`, idempotence, round-trip serialization (`auto` / `enabled` / `disabled`).

## Out of scope

Changing `provider_enabled` resolution, removing the bool fields, retiring `AutoInitPending` — all in the next slice.

## Completion

Completed 2026-07-15: added tri-state provider enablement fields, raw-key startup migration from legacy booleans, watcher support, and dual-write toggle synchronization. Effective enablement still uses the legacy booleans.
