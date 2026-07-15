!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack '"$INSTDIR\resources\suite\node\node.exe" "$INSTDIR\resources\suite\install-suite.mjs" --root "$INSTDIR\resources\suite"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "CC Switch is installed, but Claude-Mem setup did not complete.$\r$\n$\r$\nSee: $LOCALAPPDATA\claude-mem-local\logs\suite-install.log"
  ${EndIf}
!macroend
