; NSIS installer hooks (Tauri). SECURITY: Critical path.

!macro NSIS_HOOK_PREUNINSTALL
  ; Family mode: returns the adapters' DNS to DHCP and removes the firewall rules.
  ; Without this, a minor's PC would be left without DNS resolution after uninstalling.
  nsExec::Exec '"$INSTDIR\procastinacioff-agent.exe" restore-dns'
!macroend
