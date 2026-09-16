#Requires -Version 5.1
<#
.SYNOPSIS
  Install missing Portside toolchain on Windows via winget. Fully automatic
  and idempotent: skips anything already installed, so re-running is safe.
  Rust installs per-user (no admin); Build Tools + Docker + WSL self-elevate
  via one UAC prompt because they need admin.
  NEVER reboots and NEVER registers anything at startup. If a reboot is
  needed (WSL / Docker), it only prints that - you reboot when it suits you.
  Exit codes: 0 = success (run.bat rechecks and continues),
              1 = hard failure (see output).
  Usage: powershell -ExecutionPolicy Bypass -File scripts/setup-windows.ps1
#>
param([switch]$Elevated, [string]$LogTo = "")
$ErrorActionPreference = "Stop"

# Shared log path (ProgramData: writable elevated, readable unelevated).
# The elevated child writes it itself via Start-Transcript because
# Start-Process -RedirectStandardOutput cannot combine with -Verb RunAs.
$AdminLog = "C:\ProgramData\portside-setup.log"
function Have($name) { return [bool](Get-Command $name -ErrorAction SilentlyContinue) }
function VcPresent() {
  # Ground truth is the compiler itself: vswhere component queries can miss
  # instances that still ship a working toolchain, so check cl.exe on disk too.
  $vw = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  if (Test-Path $vw) {
    $roots = @(& $vw -products * -property installationPath 2>$null)
    foreach ($r in $roots) {
      if ([string]::IsNullOrWhiteSpace($r)) { continue }
      $cl = Get-ChildItem "$r\VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
      if ($cl) { return $true }
    }
  }
  foreach ($ed in @("BuildTools", "Community", "Professional", "Enterprise")) {
    $cl = Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\$ed\VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($cl) { return $true }
  }
  return $false
}

# --- Phase 1 (current user): Rust is per-user, install it BEFORE elevating,
#     otherwise it lands in the Administrator profile and stays invisible. ---
if (-not $Elevated) {
  Write-Host "== Portside Windows setup ==" -ForegroundColor Cyan
  if (-not (Have "cargo")) {
    if (Have "rustup") {
      Write-Host "Rustup present but no toolchain - installing stable..." -ForegroundColor Cyan
      rustup toolchain install stable
      rustup default stable
    } else {
      $rustupExe = Join-Path $env:USERPROFILE ".cargo\bin\rustup.exe"
      if (Test-Path $rustupExe) {
        Write-Host "Rustup present but not on PATH - installing stable toolchain..." -ForegroundColor Cyan
        & $rustupExe toolchain install stable
        & $rustupExe default stable
      } else {
        Write-Host "Installing Rustup (Rustlang.Rustup, per-user)..." -ForegroundColor Cyan
        winget install --exact --id Rustlang.Rustup --source winget --accept-package-agreements --accept-source-agreements
        $env:Path += ";$env:USERPROFILE\.cargo\bin"
        if (Have "rustup") {
          rustup toolchain install stable
          rustup default stable
        } else {
          Write-Host "[FAIL] rustup installed but still not on PATH." -ForegroundColor Red
          exit 1
        }
      }
    }
  } else {
    Write-Host "[OK] cargo already present: $(cargo --version)"
  }

  # --- Phase 2 needs admin: relaunch elevated. Output goes to a shared log
  #     (a new window can't print here), then we show its tail below. ---
  $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  if (-not $isAdmin) {
    Write-Host "Elevating for Build Tools + Docker + WSL (one UAC prompt)..." -ForegroundColor Yellow
    Remove-Item $AdminLog -ErrorAction SilentlyContinue
    $args = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -Elevated -LogTo `"$AdminLog`""
    $p = Start-Process powershell -ArgumentList $args -Verb RunAs -Wait -PassThru
    if (Test-Path $AdminLog) {
      Write-Host "--- elevated setup output (tail) ---" -ForegroundColor Cyan
      Get-Content $AdminLog | Select-Object -Last 30
    } else {
      Write-Host "(no elevated log captured - the elevated window may have been closed)" -ForegroundColor Yellow
    }
    exit $p.ExitCode
  }
  Write-Host "Already elevated - continuing to admin steps in this session." -ForegroundColor Cyan
}

# --- Phase 2 (elevated): WSL2, VS Build Tools C++ workload, Docker Desktop ---
if ($LogTo) { try { Start-Transcript -Path $LogTo -Force >$null } catch { } }
Write-Host "== Portside admin setup (elevated) ==" -ForegroundColor Cyan

if (-not (Have "winget")) {
  Write-Host "[FAIL] winget not found - install App Installer from Microsoft Store first." -ForegroundColor Red
  exit 1
}

# WSL2 first: Docker Desktop cannot run without it. Prompt (recommended),
# install now, reboot later whenever it suits you - never forced.
# NOTE: the probe runs with Continue preference - wsl.exe writes to stderr
# when the feature is off, which Stop would turn into a terminating error.
$wslOk = $false
$prevPref = $ErrorActionPreference
try {
  $ErrorActionPreference = "Continue"
  wsl --status >$null 2>&1
  if ($LASTEXITCODE -eq 0) { $wslOk = $true }
} catch { } finally { $ErrorActionPreference = $prevPref }
if ($wslOk) {
  Write-Host "[OK] WSL present"
} else {
  Write-Host "Docker Desktop requires WSL2, which is not installed." -ForegroundColor Yellow
  Write-Host "Recommended: say yes - it installs now, you reboot whenever suits you." -ForegroundColor Yellow
  $ans = Read-Host "Install WSL2 now? [Y/n]"
  if ($ans -eq "" -or $ans -match '^[Yy]') {
    Write-Host "=================================================================" -ForegroundColor Yellow
    Write-Host " Installing WSL2 (default Ubuntu). PLEASE READ:" -ForegroundColor Yellow
    Write-Host " - Downloads the kernel + Ubuntu (several hundred MB)." -ForegroundColor Yellow
    Write-Host " - Takes 10+ minutes on slow connections and goes QUIET for" -ForegroundColor Yellow
    Write-Host "   long stretches. That is normal. DO NOT close this window," -ForegroundColor Yellow
    Write-Host "   DO NOT press Ctrl+C - just leave it alone until it finishes." -ForegroundColor Yellow
    Write-Host " - If Ubuntu later asks you to create a username/password, that" -ForegroundColor Yellow
    Write-Host "   is normal: pick anything, it is just your local Linux user." -ForegroundColor Yellow
    Write-Host "=================================================================" -ForegroundColor Yellow
    wsl --install
    Write-Host "WSL2 install finished. Reboot whenever suits you to complete it, then re-run run.bat." -ForegroundColor Green
  } else {
    Write-Host "Skipped. Docker Desktop cannot run without WSL2 - install later with 'wsl --install' from an admin terminal." -ForegroundColor Yellow
  }
}

# C++ workload for the Tauri link step. NOTE: the workload id is
# NativeDesktop ("Desktop development with C++"); VCTools is a component
# inside it, and passing it as a workload id silently does nothing.
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vsInstaller = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vs_installer.exe"
if (VcPresent) {
  Write-Host "[OK] MSVC C++ workload already present"
} else {
  $anyVs = $null
  if (Test-Path $vsWhere) {
    $anyVs = & $vsWhere -products * -property installationPath 2>$null | Select-Object -First 1
  }
  if ($anyVs -and (Test-Path $vsInstaller)) {
    # Installer present but workload missing: add it directly, no manual clicks.
    # NOTE: no --wait flag - vs_installer 4.10.x rejects it as unknown (exit 87).
    Write-Host "Adding C++ workload to $anyVs (large, ~5GB)..." -ForegroundColor Cyan
    & $vsInstaller modify --installPath "$anyVs" `
      --add Microsoft.VisualStudio.Workload.NativeDesktop --includeRecommended `
      --passive --norestart
    Write-Host "vs_installer modify exit code: $LASTEXITCODE" -ForegroundColor Cyan
  } else {
    Write-Host "Installing VS 2022 Build Tools + C++ workload (large, ~5GB)..." -ForegroundColor Cyan
    winget install --exact --id Microsoft.VisualStudio.2022.BuildTools --source winget `
      --accept-package-agreements --accept-source-agreements `
      --override "--add Microsoft.VisualStudio.Workload.NativeDesktop --includeRecommended --passive --norestart"
  }
  if (VcPresent) {
    Write-Host "[OK] MSVC C++ workload installed"
  } else {
    # Silent/passive modify failed (often another installer instance is
    # busy, or flags were rejected). Fall back to the visible installer UI
    # so you can click Modify yourself - it opens on the right screen.
    if ($anyVs -and (Test-Path $vsInstaller)) {
      Write-Host "Silent install did not take. Opening VS Installer UI - click Modify there, then return here." -ForegroundColor Yellow
      Start-Process $vsInstaller -ArgumentList "modify --installPath `"$anyVs`"" -Wait
    }
    if (VcPresent) {
      Write-Host "[OK] MSVC C++ workload installed"
    } else {
      Write-Host "[FAIL] C++ workload still not detected. In VS Installer > Modify > check 'Desktop development with C++' > Modify, then re-run run.bat." -ForegroundColor Red
      exit 1
    }
  }
}

if (-not (Have "docker")) {
  $knownDocker = "C:\Program Files\Docker\Docker\resources\bin\docker.exe"
  if (Test-Path $knownDocker) {
    Write-Host "[OK] Docker Desktop files already present (skipping reinstall)"
  } else {
    Write-Host "Installing Docker Desktop..." -ForegroundColor Cyan
    winget install --exact --id Docker.DockerDesktop --source winget --accept-package-agreements --accept-source-agreements
    Write-Host "Docker Desktop installed. If the daemon won't start, reboot whenever suits you, then re-run run.bat." -ForegroundColor Yellow
  }
} else {
  Write-Host "[OK] docker already present"
}

Write-Host "`nDone. No reboot was forced - if Docker or WSL asks for one, do it whenever suits you, then re-run run.bat." -ForegroundColor Green
exit 0
