# Política de seguridad

Si encuentras una vulnerabilidad, **no abras un issue público**. Repórtala en privado con
**"Report a vulnerability"** en la pestaña *Security* de este repositorio (GitHub Private
Vulnerability Reporting).

- Confirmamos la recepción en 72 horas.
- Te mantenemos informado al menos cada 7 días.
- Publicamos la corrección y te damos crédito (si lo deseas) tras un máximo de 90 días.

**Alcance:** apps (Android, iOS, escritorio), servicio privilegiado y su IPC, verificadores
sin red, sincronización entre dispositivos (vinculación por QR, protocolo P2P, descubrimiento
en la red local), formato de eventos y criptografía, métodos de autorización de cambios
(contraseña, patrón, biometría, firmas de tutor y de contacto de confianza, códigos sin red)
y filtro DNS.

**Fuera de alcance:** ataques que requieren desbloquear físicamente el dispositivo con el
código del usuario; el código de `relay/` y `push-gateway/`, que está en proceso de eliminación
y no forma parte de ninguna versión publicada.

ProcastinaciOFF no tiene servidores: no hay datos de usuarios en manos del proyecto.
