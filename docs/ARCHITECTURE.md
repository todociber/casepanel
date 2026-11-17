# Architecture and contracts

## Processes

`sensorpanel.exe --panel` owns the display window, tray icon, editor, basic Win32 readings, and external AI input. `--collector` owns NVML and PawnIO. Hardware libraries load only in the collector. `--diagnose` writes a bounded diagnostic capture to JSON.

The two modes share an executable but not application memory. A blocked hardware query arms a watchdog that terminates only the collector. The panel retains its clock and basic Windows readings while waiting to reconnect.

## Modules

| Module | Responsibility |
|---|---|
| `config` | Versioned profiles, validation limits, defaults, and backup writes |
| `model` | Sensor catalog, readings, quality, bindings, and bounded history |
| `platform` | Basic Win32 interoperability, user identity, DLL loading, and mutexes |
| `sensors/basic` | Win32 CPU/RAM and external Registry input |
| `sensors/nvml` | NVIDIA queries using UUID identity, without control commands |
| `sensors/fps` | Verified PresentMon helper, bounded timing aggregation, foreground application selection |
| `sensors/pawnio` | Bounded access to official modules and explicitly supported hardware |
| `ipc` | Protected pipe, frame validation, supervision, and query watchdog |
| `ui/monitors` | Active displays and persistent display identity |
| `ui/render` | Direct2D/DirectWrite rendering, images, and unit conversion |
| `ui/footer` | Footer readout, unavailable states, reserved slots, and settings hit testing |
| `ui` | Native windows and editor controls |

## Readings and protocol

`Sensor.id` includes physical identity and metric. `Sensor.key` identifies a logical role, such as `cpu.temperature`. The panel resolves stored bindings before drawing and does not silently change devices when a previously bound device disappears.

Each sensor carries an optional value, unit, nominal interval, sample timestamp, quality, and message. Quality is `valid`, `reported`, `unavailable`, `error`, or `stale`. A missing reading for a known sensor preserves the last value as stale. Zero is valid, particularly for stopped fans.

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

The panel uses `HWND_BOTTOM` and `SWP_NOACTIVATE` on startup and display changes. It stays behind applications and preserves the user's focus. The editor behaves as a normal window.

Installed executables live in Program Files, writable by administrators and SYSTEM. Elevation is limited to installation and the collector. AI authentication and renewal remain outside both processes.
