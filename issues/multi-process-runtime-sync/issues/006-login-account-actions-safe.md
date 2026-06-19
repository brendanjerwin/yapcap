---
status: done
type: AFK
blocked_by:
  - 001-shared-runtime-control-state
  - 004-user-refresh-requests
---

# Make Login And Account Actions Multi-Process Safe

## Parent

`issues/multi-process-runtime-sync/PRD.md`

## What to build

Make login, re-authentication, account deletion, provider enablement, and account selection safe in a multi-process applet session. The process handling a user action may write credentials, account storage, and durable config immediately. Runtime status, usage snapshots, provider health, account health, auth state, and refresh errors must still be published only by the refresh owner through shared runtime.

After login or re-authentication on a non-owner process, the account should appear through shared config and use the existing refreshing state while the owner refreshes the affected provider/account.

## Acceptance criteria

- [ ] Login and re-authentication continue to write credentials/account storage and durable config from the process that owns the user flow.
- [ ] Login and re-authentication create refresh requests for the affected provider/account instead of publishing runtime snapshots from a non-owner.
- [ ] Newly added accounts appear across processes via config before owner refresh completes.
- [ ] Newly added or re-authenticated accounts show the existing refreshing state while owner refresh is pending/running.
- [ ] Account deletion and provider disablement update config immediately across processes.
- [ ] Runtime cleanup after account deletion or provider disablement is published only by the owner.
- [ ] Non-owner config reconciliation never writes shared runtime.
- [ ] Tests cover login request behavior, account appearance before runtime refresh, delete/disable config propagation, owner-only runtime cleanup, and non-owner runtime write prevention.

## Blocked by

- `001-shared-runtime-control-state`
- `004-user-refresh-requests`

## Completion Notes

- Routed login, re-authentication, account selection, provider enablement, and deletion through provider-scoped shared refresh requests.
- Kept runtime publication and cleanup owner-only while preserving immediate durable config reconciliation for account rows.
- Added tests for non-owner account selection, owner account-action refresh execution, provider disable reconciliation, and owner-only refresh request consumption.
