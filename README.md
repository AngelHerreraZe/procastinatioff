# ProcastinaciOFF

**Bloquea el uso indiscriminado de tus dispositivos hasta que cumples las metas que tú mismo te pones cada mañana.** Sin cuentas, sin servidores y sin grabar nunca nada: todo ocurre en tus dispositivos.

> *In English:* ProcastinaciOFF is an open-source, local-first self-commitment app for Android, iOS, Windows, macOS and Linux. It shields distracting apps and websites until you complete your daily goals, verified on-device (camera frames never leave RAM). There are no accounts and no servers: your devices sync directly over your local network or peer-to-peer. Optional family mode (guardians approve changes) and a "trusted contact" for adults.

> ⚠️ **Estado: en desarrollo, todavía no apto para uso diario.** El núcleo y las apps de Android y Windows compilan y tienen pruebas, pero aún no se han probado en dispositivos reales. Estamos migrando la sincronización a **red local / P2P**: el código actual todavía incluye `relay/` y `push-gateway/` de la arquitectura anterior, que se van a eliminar.

---

## Qué hace

1. **Por la mañana** abres el *plan del día*: tus metas fijas ya están ahí y añades las de hoy. Desde ese momento solo se pueden endurecer.
2. **Hasta cumplir tus metas "llave"**, el teléfono y el PC solo dejan usar lo esencial (llamadas, mensajes, lo que tú añadas: autenticador, mapas, app médica…) y unos pocos **permisos** de minutos.
3. **Cumples una meta** (leer, hacer ejercicio, estudiar…) y la app lo verifica **en el dispositivo**. Al cumplirlas todas, todo queda libre hasta la hora de dormir.
4. **Por la noche**, modo dormir: solo llamadas.

| Estado | Qué se permite |
|---|---|
| Dormir | Solo llamadas (y las de emergencia, siempre) |
| Bloqueado | Lo esencial + los permisos del día |
| Permiso | Todo durante unos minutos (el contador es compartido entre dispositivos) |
| Enfoque | Solo las apps y sitios de la meta en curso (p. ej. Word, Coursera y el aula virtual para "Estudiar") |
| Libre | Todo, hasta la hora de dormir |
| Emergencia | Todo durante 15 min, siempre disponible, con un costo y un registro |

**Anti-impulso:** relajar una regla (más permisos, quitar una meta, dormir más tarde) entra en vigor **mañana**; endurecer, al instante.

## Principios (no negociables)

- **Nada se graba.** Imágenes, audio, texto de OCR y transcripciones existen solo en RAM unos segundos. Se guardan resultados ("meta cumplida", minutos), nunca la evidencia.
- **El componente que usa la cámara no tiene acceso a Internet**, y lo impone el sistema operativo: APK aparte sin permiso `INTERNET` en Android, AppContainer sin red en Windows.
- **Sin servidores y sin registro.** No hay backend: tus datos viven en tus dispositivos (y en los de tu familia o tu contacto de confianza, si tú lo decides). Tu identidad es un par de claves generado en el dispositivo.
- **Cero telemetría, publicidad o SDKs de terceros.**
- **Nada de vigilancia:** sin keylogger, capturas de pantalla, lectura de mensajes, ubicación ni historial de navegación. Ni siquiera un tutor puede ver eso, porque no existe.
- **Código abierto** (MPL-2.0) y builds reproducibles.
- **Borrado total con un botón** (se destruyen las claves y todo lo cifrado queda ilegible).

## Cómo se sincroniza sin servidor

Tus dispositivos se vinculan **escaneando un QR** y comparando un código de 6 dígitos (protege contra intermediarios). Después:

| Canal | Cuándo | Cómo |
|---|---|---|
| **Red local** (por defecto) | Cuando están en la misma Wi-Fi, p. ej. en casa | Se encuentran por mDNS con un identificador que cambia cada día y se conectan directamente por QUIC. Solo se aceptan dispositivos vinculados. |
| **P2P por Internet** (opcional, apagado por defecto) | Fuera de casa | Conexión directa atravesando la NAT; si no se puede, retransmisión cifrada por un relay público o propio que no ve nada. |
| **Códigos sin red** | Para aprobaciones urgentes sin conexión | Quien pide dicta un código de 10 caracteres; quien aprueba responde con uno de 8 dígitos. |

Cada cambio es un **evento firmado** (Ed25519) y **cifrado** (`age`) en un historial de solo-añadir; todos los dispositivos calculan el mismo estado a partir de él. El PC (que tiene un servicio siempre encendido) hace de punto de encuentro: cuando el teléfono llega a casa, se intercambian lo que les falte. Cada dispositivo bloquea con su copia local, aunque esté sin red.

Las **notificaciones son locales** (alarmas, recordatorios, solicitudes recibidas al sincronizar). No usamos APNs ni FCM, porque ambos necesitan un servidor.

Como no hay nube, si pierdes todos tus dispositivos empiezas de cero. Puedes **exportar una copia cifrada** con una frase de recuperación y guardarla donde quieras.

## Quién puede cambiar las reglas

Cambiar la configuración, las rutinas, los horarios o las listas exige **uno de estos 5 métodos**:

| # | Método | Notas |
|---|---|---|
| 1 | **Contraseña**: PIN numérico (≥ 6 dígitos) o alfanumérica (≥ 8), a tu elección | Puedes ponerla tú o dársela a otra persona para que la guarde. Se guarda como hash Argon2id; los intentos fallidos imponen esperas crecientes. |
| 2 | **Biometría** (huella o rostro) | La verifica el sistema operativo; la app nunca ve tu huella. Si alguien registra una huella nueva, hay que volver a activarla. |
| 3 | **Padres o tutores** | Para perfiles de niños y adolescentes. Aprueban desde su teléfono o, en persona, con su propio PIN/patrón. |
| 4 | **Contacto de confianza** | Para adultos que lo elijan (ver abajo). |
| 5 | **Patrón** | Cuadrícula 3×3, al menos 4 puntos (recomendamos 6). |

Endurecer nunca necesita autorización, y el botón de emergencia tampoco.

### Contacto de confianza (adultos, opcional)

Eliges a una persona (hasta 3) que **tiene que aprobar** lo que relaje tus reglas. Instala la app en *modo contacto* (no le bloquea nada) y os vinculáis con un QR. Solo ve el cambio que pides ("+2 permisos de 10 min", "dormir a las 00:30") y, si quieres, avisos de emergencia o manipulación. Nunca ve tu historial, tus metas ni ninguna evidencia. Si un día quieres dejar de tenerlo y no lo aprueba, el cambio entra en vigor a los 7 días: nadie queda atrapado, y nadie lo quita en un impulso.

### Modo familiar (niños y adolescentes)

- Un perfil de menor **no puede existir sin un tutor**: se crea en persona, con el QR del teléfono del tutor.
- Todo lo que relaje reglas lleva la **firma del tutor**; el menor puede pedir (más tiempo, un sitio nuevo, un día de descanso) y siempre puede endurecer.
- **Panel espejo:** el menor ve exactamente lo mismo que ve el tutor. El tutor nunca ve mensajes, ubicación, imágenes ni historial de navegación (solo cuántos sitios se bloquearon).
- La web queda limitada a una lista blanca de sitios de estudio, con filtrado en capas (políticas del navegador + DNS local + red). En Android se recomienda configurar el teléfono como **Device Owner**.
- La cámara para verificar viene **apagada** en perfiles de menores.

## Verificación de metas (100 % local)

Toda meta tiene al menos una opción **sin cámara**. Ejemplos: sesión de enfoque en el PC, anclas QR pegadas en casa (gimnasio, escritorio), Health Connect / HealthKit, un testigo de confianza, o una declaración con fricción. Si eliges usar la cámara o el micrófono: conteo de repeticiones por pose, presencia lectora, número de página por OCR, foto de comida en vivo, resumen de lo leído evaluado por un LLM local. Los modelos van dentro de la app y se verifican por hash; nunca se descargan sin firma.

## Plataformas

| Plataforma | Bloqueo | Estado |
|---|---|---|
| Android | Accesibilidad + escudo; Device Owner para menores; VPN **local** para el filtro DNS | En desarrollo |
| Windows | Servicio `LocalSystem` + agente de sesión + políticas de navegador + DNS local | En desarrollo |
| iOS | Screen Time API (FamilyControls, ManagedSettings, DeviceActivity) | Inicio (requiere entitlement de Apple) |
| macOS | Endpoint Security + NetworkExtension | Planificado |
| Linux | systemd + fanotify + nftables | Planificado |

**Límite honesto:** en un dispositivo del que eres administrador total, ningún software es imbatible. El objetivo es que saltarse el bloqueo sea **lento, costoso y visible**, no imposible.

## Estructura del repositorio

| Carpeta | Contenido |
|---|---|
| `core/` | Núcleo Rust: reglas del día, eventos firmados, cripto, almacenamiento SQLCipher, sincronización, filtro DNS |
| `proto/` | Formato de eventos y mensajes compartidos |
| `verify/` | Lógica de verificación (pose, lectura, resumen, comida, páginas). Sin red ni disco, comprobado en CI |
| `netfilter/` | Servidor DNS local del PC de un menor (la política la decide el núcleo) |
| `desktop/` | Servicio de Windows (`agent/`), IPC (`ipc/`), UI Tauri + Svelte (`app/`), verificador en AppContainer (`sandbox/`, `verifier/`) |
| `android/` | App Kotlin + Jetpack Compose (`app/`) y verificador sin permiso de Internet (`verifier/`) |
| `apple/` | Núcleo para iOS/macOS (`ffi/`) y app de iOS con extensiones de Screen Time |
| `lists/`, `scripts/` | Fuentes de las listas de categorías; escáneres de privacidad y utilidades de build |
| `relay/`, `push-gateway/` | Arquitectura anterior, **en proceso de eliminación** |

## Compilar

Requisitos: Rust 1.100 (lo fija `rust-toolchain.toml`), [`just`](https://github.com/casey/just), Python 3, Node 22 (UI de escritorio), JDK 17 + Android SDK 35 (Android). En Windows, SQLCipher necesita [Strawberry Perl](https://strawberryperl.com/) para compilar OpenSSL.

```bash
just check      # formato, clippy, pruebas, cargo-deny, cargo-audit y escáneres de privacidad
just app        # UI de escritorio (Svelte + Tauri)
just bindings   # bindings Kotlin del núcleo
just fuzz <objetivo> 60   # fuzzing (requiere nightly)
```

> 🔒 **No instales el servicio de Windows en tu equipo de trabajo:** cierra apps. Usa una máquina virtual con una cuenta estándar.

## Seguridad

Si encuentras una vulnerabilidad, **no abras un issue público**: sigue [SECURITY.md](SECURITY.md). Los principios de arriba no son marketing: el CI falla si aparece código que grabe la cámara a disco, si el verificador enlaza una librería de red o si una dependencia tiene una licencia o vulnerabilidad no permitida.

## Contribuir

Las contribuciones son bienvenidas. Antes de un cambio grande, abre un issue para hablarlo.

- `just check` tiene que pasar en local.
- Identificadores de código en inglés; comentarios, documentación y textos de la interfaz en español.
- Sin SDKs de analítica, anuncios ni reporte de errores. Cada dependencia nueva necesita justificación y licencia compatible (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0; no GPL/AGPL/LGPL).
- Los cambios en cripto, sincronización, autorización, el verificador o el servicio privilegiado necesitan dos revisiones.

## Licencia

[Mozilla Public License 2.0](LICENSE).
