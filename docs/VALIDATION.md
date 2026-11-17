# Initial validation report

Test PC: Windows 11 x64, Ryzen 9 9900X, RTX 5080, X870 AORUS ELITE WIFI7, and a 1920 × 1200 PM1562. Original validation date: November 14, 2026, El Salvador time.

## Completed checks

- Release build with a statically linked CRT and Windows-only imported DLLs. NVML and PawnIOLib load from their native installations.
- 18 automated tests passed, with Clippy warnings treated as errors. Coverage includes damaged-profile recovery, protocol/profile versions, limits, stale values, valid zero RPM, bounded history, device identity, ambiguous monitor selection, AMD/ITE decoding, and restart budgets.
- An elevated hardware diagnostic ran with AIDA64 closed. It returned valid CPU temperature, clock, and power; CPU and SYS4 fan readings; NVIDIA temperature, load, clock, power, RPM, VRAM, and memory temperature through the signed Nvidia module. Zero GPU fan RPM remained valid.
- The native editor opened through the context menu. A position change, property application, and JSON save were verified. Editor close and Exit were checked in preview. This does not validate every possible custom widget configuration.
- The installed collector was terminated with exit code 70. It recovered in 61.1 seconds while the panel retained its process. Native bounded supervision handled the recovery after Windows task retries failed to run on this build.
- A warm first draw was logged at 38 ms from entry into the UI function. This timer excludes executable loading, task delay, and Windows boot.

Local evidence lives in `artifacts`, including hardware diagnostics, recovery tests, and performance reports. It is excluded from public distributions because it contains machine identities.

## Footer and FPS update — November 15, 2026

- 24 tests passed and Clippy passed with warnings denied. New coverage checks real PresentMon CSV column casing and quoted names, invalid timings, foreground selection, dominant swap chains, expiry, bounded frame history, footer hit testing, stale FPS removal, and migration without changing widgets.
- Live PresentMon 2.3.1 capture returned valid rendered-frame readings for the already-running Triforce application, approximately 224–272 FPS in the 12-sample diagnostic (`artifacts/fps-diagnostic-live.json`). This is a functional capture, not a reproducible gaming benchmark.
- Native visual inspection showed the footer together with live CPU/GPU temperature, clock, power, fan and VRAM readings; the footer showed 261 FPS and 3.8 ms at one point. The Settings button opened the existing editor. Graphs and Shortcuts remain disabled, reserved slots.
- The installed executable hash matched the built executable. The profile migration preserved all existing widgets and bindings and enabled the previously unused bottom strip. The prior executable and profile were backed up under the installed `backups` directory; `artifacts/footer-installation.json` records the update.
- Both scheduled tasks and the PresentMon child were running after validation. One instantaneous private-memory sample was 29.86 MiB for the panel, 27.72 MiB for the collector, and 5.14 MiB for PresentMon (62.72 MiB combined). This is not an average or a repeat of the original resource benchmark; the earlier 60 MiB result does not cover the new helper.

## Performance

The first measurement used 600 samples over 615.1 seconds, with the editor closed, the default layout, and both installed processes. CPU is normalized across 24 logical processors.

| Panel and collector combined | Initial result |
|---|---:|
| Average machine CPU | 0.0073% |
| Average private committed memory | 58.29 MiB |
| Minimum / maximum private committed memory | 56.71 / 58.77 MiB |
| Average sum of working sets | 79.92 MiB |

Private committed memory differs from resident memory. Summed working sets may count shared pages more than once. The 60 MiB budget was met for the native processes, but not for the configuration including the legacy AI producer. Reducing that producer is deferred to stage 1.1.

A second measurement used 600 intervals over 617.1 seconds and included the existing AI producer during ordinary desktop use. This was not a controlled CPU/GPU stress test.

| Process | Average machine CPU | Average private committed memory |
|---|---:|---:|
| Native panel | 0.0091% | 31.43 MiB |
| Native collector | 0.0056% | 27.50 MiB |
| Existing PowerShell AI producer | 0.0026% | 80.88 MiB |
| **Full configuration** | **0.0173%** | **139.82 MiB** |

The full configuration ranged from 138.29 to 140.66 MiB private committed memory. Its average summed working set was 122.76 MiB in the second sample; resident pages vary with system memory pressure.

Long measurements predate the final context-menu dispatch correction and English UI changes. The menu fix only changed command delivery. Formatting, tests, Clippy, and source-package builds are repeated for releases; the long measurement was not repeated after every UI-only change. These figures should not be presented as a new benchmark of every release.

The earlier AIDA64 sample lasted ten seconds and recorded about 0.35% CPU, 51 MiB resident memory, and 121 MiB private committed memory. Different durations and conditions prevent treating this as a controlled comparison or claiming a Windows boot improvement.

## Outstanding real-world tests

1. Three physical reboots: sign in without manually launching the tasks, then run `scripts\Verify-Startup.ps1` to record each boot. A manual task start is not evidence of automatic startup.
2. Suspend/resume, PM1562 disconnect/reconnect, DPI changes, and an absent target display.
3. Separate comparisons with AIDA64 under reproducible CPU/GPU load, checking units and physical fan correspondence. Matching configured sensor names is not an independent calibration.
4. Ten-minute samples for sustained GPU activity and multi-hour sessions to investigate memory growth. Normal-use measurements do not establish days of stability.
5. Equivalent time-to-usable-desktop measurements before attributing any improvement to Windows startup.
6. Validation on hardware beyond the initial CPU, motherboard, and GPU configuration.

On the original PC, AIDA64 startup was disabled after the initial diagnostic, recovery, and performance checks. Its application, configuration, and task XML were retained. The AI producer was unchanged.

## Input incident during Fortnite

A session included lost clicks outside Fortnite. An initial improvement coincided with restarting CasePanel. Short tests of the panel alone, collector alone, and both together did not reproduce the issue. In a later occurrence the problem continued with both CasePanel processes completely closed. Alt+Tab did not restore clicks; Ctrl+Alt+Delete followed by Escape did. Windows continued updating visually, and the snapshots did not show CasePanel holding mouse capture.

The root cause remains unconfirmed. This incident must not be described as fixed or conclusively attributed to any component. Diagnostic records remain local under `artifacts`; they are not published.
