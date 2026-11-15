# CasePanel

A native Windows hardware monitoring panel built with Rust.

Primera entrega pública: **0.1.0**. El ejecutable y las tareas de Windows conservan el nombre `SensorPanel`. El soporte de hardware inicial y las comprobaciones pendientes están descritos más abajo; esta entrega todavía requiere validación prolongada en otros equipos.

Panel para Windows 11 x64 desarrollado en Rust, Win32, Direct2D y DirectWrite. Direct2D dibuja por software a 1 Hz para reducir la memoria reservada por los controladores gráficos. No carga un navegador ni un runtime .NET. Un mismo ejecutable funciona como panel sin privilegios o como recolector separado.

## Uso

La instalación normal utiliza `C:\Program Files\SensorPanel\sensorpanel.exe`. El panel aparece en el PM1562 al abrir sesión, detrás de las otras ventanas y sin tomar el foco. Puedes utilizar aplicaciones en esa pantalla y dejar que cubran el panel normalmente. Clic derecho sobre el panel o sobre su icono en la bandeja permite abrir el editor o salir. «Salir» cierra también el recolector hasta el próximo inicio de sesión.

El editor permite seleccionar widgets en la lista o en la vista previa, arrastrarlos y redimensionarlos desde la esquina inferior derecha. Las propiedades se aplican con **Aplicar propiedades**. **Guardar** persiste el perfil; **Importar** y **Exportar** utilizan archivos JSON. Las capas superiores se dibujan al final.

Widgets: texto, lectura numérica, barra, medidor circular, imagen, gráfico de líneas, rectángulo y reloj. Las unidades admiten conversiones °C/°F, GHz/MHz, GiB/MiB/GB y W/mW; los límites de escala se expresan en la unidad elegida. Los gráficos conservan como máximo diez minutos en RAM. Los valores ausentes muestran `—`; una lectura antigua se atenúa y lleva `*`. Los porcentajes de IA son valores reportados por un productor externo: el panel no confirma la antigüedad real de cada consulta.

El perfil se guarda en `%LOCALAPPDATA%\SensorPanel\profile.json`; el respaldo válido anterior utiliza `.json.bak`. Si ambos fallan, se usa una plantilla temporal sin sobrescribirlos automáticamente. La escala base es 1920 × 1200, conservando proporciones con otras resoluciones. El panel se oculta cuando falta la pantalla seleccionada.

Los títulos de la plantilla admiten `{cpu_name}`, `{gpu_name}` y `{memory_name}`. Las imágenes se leen desde rutas locales, hasta 4096 × 4096; se conservan como máximo cuatro imágenes decodificadas. Los perfiles personalizados pueden consumir más memoria que la plantilla.

## Instalación

Desde una PowerShell **como administrador**, dentro del proyecto o del paquete Release:

```powershell
.\scripts\Install.ps1
Start-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Collector'
Start-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Panel'
```

El instalador copia el ejecutable a una carpeta protegida y registra dos tareas con el usuario actual: panel con privilegios limitados y recolector con privilegios elevados. Se ejecutan cinco segundos después de iniciar sesión. El panel supervisa la conexión y solicita hasta tres reinicios de la tarea del recolector por sesión, separados por un minuto; no solicita reinicios mientras la pantalla está ausente o apagada. La tarea del panel también tiene tres reintentos configurados en Windows. No tienen límite de tiempo de ejecución.

PawnIO 2.2.0 debe estar instalado. El paquete incluye el instalador oficial, verificado por SHA-256 y firma Authenticode. `scripts\Prepare-PawnIO.ps1` permite obtenerlo de su distribución oficial si falta. Se utiliza la edición firmada, conservando las protecciones de Windows.

Opciones de instalación:

```powershell
# Registrar las tareas deshabilitadas para una revisión previa.
.\scripts\Install.ps1 -StageOnly
# Cambiar la demora y deshabilitar el inicio anterior de AIDA64.
.\scripts\Install.ps1 -DelaySeconds 5 -DisableAidaAutostart
```

La tarea `AIDA64 AI Usage Monitor` y el script `C:\Tools\aida64-ai-usage.ps1` se conservan. El instalador solo modifica `AIDA64 AutoStart` cuando se indica `-DisableAidaAutostart`; antes guarda su XML en la carpeta de instalación.

Para retirar la aplicación, ejecutar `C:\Program Files\SensorPanel\Uninstall.ps1` como administrador. Conserva perfiles, PawnIO y el respaldo de AIDA64. Para volver al inicio anterior, habilitar y arrancar `AIDA64 AutoStart` desde el Programador de tareas.

## Sensores y límites

- Windows: carga CPU, RAM usada/disponible y reloj. Información SMBIOS al iniciar.
- NVIDIA: NVML del controlador instalado, cargada por ruta absoluta. Carga, temperatura, frecuencia, consumo, VRAM global y RPM reportadas. La disponibilidad depende del controlador.
- Ryzen 9 9900X, familia `1A`, modelo `44`: módulo firmado AMDFamily17; temperatura Tctl/Tdie, frecuencia máxima de los núcleos consultados y potencia del paquete calculada a partir del contador de energía.
- Gigabyte X870 AORUS ELITE WIFI7: ITE IT8696E primario, IT87952E secundario. CPU Fan: canal 1 primario. System Fan #4: canal 3 secundario. El selector del editor permite asociar estos sensores.
- Memoria NVIDIA: campo NVML 82 cuando está disponible; módulo firmado Nvidia de PawnIO en la familia Blackwell validada como alternativa. El valor alternativo representa la temperatura máxima entre canales de memoria válidos.
- IA: lectura de `DW1` (Claude) y `DW3` (Codex) en `HKCU\Software\FinalWire\AIDA64\ImportValues`. Se consultan también sus textos de estado. No se leen credenciales ni se efectúan llamadas a servicios de IA desde el panel o el recolector.

El antiguo rótulo «GPU HOTSPOT» se reemplazó por **MEMORIA GPU**, conforme al sensor real de la configuración de AIDA64.

El recolector utiliza únicamente módulos firmados y secuencias de lectura; las escrituras de índices necesarias no cambian PWM, voltajes ni frecuencias. Respeta los mutex de bus utilizados por LibreHardwareMonitor. No aplica registros específicos a CPU o placas desconocidas. La separación de procesos protege la interfaz frente a fallos del recolector; ningún diseño en modo usuario puede garantizar que un controlador de kernel nunca falle.

Las asociaciones guardan la identidad del dispositivo. Un cambio de hardware conserva el diseño y requiere reasignar el sensor correspondiente. El protocolo IPC tiene versión, tamaño acotado y ACL para el usuario y SYSTEM; no acepta registros, módulos, rutas ni órdenes de control de hardware. Un watchdog termina solo el recolector si una consulta supera diez segundos.

## Desarrollo y diagnóstico

Rust 1.100.0, objetivo `x86_64-pc-windows-msvc`, herramientas de enlace MSVC y Windows SDK. Puede usarse una Developer PowerShell de Visual Studio; `scripts\dev.ps1` reconoce también las herramientas locales de este workspace. `Cargo.lock` fija las dependencias. El CRT se enlaza estáticamente.

Para compilar desde una copia nueva del repositorio, usar una Developer PowerShell con las herramientas C++ de Visual Studio y Windows SDK:

```powershell
git clone https://github.com/todociber/casepanel.git
cd casepanel
.\scripts\Prepare-Modules.ps1
cargo build --release --locked
```

`Prepare-Modules.ps1` descarga la distribución oficial 0.2.11 de los módulos PawnIO, verifica su SHA-256 fijado y extrae los tres módulos utilizados. Se preparan al compilar y quedan fuera de Git. Para ejecutar el recolector también se necesita instalar PawnIO 2.2.0 mediante el instalador oficial; preparar los módulos no instala el controlador.

Las barras de IA requieren un productor externo que publique los valores documentados del Registro. El script personal utilizado en la validación y sus credenciales permanecen fuera del repositorio; sin productor, esas barras muestran valores no disponibles.

El paquete de ejecución se genera en `dist`; `.tools` contiene los compiladores utilizados durante el desarrollo. Los contratos y puntos de extensión están descritos en `docs\ARCHITECTURE.md`.
Los resultados medidos y las pruebas pendientes constan en `docs\VALIDATION.md`. Las órdenes de compilación siguientes se ejecutan desde el proyecto; el paquete incluye su código en `source` y permite compilar con `cargo build --release` desde una Developer PowerShell.

```powershell
.\scripts\dev.ps1 test --offline
.\scripts\dev.ps1 -Strict clippy --offline --all-targets
.\scripts\dev.ps1 build --release --offline
.\target\release\sensorpanel.exe --preview
.\target\release\sensorpanel.exe --diagnose --samples 5 --out diagnostic.json
```

El diagnóstico completo requiere elevación. `scripts\Test-Hardware.ps1` realiza una prueba corta con AIDA64 cerrado y lo restaura al terminar. Ejecútelo como administrador. No ejecute simultáneamente dos herramientas que accedan a los chips de placa durante una comparación.

`scripts\Measure.ps1` mide CPU normalizada, memoria residente y memoria privada de procesos específicos. Su propia carga pertenece a otro proceso. `scripts\Verify-Startup.ps1` registra las verificaciones de arranque y distingue reinicios reales de repetir la comprobación en la misma sesión.

Los objetivos de rendimiento se evalúan con el editor cerrado y la plantilla predeterminada: CPU media inferior al 0,2 %, memoria privada combinada inferior a 60 MiB y primer dibujo en menos de un segundo. Son objetivos de prueba; no se promete una mejora en el arranque de Windows sin una comparación equivalente. El productor PowerShell de IA existente se mide por separado: el presupuesto de memoria se cumple en los procesos nativos, pero se supera al contar ese productor heredado. Su optimización corresponde a la etapa 1.1.

La etapa 1.1 queda reservada para revisar las ventanas de consumo y optimizar el productor de IA. La autenticación permanecerá separada de la adquisición privilegiada de hardware.
