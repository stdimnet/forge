; Forge's NSIS installer hooks (bundle.windows.nsis.installerHooks in tauri.conf.json).
;
; The file associations ("Open with Forge" for text and source files) come from
; bundle.fileAssociations. These hooks add the Explorer verbs Tauri has no setting for:
;   - "Open Folder in Forge" on folders and on the background of an open folder,
;   - "Edit with Forge" on every file (also files without an extension).
; SHCTX is HKCU\Software\Classes for a per-user install (the default) and HKLM for a
; per-machine one. The uninstaller removes every key added here.

!macro FORGE_SHELL_VERB KEY LABEL ARG
  WriteRegStr SHCTX "Software\Classes\${KEY}\shell\Forge" "" "${LABEL}"
  WriteRegStr SHCTX "Software\Classes\${KEY}\shell\Forge" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr SHCTX "Software\Classes\${KEY}\shell\Forge\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"${ARG}$\""
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro FORGE_SHELL_VERB "Directory" "Open Folder in Forge" "%1"
  !insertmacro FORGE_SHELL_VERB "Directory\Background" "Open Folder in Forge" "%V"
  !insertmacro FORGE_SHELL_VERB "*" "Edit with Forge" "%1"
  ; Tell Explorer that associations changed (SHCNE_ASSOCCHANGED).
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey SHCTX "Software\Classes\Directory\shell\Forge"
  DeleteRegKey SHCTX "Software\Classes\Directory\Background\shell\Forge"
  DeleteRegKey SHCTX "Software\Classes\*\shell\Forge"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
