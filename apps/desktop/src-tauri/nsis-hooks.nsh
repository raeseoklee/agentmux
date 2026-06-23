!insertmacro WordReplace

!macro AGENTMUX_BROADCAST_ENVIRONMENT_CHANGE
  SendMessage 0xFFFF 0x001A 0 "STR:Environment" /TIMEOUT=5000
!macroend

!macro AGENTMUX_ADD_INSTALL_DIR_TO_USER_PATH
  ReadRegStr $0 HKCU "Environment" "Path"
  ClearErrors
  ${If} $0 == ""
    WriteRegExpandStr HKCU "Environment" "Path" "$INSTDIR"
    !insertmacro AGENTMUX_BROADCAST_ENVIRONMENT_CHANGE
  ${Else}
    StrCpy $1 ";$0;"
    ${StrLoc} $2 "$1" ";$INSTDIR;" ">"
    ${If} $2 == ""
      WriteRegExpandStr HKCU "Environment" "Path" "$0;$INSTDIR"
      !insertmacro AGENTMUX_BROADCAST_ENVIRONMENT_CHANGE
    ${EndIf}
  ${EndIf}
!macroend

!macro AGENTMUX_REMOVE_INSTALL_DIR_FROM_USER_PATH
  ReadRegStr $0 HKCU "Environment" "Path"
  ClearErrors
  ${If} $0 != ""
    ${If} $0 == "$INSTDIR"
      WriteRegExpandStr HKCU "Environment" "Path" ""
      !insertmacro AGENTMUX_BROADCAST_ENVIRONMENT_CHANGE
    ${Else}
      ${WordReplace} "$0" "$INSTDIR;" "" "+" $1
      ${WordReplace} "$1" ";$INSTDIR" "" "+" $1
      ${WordReplace} "$1" ";;" ";" "+" $1
      ${If} $1 != $0
        WriteRegExpandStr HKCU "Environment" "Path" "$1"
        !insertmacro AGENTMUX_BROADCAST_ENVIRONMENT_CHANGE
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro AGENTMUX_ADD_INSTALL_DIR_TO_USER_PATH
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro AGENTMUX_REMOVE_INSTALL_DIR_FROM_USER_PATH
!macroend
