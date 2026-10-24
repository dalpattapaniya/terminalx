# Local background runtime proposal (#467)

Status: investigation and validation preparation, **not implemented**. Baseline:
`958d1c77` (includes #464 / #621). #567 / #622 reserves session, pipeline,
control, registration and remote protocol integration. This document requests
those integration points; it does not authorize changing their contracts.

## Current ownership and reusable paths

| Concern | Current owner and path | Required background owner |
| --- | --- | --- |
| Agent processes | Desktop `AppState` constructs `Host`, `Terminals`, `SessionManager`; `lib.rs` destroys them with the main window | The same objects in `serve.rs`, one per data directory |
| PTY output/input | `pty.rs` reader, bounded 512 KiB byte tail, atomic `attach` replay/live handoff, tap acknowledgements; desktop channels in `sink.rs` | Runtime owns reader, history and flow control; window is a detachable tap |
| Chat/status | `session.rs` projects provider transcripts into stored events, index and activity | Runtime tails and publishes while no window exists |
| Permissions/questions | Authenticated `hooks.rs` frames enter `SessionManager::on_hook`; in-memory pending asks and decision senders; control decisions return to the blocked hook | Runtime retains the actual decision receiver; reconnect snapshots must distinguish pending from expired asks |
| Queue | PTY CLI owns typed pending prompts; session tracks echoes; headless `TabRuntime.queued` drains on turn completion | Runtime keeps both paths and #567 continuation authorization intact |
| Terminal-launched agents | `session_follow.rs` foreground monitoring currently starts only on desktop | Enable for local background runtime, not implicitly for cloud |
| Recovery | `serve::start` calls `idle_orphaned_tabs`; desktop also initializes local stores | Only a new runtime generation recovers old records; UI attachment must never initialize or reset runtime stores |
| Control | Per-launch authenticated socket plus `hooks.sock` publication guarded by `hooks.lock` | Keep per-launch hook identity; a separate lifetime lock must prevent multiple local owners before any store mutation |
| Cloud RPC | `remote/{host,server,protocol}` exposes versioned workspace methods with authority checks and idempotency | Reuse handlers over authenticated local transport, preserving cloud capabilities and authorization |

`terminalx-serve --runtime-kind local` currently has no UI lease, idle shutdown,
desktop notification host or local workspace RPC listener. Its control socket
is a request/response CLI surface, not the event/PTY subscription protocol.
`BroadcastSink` is bounded and can lag: subscribing alone is not a durable replay
mechanism. Local mode does not automatically start the cloud agent mailbox or
pipeline observer; these must not be activated using synthetic cloud identity.

## Proposed ownership and attachment

Launch a signed, versioned local `terminalx-serve` helper independently of the
desktop process, with standard streams redirected to bounded private logs and
no inherited window pipes. Use the same login environment resolution and provider
homes as current desktop launches. Do not move, migrate or refresh provider
credentials. Child processes receive the runtime's hook/control endpoint, never
the window's PID or bundle-relative hook executable.

Before initializing any store, acquire an OS lifetime lock under the canonical
data directory. A losing launcher connects to the authenticated owner or reports
an incompatible live owner; it must not reset tabs or replace the owner's token.
The existing publication lock only elects a socket publisher and permits several
live runtimes, so it is insufficient for store ownership. Never unlink a held
lock file. Refuse symlinked/insecure endpoint artifacts and keep directory and
socket permissions owner-only. Boot/runtime generation, protocol capabilities
and process identity belong in the authenticated handshake; a PID file alone is
not proof of ownership. Retain per-tab hook authentication and remote authority
checks. A paired viewer must never acquire local-owner powers through this path.

The desktop becomes a client before loading sessions. Sequence:

1. Discover or start the owner, authenticate and negotiate supported namespaces.
2. Establish event subscription before taking the session/status/pending-ask
   snapshot at a revision watermark. Replay stored events beyond each chat
   cursor, deduplicate by event identity, then follow live events.
3. Attach each visible PTY with the existing atomic history/live boundary;
   forward input, resize and acknowledgements. A disconnected window releases
   its taps so stale flow-control credits cannot block a background command.
4. On lag or reconnect, repeat snapshot/replay; never call `ensure_started` just
   because transport disconnected. Failed transport is not a finished turn.

Use an additive local lifecycle namespace for attach/lease/detach and stop-all;
exact wire fields are deferred until #567 lands. Do not proxy arbitrary Tauri
command names. Inventory `commands.rs`, `src/lib/api.ts` and portable workspace
methods before wiring: local mode must support multiple projects, unlike the
existing one-root cloud connection. Destructive worktree operations stay at the
runtime owner and preserve #464 history retention and removal dispositions.

## Lifetime and honest recovery

Proposed unattached idle grace: 60 seconds, reset whenever real work or a client
returns. Pending approvals/questions, queued delivery, pipelines, cleanup and
in-flight mutations count as work. Do not reuse `turn_counts()` as the shutdown
predicate: its cloud silence budget eventually excludes a still-live turn.
Unknown terminal activity is busy; silence is never evidence that a command
finished. A plain shell that can be proven idle may close at grace expiry; a
foreground job, stopped job or unclassified descendant keeps the runtime alive.
Idle agent CLIs may close only after their turns, pending asks and queues drain.
Idle shutdown and new attach/start must serialize under the lifecycle state
machine so work cannot be accepted after shutdown commits.

Stop-all first rejects new work and cancels queues/automations, then records
intentional stop, terminates process groups using existing teardown, flushes
activity and releases sockets/lock last. It is distinct from client detach.
SIGTERM/logout ends work and persists interruption honestly. A crash or reboot
creates a new generation; retained transcripts survive but pending decisions
and PTY process identities must not be reported live. No automatic restart of
an interrupted turn. Do not configure a helper to restart after logout or boot.

## Permission and output constraints requiring implementation

Keeping the runtime alive does **not** yet preserve an overnight approval:
`ask_permission` has a provider-specific deadline and on expiry publishes
`Lapsed` and `PermissionExpired`. Codex also imposes a hook timeout. Simply
persisting an answerable card or removing TerminalX's timeout would be false.
Validate each provider's actual hook/TUI behavior and implement a live pending
decision path or an explicit provider prompt handoff with a valid response
mechanism. Do not auto-allow, replay an expired decision into another turn, or
claim this acceptance criterion passes before provider validation.

The existing 512 KiB PTY tail is memory-only and may trim at a newline on attach.
It cannot promise unlimited full scrollback. Keep bounded retention with explicit
truncation information, and retain finished panes until the reader can observe
their result. A disk spool, if needed for grace expiry, must be owner-only,
bounded per pane and globally, distinguish output from process liveness, and
never resurrect an exited process. Coordinate all transport/renderer changes
with #232; do not add a competing PTY implementation.

## Window features, updates and macOS

Proposed background duties: automations/scheduler, pipeline runtime observer,
pairing endpoint, notification decisions and tray count/actions. Exactly one
runtime owns each scheduler and notification delivery identity. `serve` currently
excludes automations and desktop notification plumbing; feature gating must be
factored before claiming parity. A native accessory helper can own the menu-bar
item and notification delivery without a Dock icon; a headless Rust process
alone cannot retain the desktop's existing tray. Tray actions: Open TerminalX,
Stop all, Stop all and quit; counts come from runtime state.

Dictation, computer use, built-in browser and star-reminder presentation remain
window-owned. While detached, browser/computer calls report unavailable rather
than spawning another window or pretending success. Dictation changes stay with
#439. Settings and all normal quit routes share a persisted first-choice policy:
keep running by default, or stop all; cancellation changes nothing. Explicit
stop-all always overrides that preference. Preserve the worktree-cleanup guard
for work that would actually be interrupted. Update restart always detaches.

Copy the bundled signed helper and its required resources into a private,
versioned runtime location before launch; atomically install and verify its code
signature, preserving required entitlements. Later hooks must resolve that
retained executable even after app bundle replacement. Never overwrite/delete a
running version. New desktop versions negotiate compatibility with the running
owner; incompatible upgrades must preserve work and explain incompatibility,
not silently kill/restart it. Versioned helper retention ends only after its
owner exits. Validate keychain access under the helper signing identity and
notification authorization on macOS; these are not established by a shell test.

## Integration release request and reviewable stages

Coordinator release is needed for `session.rs`, `session_follow.rs`,
`issue_pipeline/*`, `remote/{host,server,protocol}.rs`, `control.rs`,
`session_ops.rs`, hooks/launch registration and CLI integration. Additional
production wiring will need `serve.rs`, `lib.rs`, `commands.rs`, `sink.rs`,
`quit.rs`, settings/tray/automations/pairing, `src/lib/api.ts`, portable workspace
client, helper packaging/signing and tests. #232 must release PTY transport
integration separately. No reserved files are changed by this preparation.

Proposed stages for coordinator agreement **before any partial PR**:

1. Runtime ownership, authentication and local transport, with process tests;
   cloud and existing desktop behavior remain runnable.
2. Desktop attachment, replay and provider prompts, plus quit policy, helper
   tray/notifications/automations and update-safe packaging; enable only with
   complete behavior. Preserve #567 and #464 contracts.
3. Native acceptance and repair across all providers, real process exit and
   signed bundle replacement; run all four CONTRIBUTING checks at final head.

Prefer incremental commits in one complete PR. If stages require separate PRs,
none may auto-close #467 until the final stage meets every acceptance criterion.

## Disposable validation

`scripts/local-runtime/probe.py` takes an explicitly supplied built serve binary.
It uses a short private temporary directory, an allowlisted child environment,
no provider credentials or installed app, and real authenticated control calls.
It verifies launcher exit, reconnect to the same runtime PID, wrong-token
rejection, explicit SIGTERM and fresh endpoint credentials after restart. The
built-in PTY self-test separately exercises a real shell. These are baseline
prerequisites, **not** desktop acceptance or stop-all/idle protocol coverage.

Extend after release of integration using existing fake-Claude/mailbox fixtures
and `serve/tests/relay_e2e.rs` reattachment coverage: keep a delayed fake turn and
numbered terminal writer running after the UI client process exits; compare
complete event/byte sequences and PIDs after reconnect; hold a question and
permission across detach and answer once; verify stale decisions are refused;
exercise queues and all four harnesses. Test stop-all descendants, idle grace
reset/races, singleton concurrent launch, SIGKILL/restart recovery, bounded spool
overflow and auth/version mismatch without touching real data.

Native acceptance must use a disposable signed test bundle and private homes,
with coordinator-operated UI: start work, truly exit the desktop process, verify
helper/tray/notifications and same child PIDs, reopen and answer prompts, then
replace the test bundle while work continues and launch the new client. Record
versions, runtime/child PIDs, before/after output cursors and visible results.
Window hide/close and a raw executable rename are insufficient evidence for
signed updater survival. The installed live app is never a test target.

### Baseline results (2026-10-24, macOS)

- `CARGO_BUILD_JOBS=2 SDKROOT="$(xcrun --sdk macosx --show-sdk-path)" cargo build
  --manifest-path src-tauri/serve/Cargo.toml --bin terminalx-serve`: passed.
- `python3 scripts/local-runtime/probe.py src-tauri/target/debug/terminalx-serve`:
  passed launcher exit, authenticated reconnect, wrong-token rejection, SIGTERM,
  fresh restart credentials and real PTY output/exit assertions.
- `cargo test --manifest-path src-tauri/serve/Cargo.toml --test fake_agent`, with
  jobs=2, SDKROOT and disposable `TERMINALX_HOME` / `CLAUDE_CONFIG_DIR`: one test
  passed. Existing fake CLI covered transcript projection, permission allow/deny
  and mid-turn steering through the real harness. This test does not detach a
  desktop or hold permissions past provider deadlines.
- Python syntax and `git diff --check`: passed.

The four final CONTRIBUTING checks and native acceptance have not been run for
this preparation. Production behavior is unchanged; #467 is blocked on the
coordinator's integration release, not ready for a completion PR.
