# Estructura y contratos

## Procesos

`sensorpanel.exe --panel` mantiene la ventana, el icono de bandeja, el editor, las lecturas Win32 básicas y la entrada de IA. `--collector` mantiene NVML y PawnIO. Las bibliotecas de hardware solo se cargan en el recolector. `--diagnose` realiza una captura puntual a JSON para soporte.

El panel y el recolector comparten un ejecutable, pero no memoria de aplicación. Una consulta bloqueada solo activa la terminación del recolector. El panel conserva su reloj y sus lecturas básicas mientras espera la reconexión.

## Módulos

| Módulo | Responsabilidad |
|---|---|
| `config` | Perfil versionado, límites de entrada, plantilla y escritura con respaldo |
| `model` | Catálogo, muestras, calidad, asociaciones e historial acotado |
| `platform` | Interoperabilidad Win32 básica, identidad de usuario, DLL y mutex |
| `sensors/basic` | CPU/RAM Win32 y Registro externo |
| `sensors/nvml` | Consultas NVIDIA por UUID, sin órdenes de modificación |
| `sensors/pawnio` | Acceso acotado a los módulos oficiales y hardware admitido |
| `ipc` | Tubería protegida, validación de tramas, supervisión y watchdog |
| `ui/monitors` | Pantallas activas e identidad persistente |
| `ui/render` | Dibujo Direct2D/DirectWrite, imágenes y conversión de unidades |
| `ui` | Ventanas y controles nativos del editor |

## Lecturas y protocolo

`Sensor.id` incluye identidad física y métrica. `Sensor.key` identifica su función lógica, por ejemplo `cpu.temperature`. El panel resuelve las asociaciones guardadas antes de dibujar; no cambia de dispositivo automáticamente cuando desaparece uno previamente asociado.

Cada sensor incluye valor opcional, unidad, intervalo nominal, instante de muestra, calidad y mensaje. Las calidades son `valid`, `reported`, `unavailable`, `error` y `stale`. Una lectura ausente de un sensor conocido conserva el último valor como antiguo. Cero es un valor válido, especialmente para ventiladores detenidos.

Las tramas JSON terminan en salto de línea, tienen versión 1 y un límite de 256 KiB. Los snapshots transportan catálogo y muestras. El cliente solo puede activar o pausar la suscripción y pedir el cierre. No existe un mensaje de escritura de registros o carga de código.

La tubería admite al SID del usuario y SYSTEM, rechaza clientes remotos y limita el catálogo. El recolector utiliza la primera instancia de la tubería para evitar procesos duplicados. Los perfiles y rutas de imágenes pertenecen exclusivamente al panel sin privilegios.

## Añadir hardware

Crear un adaptador en `sensors` que produzca `Sensor` con identidades persistentes y unidades explícitas, incorporarlo al catálogo del recolector y añadir fixtures para su decodificación. Para operaciones de bajo nivel, reconocer primero modelo/chip y utilizar exclusivamente módulos firmados publicados. Conservar las exclusiones y los mutex de la implementación de referencia; no añadir control de hardware a la interfaz de lectura.

Una nueva GPU accesible por NVML puede asociarse desde el editor. El módulo de temperatura de memoria y el acceso AMD/ITE tienen una lista inicial más estrecha y necesitan validación específica antes de ampliarse.

Cambios incompatibles del perfil requieren incrementar `schema_version` e introducir migración explícita. Cambios incompatibles de IPC requieren incrementar `PROTOCOL_VERSION`; las versiones desconocidas se rechazan.

## Recuperación

En este equipo Windows no repitió automáticamente una tarea tras terminar el proceso, aunque estuviera configurado `RestartOnFailure`. Por ello el panel solicita la ejecución de la tarea del recolector después de 60 segundos desconectado, como máximo tres veces por sesión. Invoca únicamente `schtasks.exe /Run /TN \SensorPanel\Collector`, sin intérprete de órdenes, argumentos del perfil ni UAC diario. El proceso auxiliar solo existe durante la solicitud de recuperación.

La tarea del recolector no tiene reintentos adicionales, evitando duplicar presupuestos. «Salir» transmite el cierre voluntario y termina el cliente, por lo que no inicia recuperación. La política de reintentos se prueba con reloj lógico y con la terminación real del recolector instalado.

## Recursos y permisos

El dibujo por software a 1 Hz evita las reservas persistentes del controlador gráfico observadas con el renderizado acelerado en la integrada AMD. Se conservan formatos de texto, pinceles e imágenes; se reconstruyen recursos de dibujo al cambiar tamaño o perder el destino.

La ventana del panel utiliza `HWND_BOTTOM` y `SWP_NOACTIVATE` al iniciar o cambiar de pantalla: queda detrás de las aplicaciones y conserva el foco del usuario. No utiliza el estado «siempre encima». El editor mantiene el comportamiento de una ventana normal.

Los ejecutables instalados están en Program Files con acceso de escritura para administradores y SYSTEM. La elevación se limita al recolector y a la instalación. Las credenciales de IA y su renovación permanecen fuera de estos procesos.
