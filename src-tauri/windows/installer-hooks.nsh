; Always replace an existing FujiView installation before copying new files.
; /UPDATE preserves application data and shortcuts while the new setup runs.
!macro NSIS_HOOK_PREINSTALL
  ReadRegStr $R8 SHCTX "${UNINSTKEY}" "UninstallString"
  ReadRegStr $R9 SHCTX "${MANUPRODUCTKEY}" ""

  ${If} $R8 != ""
    DetailPrint "Removing the previous FujiView installation..."
    ClearErrors
    ExecWait '$R8 /S /UPDATE _?=$R9' $R7

    ${If} ${Errors}
      MessageBox MB_ICONSTOP "The previous FujiView installation could not be started for removal. Installation has been stopped."
      Abort
    ${ElseIf} $R7 != 0
      MessageBox MB_ICONSTOP "The previous FujiView installation could not be removed (exit code $R7). Installation has been stopped."
      Abort
    ${EndIf}
  ${EndIf}
!macroend
