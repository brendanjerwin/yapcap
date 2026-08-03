---
status: done
type: AFK
blocked_by:
  - 011-tri-state-config-migration
---

# Switch enablement resolution to tri-state

Slice 6 of the [provider discovery PRD](../PRD.md). After this slice, fresh installs are disabled-by-default (`Auto`) and detection drives visibility; existing users are unaffected (migration wrote explicit values in slice 011).

## Scope

- Effective enablement resolves per the PRD: `Auto → detected(provider) || provider has at least one YapCap account`, `Enabled → true`, `Disabled → false`. Account presence always wins.
- `Config::provider_enabled(&self, provider)` cannot see detection; restructure so resolution happens where config, the `DetectionSnapshot`, and account presence meet (e.g. a resolver taking the snapshot, or resolved flags held in app state and refreshed on config/detection/account changes). Update all call sites (`src/app/mod.rs`, `refresh.rs`, `provider_actions.rs`, `applet.rs`, `runtime.rs`, `providers/adapters.rs`, `demo_env.rs`); keep the resolution rule in exactly one place.
- Remove the legacy `*_enabled` bool fields from `Config` (on-disk files stay, untouched).
- `set_provider_enabled` / the settings toggle write explicit `Enabled`/`Disabled` only — plain two-state toggle, `Auto` unreachable after any manual touch, no reset affordance. Toggle rendering: a provider that is `Auto`-resolved-enabled shows as on; toggling writes the explicit value.
- Retire the `AutoInitPending` force-enable: `registry::initialize_provider_visibility` must no longer mass-write enabled values (remove the mechanic; keep whatever is needed so existing configs with either mode value still deserialize).
- Tests: `Auto` + detected, `Auto` + account-no-detection, `Auto` + neither, explicit states override detection, fresh-install default resolves to zero enabled when nothing is detected.

Update `docs/spec.md` (enablement model changes). Together with slices 009–010 a fresh install now shows the app-icon panel and empty-state hero.

## Completion

Implemented the single tri-state resolver, removed legacy in-memory booleans and the auto-enable pass, and reconciled effective enablement through runtime state.
