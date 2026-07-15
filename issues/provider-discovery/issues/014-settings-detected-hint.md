---
status: done
type: AFK
blocked_by:
  - 013-detected-tab-and-cta
---

# Settings detected hint

Slice 8 of the [provider discovery PRD](../PRD.md). Visual reference: [discoverability sketch](../research/002-discoverability-sketch.html), settings hint variant A.

## Scope

For a detected-but-unconfigured provider (detected, no YapCap account):

- Accent-colored dot on that provider's settings category tab, reusing the update-notification dot mechanic (`settings_category_tab` / `update_notification_dot` in `src/app/popup_view.rs`) but in the theme accent color, not red. The existing red update dot on the General tab is unchanged.
- One-line caption on that provider's settings page: `provider-detected-caption = Detected on this machine`. No layout changes beyond the caption line.
- Both hints disappear once the provider has an account; explicit `Disabled` does not suppress the hints (the settings page is exactly where a disabled-but-detected provider is re-enabled).
- New i18n key per the PRD: `provider-detected-caption`.

Copy rule: "detected", never "installed". Update `docs/spec.md`.

## Completion note

Implemented the accent settings-tab dot and detected caption for providers with no YapCap account, including explicitly disabled providers.
