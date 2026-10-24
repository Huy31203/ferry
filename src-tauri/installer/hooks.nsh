; Stream H: "Optimise with Ferry" in Explorer and Ferry under "Open with", for the
; current user only (installMode currentUser). Ferry never becomes the default app:
; nothing here writes the default value of HKCU\Software\Classes\.<ext>.
; Windows 11 shows the verb under "Show more options"; the top-level menu needs a
; signed sparse package (out of scope).
!define FERRY_PROGID "Ferry.File"
!define FERRY_VERB "Ferry.Optimise"

!macro FERRY_REGISTER_TYPE EXT
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\${FERRY_VERB}" "" "Optimise with Ferry"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\${FERRY_VERB}" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\${FERRY_VERB}\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" --optimise "%1"'
  WriteRegStr HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${FERRY_PROGID}" ""
!macroend

!macro FERRY_UNREGISTER_TYPE EXT
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\${FERRY_VERB}"
  DeleteRegValue HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${FERRY_PROGID}"
!macroend

; Keep in step with ferry_core::integrations::FILE_TYPES (a test checks).
!macro FERRY_EACH_TYPE MACRO
  !insertmacro ${MACRO} "png"
  !insertmacro ${MACRO} "jpg"
  !insertmacro ${MACRO} "jpeg"
  !insertmacro ${MACRO} "gif"
  !insertmacro ${MACRO} "webp"
  !insertmacro ${MACRO} "heic"
  !insertmacro ${MACRO} "heif"
  !insertmacro ${MACRO} "tif"
  !insertmacro ${MACRO} "tiff"
  !insertmacro ${MACRO} "avif"
  !insertmacro ${MACRO} "mp4"
  !insertmacro ${MACRO} "mov"
  !insertmacro ${MACRO} "m4v"
  !insertmacro ${MACRO} "pdf"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\${FERRY_PROGID}" "" "Ferry"
  WriteRegStr HKCU "Software\Classes\${FERRY_PROGID}\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\${FERRY_PROGID}\shell\open" "" "Optimise with Ferry"
  WriteRegStr HKCU "Software\Classes\${FERRY_PROGID}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" --optimise "%1"'
  !insertmacro FERRY_EACH_TYPE FERRY_REGISTER_TYPE
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro FERRY_EACH_TYPE FERRY_UNREGISTER_TYPE
  DeleteRegKey HKCU "Software\Classes\${FERRY_PROGID}"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
