---
status: done
type: AFK
blocked_by:
  - 012-enablement-resolution-switch
---

# Detected-provider tab chip + add-account CTA

Slice 7 of the [provider discovery PRD](../PRD.md). Visual reference: [discoverability sketch](../research/002-discoverability-sketch.html), detected state variant A.

## Scope

A detected provider with no YapCap account (effectively enabled via `Auto`) already gets a normal nav tab after slice 012. This slice gives its body the settled treatment:

- Nav tab stays completely normal (empty usage bar) — no tab-level badge.
- Body: "Detected" chip next to the provider name, plus an add-account call to action using the PRD copy — `provider-detected-cta = { $provider } was detected on this machine, but YapCap has no account for it yet.` — and a button (`provider-detected-add-account = Add account in Settings`) deep-linking to `PopupRoute::Settings(SettingsRoute::Provider(p))`.
- Chip appears only in the detected-and-no-account state; once an account exists the body is the normal provider view.
- New i18n keys per the PRD: `provider-detected-chip`, `provider-detected-cta`, `provider-detected-add-account`.
- Empty-state condition from slice 010 stays consistent: the popup is only "empty" when there are no tabs at all.

Copy rule: "detected", never "installed". Update `docs/spec.md`.

## Completion

Implemented 2026-07-15: detected providers with no YapCap account now show an accent Detected chip and a Settings deep-link CTA; account-bearing and undetected providers retain the normal detail view.
