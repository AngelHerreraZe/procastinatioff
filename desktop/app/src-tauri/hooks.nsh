; Ganchos del instalador NSIS (Tauri). 🔒 Ruta crítica.

!macro NSIS_HOOK_PREUNINSTALL
  ; Modo familiar: devuelve el DNS de las tarjetas a DHCP y quita las reglas de firewall (07b · 2.2).
  ; Sin esto, el PC de un menor se quedaría sin resolución DNS al desinstalar.
  nsExec::Exec '"$INSTDIR\procastinacioff-agent.exe" restore-dns'
!macroend
