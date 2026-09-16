; Portside NSIS installer hooks (wired via tauri.bundle.windows.nsis.installerHooks).
; Installs/points at the runtime prerequisites the app needs on a user's
; machine: VC++ redistributable, WSL2, Docker Desktop. WebView2 is already
; handled by Tauri's own webviewInstallMode step.
;
; Rules: never abort the install on a prereq failure (the app's own
; PrereqPanel onboarding covers leftovers at runtime), never force a reboot
; (/norestart everywhere, 3010 treated as success-with-reboot-pending).
;
; NOTE 32/64-bit: the NSIS installer runs 32-bit, so $SYSDIR resolves to
; SysWOW64 where wsl.exe does NOT exist. All wsl.exe paths below go through
; Sysnative explicitly.

!macro NSIS_HOOK_POSTINSTALL
  ; ---- VC++ 2015-2022 redistributable (x64), required by Rust/MSVC builds ----
  ClearErrors
  ReadRegDWord $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64" "Installed"
  ${If} ${Errors}
    StrCpy $0 0
  ${EndIf}
  ${If} $0 != 1
    DetailPrint "Portside: VC++ redistributable missing, downloading..."
    ExecWait '"$SYSDIR\curl.exe" -L --fail -o "$TEMP\portside-vc_redist.x64.exe" "https://aka.ms/vs/17/release/vc_redist.x64.exe"' $1
    ${If} $1 == 0
      ExecWait '"$TEMP\portside-vc_redist.x64.exe" /passive /norestart' $1
      ; 0 = ok, 3010 = ok but reboot pending, 1638 = newer version already present
      ${If} $1 == 0
        DetailPrint "Portside: VC++ redistributable installed"
      ${ElseIf} $1 == 3010
        DetailPrint "Portside: VC++ redistributable installed (reboot pending)"
      ${ElseIf} $1 == 1638
        DetailPrint "Portside: newer VC++ redistributable already present"
      ${Else}
        MessageBox MB_ICONEXCLAMATION "VC++ redistributable install returned $1. Portside's setup panel will guide you if the app misbehaves."
      ${EndIf}
      Delete "$TEMP\portside-vc_redist.x64.exe"
    ${Else}
      MessageBox MB_ICONEXCLAMATION "Could not download the VC++ redistributable (no network?). Portside's setup panel will guide you."
    ${EndIf}
  ${Else}
    DetailPrint "Portside: VC++ redistributable already installed"
  ${EndIf}

  ; ---- WSL2 (required by Docker Desktop) ----
  ; Resolve a working wsl.exe path first (Sysnative for the 32-bit installer).
  StrCpy $2 "$WINDIR\Sysnative\wsl.exe"
  IfFileExists "$2" wsl_path_ok
    StrCpy $2 "$WINDIR\System32\wsl.exe"
  wsl_path_ok:
  ExecWait '"$2" --status' $1
  ${If} $1 != 0
    MessageBox MB_YESNO "Portside needs WSL2 (Windows Subsystem for Linux) for its databases, and it is not installed. Installing downloads several hundred MB and can sit quiet for 10+ minutes - that is normal, leave it alone until it finishes.$\n$\nInstall WSL2 now (recommended)?" IDYES wsl_do_install IDNO wsl_skip
    wsl_do_install:
      DetailPrint "Portside: installing WSL2 (slow, do not close the installer)..."
      ExecWait '"$2" --install' $1
      ${If} $1 == 0
        MessageBox MB_OK "WSL2 installed. Reboot whenever suits you to finish it, then start Portside."
      ${Else}
        MessageBox MB_ICONEXCLAMATION "WSL install returned $1. Portside's setup panel will guide you on first launch."
      ${EndIf}
      Goto wsl_done
    wsl_skip:
      DetailPrint "Portside: WSL2 install skipped by user"
  ${Else}
    DetailPrint "Portside: WSL already present"
  ${EndIf}
  wsl_done:

  ; ---- Docker Desktop (too big to bundle: ~500MB+, link only) ----
  ExecWait '"$SYSDIR\cmd.exe" /c docker info >NUL 2>&1' $1
  ${If} $1 != 0
    MessageBox MB_YESNO "Docker Desktop was not detected. Portside cannot create databases without it.$\n$\nOpen the Docker Desktop download page now?" IDYES docker_open IDNO docker_skip
    docker_open:
      ExecShell "open" "https://www.docker.com/products/docker-desktop/"
    docker_skip:
      DetailPrint "Portside: Docker Desktop left for the user (setup panel guides at runtime)"
  ${Else}
    DetailPrint "Portside: Docker already present"
  ${EndIf}
!macroend
