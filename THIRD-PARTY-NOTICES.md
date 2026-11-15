# Componentes y atribuciones

## LibreHardwareMonitor

Las secuencias de acceso y decodificación de AMD Zen 5 e ITE en `src/sensors/pawnio.rs` se adaptaron de LibreHardwareMonitor, versión v0.9.6: Amd17Cpu.cs, IT87XX.cs, LpcIO.cs, LpcIO de PawnIo, Mutexes.cs y SuperIOHardware.cs.

Copyright LibreHardwareMonitor and Contributors; portions Copyright Michael Möller and Contributors. Mozilla Public License 2.0. El código adaptado conserva esta licencia. Referencias y texto de licencia en `vendor/reference`. Proyecto: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor

## PawnIO y módulos

Controlador oficial PawnIO 2.2.0, distribuido sin modificaciones por namazso: https://pawnio.eu/ y https://github.com/namazso/PawnIO.Setup/releases/tag/2.2.0

Instalador SHA-256: `1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032`.

Módulos oficiales firmados, release 0.2.11: https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.11

Archivo de distribución SHA-256: `43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4`.

La aplicación incorpora sin modificaciones solamente AMDFamily17.bin, LpcIO.bin y Nvidia.bin. Los módulos se distribuyen bajo LGPL 2.1; su texto se incluye en `vendor/pawnio/modules/COPYING`. Código fuente y versiones: https://github.com/namazso/PawnIO.Modules/tree/0.2.11 . La distribución oficial firmada de PawnIO tiene sus propios términos; su instalador se incluye sin modificaciones. PawnIOLib se carga dinámicamente desde la instalación oficial.

Para preparar una copia del repositorio, `scripts/Prepare-Modules.ps1` obtiene los tres binarios desde la distribución oficial fijada y verifica el archivo antes de extraerlos. Los binarios descargados y el instalador quedan fuera del historial Git. Se conservan los avisos de licencia y los enlaces al código fuente de sus autores.

Código fuente del controlador: https://github.com/namazso/PawnIO (GPL 2 con excepción). Código fuente de PawnIOLib en ese repositorio (LGPL 2.1). Condiciones de integración: https://github.com/namazso/PawnIO.Modules/wiki/Using-PawnIO-Modules

## NVIDIA

NVML se carga desde el controlador gráfico instalado. No se distribuyen DLL de NVIDIA. Referencia de interfaces: https://docs.nvidia.com/deploy/nvml-api/ . El encabezado de referencia descargado desde NVIDIA/go-nvml conserva sus avisos originales y se mantiene como referencia local fuera de Git; no se incorpora como código compilado.

## Rust y crates

`windows` y `windows-numerics`: Microsoft, MIT/Apache-2.0. `serde` y `serde_json`: MIT/Apache-2.0. `sha2`: MIT/Apache-2.0. Las versiones exactas y dependencias transitivas constan en Cargo.lock. Las fuentes y licencias pueden obtenerse de crates.io y los repositorios indicados en sus manifiestos.
