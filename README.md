# CasePanel

A native Windows hardware monitoring panel built with Rust, Win32, Direct2D, and DirectWrite.

**[Download Windows builds](https://github.com/todociber/casepanel/releases)** · [Architecture](docs/ARCHITECTURE.md) · [Validation results](docs/VALIDATION.md)

CasePanel displays hardware readings on a dedicated monitor using a configurable layout. It draws once per second, keeps a bounded history, and runs its hardware collector in a separate process. Rendering uses software Direct2D; no browser or .NET runtime is required.

## Download and install

This is an early **0.1.2 preview** for Windows 11 x64. Low-level sensor support has been validated on one hardware configuration; see the support table and outstanding tests below.

| Release asset | Purpose |
|---|---|
| `CasePanel-0.1.2-windows-x64.zip` | Recommended package: application, setup scripts, verified official PawnIO installer, documentation, licenses, and source |
| `sensorpanel.exe` | Application executable for manual use; it is not a setup program and does not install dependencies or startup tasks |
| `CasePanel-0.1.2-source.zip` | Source for this build, including the corresponding PawnIO module source |
| `SHA256SUMS.txt` | SHA-256 checksums of the downloadable files |

Extract the Windows ZIP, open PowerShell **as administrator** in the extracted folder, and run:

```powershell
.\scripts\Install.ps1
Start-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Collector'
Start-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Panel'
```

Installation copies the application to `C:\Program Files\SensorPanel` and creates two Task Scheduler tasks. The panel runs without elevation; the collector runs elevated. Both start five seconds after you **sign in**, with no daily UAC prompt. Scheduled tasks are managed in **Task Scheduler → Task Scheduler Library → SensorPanel**, rather than Task Manager's Startup apps list.

The installed executable, task names, and profile directory retain the `SensorPanel` name for compatibility. The application executable is currently unsigned; the bundled, unmodified PawnIO installer is separately Authenticode-signed by its publisher. Windows may display an executable reputation warning.

The default layout targets a **PM1562, 1920 × 1200**. Use the tray icon to open the editor and select another display. The panel opens behind other applications without taking focus. If its display is missing, it hides and pauses collection; the tray remains available.

Install options:

```powershell
# Register disabled tasks for review.
.\scripts\Install.ps1 -StageOnly
# Change the logon delay; optionally back up and disable the old AIDA64 task.
.\scripts\Install.ps1 -DelaySeconds 5 -DisableAidaAutostart
```

To remove CasePanel, run `C:\Program Files\SensorPanel\Uninstall.ps1` as administrator. Profiles, the shared PawnIO installation, and any AIDA64 task backup are preserved.

## Editor and profiles

Right-click the panel or its tray icon to open the editor or exit. **Exit** closes both processes until the next sign-in or manual launch.

Select a widget in the list or preview, drag it to move it, and drag its lower-right corner to resize it. **Apply properties** applies the fields; **Save** writes the profile. **Import** and **Export** use versioned JSON. Higher layers are drawn last.

Widgets include text, numeric values, bars, circular gauges, images, line charts, rectangles, and clocks. Configure the sensor, label, unit, decimals, colors, font, alignment, and scale. Supported conversions include °C/°F, GHz/MHz, GiB/MiB/GB, and W/mW. Chart history is limited to ten minutes. Images are limited to 4096 × 4096 pixels, with at most four decoded images cached.

Profiles live at `%LOCALAPPDATA%\SensorPanel\profile.json`. The previous valid file is backed up as `.json.bak`. If neither file is valid, a temporary default is used without silently overwriting them. The base canvas scales proportionally to the display. Title tokens include `{cpu_name}`, `{gpu_name}`, and `{memory_name}`. Existing custom profile labels are preserved; newly created default profiles use English.

Missing readings display `—`. Stale readings retain their last value, appear dimmed, and carry `*`. Zero RPM is valid. Bindings include device identity: replacing hardware preserves the layout and requires reassignment of the affected sensors.

The bottom 150 pixels contain a persistent navigation strip and game FPS. **Resumen** marks the current view; **Gráficas** and **Accesos** reserve positions for future views and remain disabled. **Ajustes** opens the existing editor. Footer labels are Spanish. Existing profiles gain this strip only when the entire bottom area is unused; the widgets and bindings are preserved. Set `footer.enabled` to `false` in an exported JSON profile to hide it.

**Ajustes → Resumen** controls semantic colors, critical card borders, bar micro-labels, AI reset timers, network traffic, footer visibility, and its three modes. Changes preview on the next one-second tick; **Save** persists them, and closing without saving restores the last saved profile. **Estilo minimalista** disables all dashboard additions and selects the original game-only footer, retaining your thresholds and widget layout. **Diseño** opens the existing widget/display editor.

Profiles have an additive `dashboard` object with `#[serde(default)]` compatibility. All five booleans (`enable_semantic_colors`, `enable_critical_pulse`, `show_bar_micro_labels`, `show_ai_reset_timer`, `show_network_in_header`) default to `true`. `bottom_bar_mode` accepts `game_and_power` (default), `game_only`, or `total_watts_only`. Partial dashboard/threshold objects preserve omitted defaults. New profiles are written automatically; damaged existing profiles are still protected from automatic replacement.

`dashboard.thresholds` contains warning/critical pairs: CPU temperature **75/85 °C**, GPU temperature **70/82 °C**, GPU memory temperature **85/95 °C**, and memory/load usage **80/92%**. The percentage pair also applies to CPU/GPU load bars. Values below warning remain white, warning is amber (`#F59E0B`), and critical is coral (`#EF4444`). The critical option draws a steady, subtle 1 px border at 60% opacity, without adding animation or a faster timer. Turning semantic colors off also suppresses colored borders and the blue navigation accent. Thresholds are Celsius even when widgets display Fahrenheit; unavailable or stale readings do not trigger alerts.

The network header shows decimal **MB/s** across active physical adapters; virtual/VPN and loopback interfaces are excluded to avoid counting traffic twice. The first sample or a query failure shows dashes. Disabling the network option stops its queries. The power footer reads the active Windows power scheme without changing it. Watts require fresh CPU **and** GPU power readings and represent their sum, not whole-system or wall power; missing readings show `— W`. `game_and_power` shows the game and FPS while presenting, otherwise a power-plan pill; `total_watts_only` always shows the combined watts. FPS capture errors remain visible below the power pill.

An external AI producer may optionally write `Claude5HResetAt` and `Codex5HResetAt` as **REG_SZ Unix timestamps in seconds (UTC)** under the existing `HKCU\Software\FinalWire\AIDA64\ImportValues` key. For example, a producer with an actual reset time can write `[string]$reset.ToUnixTimeSeconds()`. The panel carries this as optional `Sensor.reset_at_ms` metadata, refreshes it with the existing five-second Registry input, and shows remaining hours/minutes only for a future reset within five hours. Missing, invalid, or expired timestamps hide the timer; the panel does not infer a reset from the usage percentage. Existing producer scripts remain compatible.

FPS and mean frame time come from the pinned PresentMon 2.3.1 console application, installed beside CasePanel. These are **rendered/presented frames**, not display refresh rate or a guarantee of counting generated frames. Capture follows a foreground graphics application, excludes common browsers, launchers and desktop processes, and retains the last selected application while it continues presenting. With multiple swap chains, it uses the busiest one instead of adding their rates. It displays a one-second rolling mean, updated once per second; no game shows **Sin juego**, and a capture error shows **FPS no disponibles**. The helper uses ETW without an overlay or input tracking, runs with the elevated collector, and stops when the panel's display is inactive. History and restart attempts are bounded.

## Initial hardware support

| Provider | Readings and limits |
|---|---|
| Windows | CPU usage, used/available RAM, clock, startup hardware identification, physical-adapter network rates, and active power-scheme name |
| NVIDIA NVML | GPU temperature, utilization, clock, power, global VRAM, and reported fan RPM; availability depends on the installed driver |
| Ryzen 9 9900X, family `1A`, model `44` | Signed AMDFamily17 module: Tctl/Tdie, maximum clock across queried cores, and package power from the energy counter |
| X870 AORUS ELITE WIFI7 | Primary IT8696E and secondary IT87952E; CPU fan is primary channel 1, System Fan #4 is secondary channel 3 |
| Validated NVIDIA Blackwell GPU | Memory temperature through NVML field 82 when supported, otherwise the signed Nvidia module; the fallback reports the maximum valid memory-channel temperature |
| External AI usage | Claude `DW1` and Codex `DW3` in `HKCU\Software\FinalWire\AIDA64\ImportValues`, with associated status strings |

The default GPU memory-temperature value is **not a hotspot reading**. Its label reflects the actual metric.

AI widgets require a separate producer. The personal producer used during validation and its authentication are not distributed. The panel only reads reported Registry values and does not authenticate to AI services. A percentage alone does not establish that a provider query recently succeeded. Existing `AIDA64 AI Usage Monitor` tasks and scripts are left in place; AIDA64 itself is not required.

Hardware access is limited to monitoring. Necessary register-index writes do not change fan curves, clocks, or voltages. Unsupported CPUs and boards are not probed using model-specific register sequences. Only official signed PawnIO modules are loaded, with the reference implementation's bus synchronization. Process separation protects the panel from collector failures; it cannot guarantee that a kernel driver will never fail.

## Recovery and resource limits

The local pipe uses a versioned, size-limited protocol and an ACL for the current user and SYSTEM. It accepts subscription state and shutdown requests, not arbitrary hardware commands. A ten-second query watchdog terminates only the collector. The panel can request up to three collector-task restarts per session, spaced one minute apart, while the display is active.

Updates are manual. There are no background version checks or recurring update prompts. Logs and chart history are bounded. Normal sampling is once per second, fan readings every two seconds, and Registry input every five seconds.

Measured results and limitations are documented in [VALIDATION.md](docs/VALIDATION.md). The initial two-process sample averaged **58.29 MiB private committed memory** and **0.0073% of machine CPU**. Including the existing external AI producer, a separate sample averaged **139.82 MiB**. Private committed memory and resident memory are different metrics. These measurements do not establish a Windows boot-time improvement or predict every workload.

A reported loss of clicks while Fortnite was running remains unresolved. In a later occurrence it persisted with both CasePanel processes closed and recovered after Ctrl+Alt+Delete followed by Escape. Neither a permanent fix nor a responsible component has been established. See the validation report before treating this preview as fully validated for gaming sessions.

## Build from source

Requirements: **Rust 1.100.0**, `x86_64-pc-windows-msvc`, Visual Studio C++ build tools, and a Windows SDK. The CRT is linked statically. `Cargo.lock` fixes dependency versions.

```powershell
git clone https://github.com/todociber/casepanel.git
cd casepanel
.\scripts\Prepare-Modules.ps1
.\scripts\Prepare-PresentMon.ps1
.\scripts\dev.ps1 build --release --locked
```

The development wrapper can use an installed Visual Studio toolchain or the private tools in the development workspace. It does not install tools on an end user's PC. `Prepare-Modules.ps1` downloads the official PawnIO module release, verifies its fixed SHA-256, and extracts the three required modules. It does not install the PawnIO driver. To collect low-level readings, install the official PawnIO 2.2.0 distribution or use the setup script in a release package.

```powershell
.\scripts\dev.ps1 fmt --check
.\scripts\dev.ps1 test --locked
.\scripts\dev.ps1 -Strict clippy --locked --all-targets
.\target\release\sensorpanel.exe --preview
.\target\release\sensorpanel.exe --diagnose --samples 5 --out diagnostic.json
.\target\release\sensorpanel.exe --diagnose-fps --samples 15 --out fps.json
```

The full hardware diagnostic requires elevation. `scripts\Test-Hardware.ps1` temporarily stops installed collectors and AIDA64, then restores their prior running tasks. Avoid concurrent low-level monitoring tools when comparing readings. `Measure.ps1` measures selected processes; `Verify-Startup.ps1` records checks across distinct boots.

## Releases

`scripts\Package.ps1` builds the release assets in `dist`. It includes licenses, source, the verified official PawnIO installer, and the corresponding module source. The GitHub Actions workflow runs formatting, tests, Clippy, and packaging on Windows before publishing assets for a `v*` tag. Manual workflow runs build artifacts without publishing a release unless run against a version tag.

To publish a new version, update `Cargo.toml`, `app.manifest`, the release notes, and any version-specific documentation; commit, tag the matching version, and push the tag. Existing version tags should remain unchanged. Preview releases are marked as prereleases.

## License and roadmap

CasePanel is distributed under **MPL-2.0**. Third-party code and components retain their own terms; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

Stage 1.1 will review usage-window accuracy and optimize or replace the external AI producer. Authentication remains outside the privileged collector. Additional hardware providers should preserve stable identities and keep device-specific access behind explicit compatibility checks.
