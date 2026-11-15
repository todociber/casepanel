# Validación de la primera entrega

Equipo de prueba: Windows 11 x64, Ryzen 9 9900X, RTX 5080, X870 AORUS ELITE WIFI7 y PM1562 de 1920 × 1200. Fecha: 14 de noviembre de 2026, hora de El Salvador.

## Comprobaciones realizadas

- Compilación Release con CRT estático; dependencias importadas limitadas a DLL de Windows. NVML y PawnIOLib se cargan desde sus instalaciones nativas.
- 18 pruebas automatizadas aprobadas y Clippy con advertencias como errores. Cubren perfiles dañados y respaldo, versiones, límites, datos antiguos, cero RPM, historial acotado, identidad de hardware, selección ambigua de pantalla, decodificación AMD/ITE y presupuesto de recuperación.
- Diagnóstico elevado real, con AIDA64 cerrado durante la captura. Se obtuvieron lecturas válidas de temperatura, frecuencia y potencia CPU; ventiladores CPU y SYS4; temperatura, utilización, frecuencia, potencia, RPM y VRAM NVIDIA; temperatura de memoria mediante el módulo Nvidia firmado. Cero RPM de GPU se conservó como lectura válida.
- Editor nativo abierto desde el menú contextual; cambio de posición, aplicación y guardado de JSON verificados. Cierre del editor y opción «Salir» comprobados en la vista previa. La vista previa no sustituye una comparación visual de cada widget personalizado.
- Terminación real del recolector instalado con código 70: recuperación en 61,1 segundos y mismo proceso del panel durante la interrupción. Windows no ejecutó sus reintentos de tarea en esta compilación; la supervisión acotada del panel resolvió la recuperación.
- Primer dibujo registrado en 38 ms desde la entrada en la función de interfaz, en una ejecución con cachés calientes. Este cronómetro no mide carga del ejecutable, demora de tarea ni arranque de Windows.

Los archivos de evidencia locales están en `artifacts`: `diagnostic-hardware.json`, `recovery-test.json` y los informes de rendimiento. No se incluyen en el paquete distribuible porque contienen identidades de este equipo.

## Rendimiento

Medición inicial completada: 600 muestras durante 615,1 segundos, editor cerrado, plantilla predeterminada y ambos procesos instalados. CPU normalizada entre 24 procesadores lógicos.

| Métrica conjunta del panel y recolector | Resultado inicial |
|---|---:|
| CPU media de la máquina | 0,0073 % |
| Memoria privada comprometida media | 58,29 MiB |
| Memoria privada comprometida mínima / máxima | 56,71 / 58,77 MiB |
| Suma de memoria residente media | 79,92 MiB |

La memoria privada comprometida y la memoria residente son métricas diferentes. La suma de memoria residente puede contar páginas compartidas más de una vez. Estas cifras corresponden a los dos procesos nativos; el productor PowerShell de IA existente se conserva como dependencia externa de la configuración actual y se evalúa por separado. El presupuesto de 60 MiB se cumple en los procesos nativos, pero no en el conjunto que incluye ese productor heredado. Reducir este último corresponde a la etapa 1.1 acordada.

Se completó además otra muestra de 600 intervalos durante 617,1 segundos, incluyendo el productor real de IA, con actividad habitual del escritorio. No es una carga CPU/GPU controlada.

| Proceso | CPU media de la máquina | Memoria privada comprometida media |
|---|---:|---:|
| Panel nativo | 0,0091 % | 31,43 MiB |
| Recolector nativo | 0,0056 % | 27,50 MiB |
| Productor PowerShell de IA existente | 0,0026 % | 80,88 MiB |
| **Conjunto completo** | **0,0173 %** | **139,82 MiB** |

El conjunto tuvo un mínimo de 138,29 MiB y un máximo de 140,66 MiB privados comprometidos. La suma residente media fue 122,76 MiB en esta segunda muestra; el conjunto de páginas residentes cambia con la presión de memoria del sistema. Evidencia: `artifacts/performance-with-ai-producer.json`.

Las mediciones largas corresponden a la compilación anterior a la corrección final del despacho del menú contextual. La corrección solo cambia cómo se entrega la orden del menú a la ventana; después se repitieron las 18 pruebas, Clippy y la prueba real de abrir el editor y salir. También se compiló y probó el código fuente incluido en el paquete como proyecto independiente. No se repitió el ensayo largo tras ese ajuste de interacción.

La muestra previa de AIDA64 duró diez segundos y registró aproximadamente 0,35 % CPU, 51 MiB residentes y 121 MiB privados comprometidos. Las duraciones y condiciones distintas impiden tratarla como comparación controlada o atribuir una mejora al inicio de Windows.

## Pruebas pendientes en uso real

1. Tres reinicios físicos: iniciar sesión sin abrir manualmente la aplicación y ejecutar `scripts\Verify-Startup.ps1` desde el paquete para registrar cada arranque. No contar una ejecución manual de las tareas como evidencia de inicio automático.
2. Suspender y reanudar; desconectar y reconectar el PM1562; comprobar cambios de DPI y ausencia de la pantalla.
3. Comparación separada frente a AIDA64 bajo carga CPU/GPU reproducible, verificando unidades y la correspondencia física de ambos ventiladores. La lectura obtenida y la coincidencia con los sensores configurados no equivalen a una calibración independiente.
4. Diez minutos por escenario bajo actividad GPU sostenida y una sesión de varias horas para descartar crecimiento continuo de memoria. La muestra normal no demuestra estabilidad durante días.
5. Medición equivalente del tiempo hasta un escritorio utilizable antes de afirmar que mejoró el arranque de Windows.

El inicio de AIDA64 quedó deshabilitado después de superar el diagnóstico, la recuperación y el presupuesto de la muestra inicial. Su aplicación, configuración y XML de tarea se conservaron. El productor de IA continúa sin cambios; la revisión de ventanas de uso y autenticación corresponde a la etapa 1.1.

## Incidencia de entrada durante Fortnite

Durante una sesión se reportó pérdida de clics fuera de Fortnite. Una primera mejora coincidió con reiniciar SensorPanel; pruebas breves de panel solo, recolector solo y ambos juntos no reprodujeron el problema. En una segunda aparición el bloqueo continuó con ambos procesos de SensorPanel completamente cerrados. Alt+Tab no lo resolvió; Ctrl+Alt+Supr seguido de Esc recuperó los clics. Las ventanas seguían actualizándose visualmente y no se observó captura de ratón por SensorPanel en las lecturas realizadas.

La causa exacta permanece sin confirmar. Esta incidencia no debe presentarse como corregida ni atribuirse de forma concluyente a un componente. Los registros del equipo se conservan fuera del repositorio público, en `artifacts`.
