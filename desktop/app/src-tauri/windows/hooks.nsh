; NSIS installer hooks (security-critical). SIDs are used (group names change with the system language):
; S-1-5-18 = SYSTEM, S-1-5-32-544 = Administrators.

!macro PCO_FIREWALL_REMOVE
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="ProcastinatiOFF sync"'
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="ProcastinatiOFF discovery"'
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; 1) Data folder: SYSTEM and Administrators only
  CreateDirectory "$COMMONAPPDATA\ProcastinatiOFF"
  nsExec::ExecToLog 'icacls "$COMMONAPPDATA\ProcastinatiOFF" /inheritance:r /grant:r *S-1-5-18:(OI)(CI)F *S-1-5-32-544:(OI)(CI)F'
  ; 2) The service also starts in Safe Mode (with and without networking)
  WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Minimal\ProcastinatiOFF" "" "Service"
  WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Network\ProcastinatiOFF" "" "Service"
  ; 3) Sync with the profile's devices: only on networks marked Private (home), only the service.
  ;    QUIC on UDP 47800 and mDNS discovery on UDP 5353. Nothing is opened on Public networks.
  !insertmacro PCO_FIREWALL_REMOVE
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="ProcastinatiOFF sync" dir=in action=allow protocol=UDP localport=47800 profile=private program="$INSTDIR\procastinatioff-agent.exe"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="ProcastinatiOFF discovery" dir=in action=allow protocol=UDP localport=5353 profile=private program="$INSTDIR\procastinatioff-agent.exe"'
  ; 4) Register and start the service (LocalSystem, automatic restart)
  nsExec::ExecToLog '"$INSTDIR\procastinatioff-agent.exe" install'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Only an administrator gets here (perMachine install).
  ; Family mode: returns the adapters' DNS to DHCP and removes the filter's firewall rules.
  ; Without this, a minor's PC would be left without DNS resolution after uninstalling.
  nsExec::ExecToLog '"$INSTDIR\procastinatioff-agent.exe" restore-dns'
  nsExec::ExecToLog 'taskkill /F /IM procastinatioff-agent.exe'
  nsExec::ExecToLog 'taskkill /F /IM procastinatioff-app.exe'
  nsExec::ExecToLog 'sc.exe delete ProcastinatiOFF'
  !insertmacro PCO_FIREWALL_REMOVE
  DeleteRegKey HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Minimal\ProcastinatiOFF"
  DeleteRegKey HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Network\ProcastinatiOFF"
  ; C:\ProgramData\ProcastinatiOFF is not deleted: full erasure is the crypto-shredding from the app.
!macroend
