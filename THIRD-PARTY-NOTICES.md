# Third-party components and notices

## LibreHardwareMonitor

AMD Zen 5 and ITE access/decoding sequences in `src/sensors/pawnio.rs` were adapted from LibreHardwareMonitor v0.9.6: Amd17Cpu.cs, IT87XX.cs, LpcIO.cs, PawnIo LpcIO, Mutexes.cs, and SuperIOHardware.cs.

Copyright LibreHardwareMonitor and Contributors; portions Copyright Michael Möller and Contributors. Mozilla Public License 2.0. Adapted code retains this license. Reference files and license text are in `vendor/reference`.

Project: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor

## PawnIO and modules

Official PawnIO 2.2.0 installer, redistributed without modification by permission of its publisher's distribution terms:

- https://pawnio.eu/
- https://github.com/namazso/PawnIO.Setup/releases/tag/2.2.0
- Installer SHA-256: `1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032`

Only the unmodified signed `AMDFamily17.bin`, `LpcIO.bin`, and `Nvidia.bin` modules from release 0.2.11 are embedded. Module distribution SHA-256: `43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4`.

The modules use LGPL 2.1; its text is included as `vendor/pawnio/modules/COPYING` and in release licenses. Corresponding module source is included in release packages as the unmodified upstream source archive, pinned to commit `52a7e536dff3e53c96917a28caac5e0fa6510696`. CasePanel source is included so the application can be rebuilt with replacement module bytes. The official driver enforces its own module-signature policy.

The repository's preparation script retrieves the pinned official distribution and verifies it before extraction. Downloaded binaries and installers remain outside Git. PawnIOLib loads dynamically from the official installation. The signed official binary edition has its own terms; the open-source driver and library have separate licenses.

- Modules and source: https://github.com/namazso/PawnIO.Modules/tree/0.2.11
- Driver source: https://github.com/namazso/PawnIO (GPL 2 with exception)
- PawnIOLib source: the same repository (LGPL 2.1)
- Integration and distribution information: https://github.com/namazso/PawnIO.Modules/wiki/Using-PawnIO-Modules

## NVIDIA

NVML loads from the installed NVIDIA display driver. NVIDIA DLLs are not redistributed. Interface reference: https://docs.nvidia.com/deploy/nvml-api/ . A locally downloaded NVIDIA/go-nvml reference header retains its original notices, stays outside Git, and is not compiled into the application.

## Rust crates

`windows` and `windows-numerics`: Microsoft, MIT/Apache-2.0. `serde`, `serde_json`, and `sha2`: MIT/Apache-2.0. Exact versions and transitive dependencies are fixed in Cargo.lock. Release packages include available license/notice files from the resolved crates. Their source and manifests are available from crates.io and the upstream repositories identified in those manifests.
