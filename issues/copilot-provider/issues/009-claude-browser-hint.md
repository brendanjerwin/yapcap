---
status: done
type: AFK
blocked_by: []
---

# Claude browser-account hint backport

## What to build

The Copilot add-account flow (#002) introduces a shared Fluent hint: "Sign in to your browser as the account you want to add. Use a private window to switch accounts." The same problem exists for Claude — adding a second Claude account requires logging out of the first or using a private browsing window, but the current Claude add-account flow doesn't surface this guidance.

Add the same hint near Claude's add-account button.

Independent of all Copilot slices — can ship at any time.

## Acceptance criteria

- [x] A Fluent string is added (or reused if #002 already added it) and rendered in Settings → Claude near the **Add account** control.
- [x] String is shared between Claude and Copilot add-account UIs (one canonical Fluent key, used by both providers' Settings views).
- [x] Hint text is short, single-line, not styled as an error.
- [x] Visual check: Claude Settings → Add account shows the hint above or near the button.
- [x] `cargo fmt`, `just check`, `cargo test` all pass.

## Blocked by

None — can start immediately.

## Completion note

Shared the browser account/private-window hint between Claude and Copilot settings through one Fluent key and documented the Claude behavior in `docs/spec.md`.
