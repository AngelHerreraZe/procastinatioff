; Ganchos del instalador NSIS 🔒. Se usan SID (los nombres de grupo cambian con el idioma):
; S-1-5-18 = SYSTEM, S-1-5-32-544 = Administradores.

!macro NSIS_HOOK_POSTINSTALL
  ; 1) Carpeta de datos: solo SYSTEM y Administradores
  CreateDirectory "$COMMONAPPDATA\ProcastinaciOFF"
  nsExec::ExecToLog 'icacls "$COMMONAPPDATA\ProcastinaciOFF" /inheritance:r /grant:r *S-1-5-18:(OI)(CI)F *S-1-5-32-544:(OI)(CI)F'
  ; 2) El servicio también arranca en Modo seguro (con y sin red)
  WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Minimal\ProcastinaciOFF" "" "Service"
  WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Network\ProcastinaciOFF" "" "Service"
  ; 3) Registrar e iniciar el servicio (LocalSystem, reinicio automático)
  nsExec::ExecToLog '"$INSTDIR\procastinacioff-agent.exe" install'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Solo llega aquí un administrador (instalación perMachine)
  nsExec::ExecToLog 'taskkill /F /IM procastinacioff-agent.exe'
  nsExec::ExecToLog 'taskkill /F /IM procastinacioff-app.exe'
  nsExec::ExecToLog 'sc.exe delete ProcastinaciOFF'
  DeleteRegKey HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Minimal\ProcastinaciOFF"
  DeleteRegKey HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Network\ProcastinaciOFF"
  ; No se borra C:\ProgramData\ProcastinaciOFF: el borrado total es el crypto-shredding desde la app.
!macroend
