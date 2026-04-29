; Mi Tienda POS Desktop — NSIS installer hooks
;
; Tauri 2 inyecta estos macros en su template NSIS standard. Nos sirve
; para agregar pasos custom al install/uninstall sin reemplazar todo el
; template (que cambia entre versiones de tauri-bundler).
;
; Ver: https://v2.tauri.app/distribute/windows-installer/#using-custom-nsis-template

; ─────────────────────────────────────────────────────────────────
; POST-INSTALL: corre apenas terminan de copiarse los archivos
; ─────────────────────────────────────────────────────────────────
!macro NSIS_HOOK_POSTINSTALL
  ; Acceso directo en el escritorio para que el cajero abra la app con
  ; doble click sin tener que buscarla en el menú Inicio.
  CreateShortCut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"

  ; Auto-arranque cuando prende la PC. Crítica para el flow de un comercio:
  ; la cajera prende la máquina a la mañana y ya tiene Mi Tienda POS abierto.
  ; Si el comercio no lo quiere, lo desactiva desde Configuración → Apps →
  ; Inicio en Windows.
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" \
    "${PRODUCTNAME}" '"$INSTDIR\${MAINBINARYNAME}.exe"'
!macroend

; ─────────────────────────────────────────────────────────────────
; PRE-UNINSTALL: corre antes de borrar archivos al desinstalar
; ─────────────────────────────────────────────────────────────────
!macro NSIS_HOOK_PREUNINSTALL
  Delete "$DESKTOP\${PRODUCTNAME}.lnk"

  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" \
    "${PRODUCTNAME}"
!macroend
