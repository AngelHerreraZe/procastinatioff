; NSIS installer hooks (security-critical). SIDs are used (group names change with the system language):
; S-1-5-18 = SYSTEM, S-1-5-32-544 = Administrators.

!macro NSIS_HOOK_POSTINSTALL
  ; 1) Data folder: SYSTEM and Administrators only
  CreateDirectory "$COMMONAPPDATA\ProcastinaciOFF"
  nsExec::ExecToLog 'icacls "$COMMONAPPDATA\ProcastinaciOFF" /inheritance:r /grant:r *S-1-5-18:(OI)(CI)F *S-1-5-32-544:(OI)(CI)F'
  ; 2) The service also starts in Safe Mode (with and without networking)
  WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Minimal\ProcastinaciOFF" "" "Service"
  WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Network\ProcastinaciOFF" "" "Service"
  ; 3) Register and start the service (LocalSystem, automatic restart)
  nsExec::ExecToLog '"$INSTDIR\procastinacioff-agent.exe" install'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Only an administrator gets here (perMachine install)
  nsExec::ExecToLog 'taskkill /F /IM procastinacioff-agent.exe'
  nsExec::ExecToLog 'taskkill /F /IM procastinacioff-app.exe'
  nsExec::ExecToLog 'sc.exe delete ProcastinaciOFF'
  DeleteRegKey HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Minimal\ProcastinaciOFF"
  DeleteRegKey HKLM "SYSTEM\CurrentControlSet\Control\SafeBoot\Network\ProcastinaciOFF"
  ; C:\ProgramData\ProcastinaciOFF is not deleted: full erasure is the crypto-shredding from the app.
!macroend
