# Architecture and contracts

## Processes

`sensorpanel.exe --panel` owns the display window, tray icon, editor, basic Win32 readings, and external AI input. `--collector` owns NVML and PawnIO. Hardware libraries load only in the collector. `--diagnose` writes a bounded diagnostic capture to JSON.

The two modes share an executable but not application memory. A blocked hardware query arms a watchdog that terminates only the collector. The panel retains its clock and basic Windows readings while waiting to reconnect.

## Modules

| Module | Responsibility |
|---|---|
| `config` | Versioned profiles, validation limits, defaults, and backup writes |
| `ha_config` | Separate TOML integration settings, safe defaults, bounded reloads and credential-redacted errors |
| `ha_client` | Lazy single-thread Tokio/reqwest worker, bounded channels, 3 s HTTP timeout and fixed HA service routing |
| `model` | Sensor catalog, readings, quality, bindings, and bounded history |
| `platform` | Basic Win32 interoperability, user identity, DLL loading, and mutexes |
| `sensors/basic` | Win32 CPU/RAM and external Registry input |
| `sensors/nvml` | NVIDIA queries using UUID identity, without control commands |
| `sensors/fps` | Verified PresentMon helper, bounded timing aggregation, foreground application selection |
| `sensors/system` | Unelevated IP Helper byte-counter deltas and read-only Windows power-scheme lookup |
| `sensors/pawnio` | Bounded access to official modules and explicitly supported hardware |
| `ipc` | Protected pipe, frame validation, supervision, and query watchdog |
| `ui/monitors` | Active displays and persistent display identity |
| `ui/render` | Direct2D/DirectWrite rendering, images, and unit conversion |
| `ui/footer` | Footer readout, unavailable states, reserved slots, and settings hit testing |
| `ui/dashboard` | One-second presentation cache: semantic colors, strings/UTF-16, optional labels, critical cards, and footer state |
| `ui/settings` | Native Resumen/Diseño settings pages, live dashboard controls, validated thresholds and minimal preset |
| `ui/access` | Home Assistant card state, optimistic rollback, confirmation deadlines, grid pages and prepared visuals |
| `ui` | Native windows and editor controls |

## Readings and protocol

`Sensor.id` includes physical identity and metric. `Sensor.key` identifies a logical role, such as `cpu.temperature`. The panel resolves stored bindings before drawing and does not silently change devices when a previously bound device disappears.

Each sensor carries an optional value, unit, nominal interval, sample timestamp, quality, and message. Quality is `valid`, `reported`, `unavailable`, `error`, or `stale`. A missing reading for a known sensor preserves the last value as stale. Zero is valid, particularly for stopped fans.

`Sensor.reset_at_ms` is optional additive metadata for externally supplied AI reset times; older snapshots omit it. The local panel's basic Windows/Registry readings are applied after collector snapshots, so an older collector cannot erase newer optional Registry data. Dashboard-only network and power-plan queries run in the unelevated panel and do not introduce subprocesses or IPC commands. Power scheme names are cached by GUID. Network deltas use monotonic elapsed time and per-interface LUID baselines; newly appearing or reset counters are never treated as traffic bursts.

JSON frames end with a newline, use protocol version 1, and are limited to 256 KiB. Snapshots carry catalog and readings. Clients can activate/pause subscriptions or request shutdown. No message can specify registers, load code, or change hardware controls.

The pipe allows the user's SID and SYSTEM, rejects remote clients, and bounds catalog size. The collector claims the first pipe instance to prevent duplicates. Profiles and image paths belong to the unelevated panel.

## Adding hardware

Add an adapter under `sensors` that produces `Sensor` values with persistent identities and explicit units. Incorporate it into the collector catalog and add meaningful decoding fixtures. For low-level operations, recognize the model/chip first and use only published signed modules. Preserve the reference implementation's exclusions and bus mutexes. Keep hardware control outside the reading interface.

A replacement NVML-compatible GPU can be assigned in the editor. The memory-temperature fallback and AMD/ITE access have narrower initial compatibility lists and require device-specific validation before expansion.

Incompatible profile changes require a `schema_version` increase and an explicit migration. Incompatible IPC changes require a `PROTOCOL_VERSION` increase. Unknown versions are rejected.

## Recovery

On the validation PC, Windows did not restart the collector after termination despite a configured `RestartOnFailure` policy. The panel therefore requests the collector task after 60 seconds disconnected, at most three times per panel session. It invokes only `schtasks.exe /Run /TN \SensorPanel\Collector`, without a shell, profile-supplied arguments, or daily UAC prompts. The helper process exists only for the restart request.

The collector task has no additional Windows retries, avoiding duplicate budgets. Exit sends a voluntary shutdown and closes the client, preventing recovery from restarting it. Retry policy is tested with a logical clock and with actual termination of the installed collector.

## Resources and permissions

PresentMon 2.3.1 is a separate console helper with a pinned SHA-256 verified before execution. Its stdout is consumed on a reader thread into at most 64 swap chains, each with at most 4096 frame intervals and a one-second rolling window. It emits only `game.fps` and `game.frame_time` samples through the existing protocol. Selection requires foreground presentation activity, preserves the previous target during an active background session, and expires idle data after two seconds. Missing FPS explicitly clear the footer even when the general sensor store retains a stale value.

The helper disables display, input, and GPU timing tracking and uses a private per-user ETW session. A named mutex prevents competing CasePanel captures. Pausing/disconnecting stops that session; resuming recreates it. Capture restart is limited to three attempts per active session, spaced one minute apart after failure. A Windows job closes the helper if the collector terminates, and the next launch cleans up its own previously abandoned ETW session. No helper-supplied commands or profile-supplied executable paths are accepted.

The footer is additive profile data and keeps schema version 1. Older profiles enable it only when there is at least a 1200 × 600 canvas and the bottom 150 pixels are completely clear. Explicit `footer.enabled: false` is preserved. Existing widgets are unchanged. Settings hit testing converts client coordinates through the same viewport as rendering, handles only local button release, and preserves the panel's non-activating behavior.

Software rendering at 1 Hz avoids the persistent graphics-driver allocations observed with accelerated rendering on the integrated AMD GPU. Text formats, brushes, and images are cached; drawing resources are recreated on resize or target loss.

`App::tick` prepares one `DashboardState` shared by the panel and editor. Semantic colors, formatted values/clock/titles, countdowns, network strings and footer text are calculated only at this one-second sensor tick. Additional paint/resize messages reuse prepared values and UTF-16. Critical alerts use a fixed 1 px border at alpha 0.6, avoiding another animation loop. Existing widget coordinates are preserved; micro-labels follow the six built-in bar IDs, network text follows `case_bar`, reset timers follow AI value widgets, and critical outlines follow built-in card IDs. Custom layouts can switch these additions off independently.

Dashboard options and threshold subfields have Serde defaults and remain schema version 1. The Resumen settings page previews changes within one tick, validates finite ordered warning/critical pairs, saves through the existing atomic profile writer, and participates in the editor's save/discard behavior. The minimal preset retains layout, bindings, and thresholds. Disabling semantic colors overrides critical borders and navigation accents as well. Optional unbound metrics resolve only when one device matches; an explicit binding to a missing device never falls back to another one.

The panel uses `HWND_BOTTOM` and `SWP_NOACTIVATE` on startup and display changes. It stays behind applications and preserves the user's focus. The editor behaves as a normal window.

Installed executables live in Program Files, writable by administrators and SYSTEM. Elevation is limited to installation and the collector. AI authentication and renewal remain outside both processes.

## Home Assistant

The unelevated panel owns Home Assistant. Neither the elevated hardware collector nor its IPC protocol receives credentials or commands. `config.toml` beside the selected JSON profile contains `[home_assistant]`; it is independent of layout imports/exports. The file is bounded to 64 KiB, compared at 1 Hz and parsed only when its contents change. Invalid edits stop the old worker and clear pending UI state; valid edits create a new worker only when enabled and fully configured. TOML source snippets and bearer tokens are excluded from errors/Debug output.

Each worker has a current-thread Tokio runtime, an eight-command input channel and a 32-event output channel. Reqwest uses Windows native TLS, a three-second timeout including response consumption, no redirects and no environment proxies. State bodies are capped at 4 MiB and only configured entities enter the UI store. Reads use `/api/states`; commands are selected only by configured action ID and map to fixed `homeassistant/toggle`, `script/turn_on` or `scene/turn_on` paths. Read-only actions have no service path. No POST is automatically replayed. Dropping the client cancels outstanding futures and queued commands without joining on the UI thread.

Polling schedules from the last completed read: configured seconds in Accesos, 30 seconds elsewhere. Navigation wakes the worker to recalculate its deadline. Actions are serialized; queued clicks older than three seconds are rejected instead of executing late. A completed or failed command schedules reconciliation via GET. Pending actions are tracked by entity so duplicated cards cannot issue overlapping toggles; in-flight poll replies preserve pending optimistic values. Errors restore the saved pre-click state.

The one-second presentation tick prepares all on/off/pending/confirmation/success/unavailable/sensor variants. Input callbacks only select variants and enqueue commands; they do not format strings or recalculate colors. Confirmation uses a monotonic three-second deadline and trigger success a 1.2-second deadline. A one-shot Win32 timer switches back to existing variants at expiry. The render loop stays at 1 Hz apart from input/feedback invalidations. The grid paginates when actions exceed visible rows. Resumen and the widget editor keep their original state and dashboard settings.
