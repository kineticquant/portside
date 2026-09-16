#Requires -Version 5.1
<#
.SYNOPSIS
  Check Portside prerequisites. Exit codes: 0 = ready, 1 = missing
  (run setup), 2 = reboot pending (files present, reboot + first launch).
  Called by run.bat and `npm run check`.
  Refreshes session PATH from machine+user env first, so installs that
  just completed in another (elevated) process are visible here.
#>
$ErrorActionPreference = "Continue"
$missing = $false
$rebootNeeded = $false

# Pick up PATH changes made by installers (winget/VS/Docker) without reopening.
try {
  $m = [Environment]::GetEnvironmentVariable("Path", "Machine")
  $u = [Environment]::GetEnvironmentVariable("Path", "User")
  if ($m -or $u) { $env:Path = "$m;$u" }
} catch { }

# Well-known locations checked when the command is not on PATH yet.
$KnownCargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
$KnownDocker = "C:\Program Files\Docker\Docker\resources\bin\docker.exe"
$VsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"

function Test-Cmd($name) {
  return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

Write-Host "== Portside env check ==" -ForegroundColor Cyan

# Node
if (Test-Cmd "node") {
  $v = (node --version) 2>$null
  Write-Host "[OK] node $v"
} else {
  Write-Host "[MISS] node 20+ required (winget install OpenJS.NodeJS.LTS)" -ForegroundColor Red
  $missing = $true
}

# Rust / cargo (also accept %USERPROFILE%\.cargo\bin\cargo.exe pre-PATH-refresh)
if (Test-Cmd "cargo") {
  $v = try { (cargo --version) 2>$null } catch { "cargo (found, version unknown)" }
  Write-Host "[OK] $v"
} elseif (Test-Path $KnownCargo) {
  Write-Host "[OK] cargo found at $KnownCargo (PATH refresh pending - reopen terminal if 'cargo' stays unknown)"
} else {
  Write-Host "[MISS] Rust/cargo (winget install --exact --id Rustlang.Rustup, then: rustup toolchain install stable)" -ForegroundColor Red
  $missing = $true
}

# MSVC (Tauri link step needs a working compiler, not just the installer).
# Ground truth is cl.exe on disk: vswhere component queries can miss
# instances that still ship a working toolchain.
$vcOk = $false
$vcWhere = ""
if (Get-Command "cl.exe" -ErrorAction SilentlyContinue) {
  $vcOk = $true
  $vcWhere = "cl.exe (on PATH)"
} else {
  $roots = @()
  if (Test-Path $VsWhere) {
    $roots += @(& $VsWhere -products * -property installationPath 2>$null)
  }
  $roots += @(
    "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools",
    "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\Community",
    "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\Professional",
    "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\Enterprise"
  )
  foreach ($r in $roots) {
    if ([string]::IsNullOrWhiteSpace($r)) { continue }
    $cl = Get-ChildItem "$r\VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($cl) { $vcOk = $true; $vcWhere = $cl.FullName; break }
  }
}
if ($vcOk) {
  Write-Host "[OK] MSVC compiler at $vcWhere"
  # Tauri's resource step (winres) shells to rc.exe from PATH - cargo
  # check/test never exercise it, so verify it here instead of mid-build.
  $rc = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\rc.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($rc) {
    Write-Host "[OK] Windows SDK rc.exe at $($rc.FullName)"
  } else {
    Write-Host "[MISS] Windows SDK (rc.exe) not found - Tauri builds fail without it." -ForegroundColor Red
    Write-Host "       Fix: VS Installer > Modify > Individual components > check a 'Windows 11 SDK' > Modify." -ForegroundColor Red
    $missing = $true
  }
} elseif (Test-Path $VsWhere) {
  Write-Host "[MISS] Build Tools installer present but no C++ compiler found (run.bat adds it automatically)." -ForegroundColor Red
  $missing = $true
} else {
  Write-Host "[MISS] MSVC Build Tools not found (see TOOLCHAIN.md step 2)" -ForegroundColor Red
  $missing = $true
}

# WebView2 (ships with Edge; check update-client key, not msedgewebview2.exe path)
$wv = "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
if (Test-Path $wv) {
  Write-Host "[OK] WebView2 runtime key present"
} else {
  Write-Host "[WARN] WebView2 key not found; Edge normally provides it" -ForegroundColor Yellow
}

# Docker: distinguish MISS (never installed) from REBOOT (installed, needs reboot/first launch)
if (Test-Cmd "docker") {
  try {
    $sv = (docker info --format '{{.ServerVersion}}' 2>$null)
    if ($sv) {
      Write-Host "[OK] docker server $sv (daemon running)"
    } else {
      Write-Host "[WARN] docker CLI present but daemon not running - start Docker Desktop (run.bat does it automatically)" -ForegroundColor Yellow
    }
  } catch {
    Write-Host "[WARN] docker CLI present but daemon not reachable - start Docker Desktop (run.bat does it automatically)" -ForegroundColor Yellow
  }
} elseif (Test-Path $KnownDocker) {
  Write-Host "[REBOOT] Docker Desktop files present but CLI not on PATH/usable yet (reboot pending)." -ForegroundColor Yellow
  $rebootNeeded = $true
} else {
  Write-Host "[MISS] docker CLI (winget install --exact --id Docker.DockerDesktop, then reboot)" -ForegroundColor Red
  $missing = $true
}

# node_modules
if (Test-Path (Join-Path $PSScriptRoot "..\node_modules")) {
  Write-Host "[OK] node_modules/ present"
} else {
  Write-Host "[INFO] node_modules/ absent - will run npm install on launch"
}

if ($missing) {
  Write-Host "`nNOT READY - run run.bat (installs automatically) or see TOOLCHAIN.md" -ForegroundColor Red
  exit 1
}
if ($rebootNeeded) {
  Write-Host "`nNOT READY - reboot pending (see [REBOOT] lines above)." -ForegroundColor Yellow
  exit 2
}
Write-Host "`nREADY" -ForegroundColor Green
exit 0
