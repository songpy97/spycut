#requires -Version 5.1
param(
  [switch]$SkipChecks,
  [switch]$SmokeTest
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw "Windows packages must be built on Windows."
}

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$bundleDirectory = Join-Path $root "src-tauri\target\release\bundle\nsis"
$prepareSidecars = Join-Path $PSScriptRoot "prepare-ffmpeg-windows.ps1"
$tauriConfig = "src-tauri/tauri.release.conf.json"
$pnpm = (Get-Command pnpm.cmd -ErrorAction Stop).Source

function Write-Step([string]$Message) {
  Write-Host "[package-windows] $Message"
}

function Invoke-BoundedProcess {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$ArgumentList = @(),
    [Parameter(Mandatory = $true)][int]$TimeoutSeconds,
    [string]$WorkingDirectory = $root
  )

  $stdout = Join-Path ([System.IO.Path]::GetTempPath()) ("spycut-process-" + [Guid]::NewGuid() + ".out.log")
  $stderr = Join-Path ([System.IO.Path]::GetTempPath()) ("spycut-process-" + [Guid]::NewGuid() + ".err.log")
  try {
    $process = Start-Process -FilePath $FilePath `
      -ArgumentList $ArgumentList `
      -WorkingDirectory $WorkingDirectory `
      -NoNewWindow `
      -PassThru `
      -RedirectStandardOutput $stdout `
      -RedirectStandardError $stderr
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
      & taskkill.exe /PID $process.Id /T /F 2>$null | Out-Null
      throw "Process timed out after $TimeoutSeconds seconds: $FilePath $($ArgumentList -join ' ')"
    }
    $process.WaitForExit()
    if (Test-Path $stdout) { Get-Content $stdout | Write-Host }
    if (Test-Path $stderr) { Get-Content $stderr | Write-Host }
    if ($process.ExitCode -ne 0) {
      throw "Process exited with code $($process.ExitCode): $FilePath $($ArgumentList -join ' ')"
    }
  } finally {
    Remove-Item $stdout, $stderr -Force -ErrorAction SilentlyContinue
  }
}

function Get-ExistingSpyCutInstallations {
  $roots = @(
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
  )
  foreach ($registryRoot in $roots) {
    if (-not (Test-Path $registryRoot)) { continue }
    Get-ChildItem $registryRoot -ErrorAction SilentlyContinue |
      ForEach-Object { Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue } |
      Where-Object {
        $_.PSObject.Properties.Name -contains "DisplayName" -and $_.DisplayName -eq "SpyCut"
      }
  }
}

function Assert-SmokeDirectory([string]$Path) {
  $smokeRoot = [System.IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA "SpyCut-SmokeTests")).TrimEnd('\')
  $fullPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
  $parent = (Split-Path $fullPath -Parent).TrimEnd('\')
  $leaf = Split-Path $fullPath -Leaf
  if ($parent -ne $smokeRoot -or $leaf -notmatch '^SpyCut-Smoke-[0-9a-f-]+$') {
    throw "Refusing to use an unexpected smoke-test directory: $Path"
  }
}

function Test-Installer {
  param([Parameter(Mandatory = $true)][string]$InstallerPath)

  if (@(Get-ExistingSpyCutInstallations).Count -gt 0) {
    throw "Smoke testing is disabled because SpyCut is already installed on this machine. Use a clean Windows VM or CI runner."
  }

  $smokeRoot = Join-Path $env:LOCALAPPDATA "SpyCut-SmokeTests"
  $installDirectory = Join-Path $smokeRoot "SpyCut-Smoke-$([Guid]::NewGuid().ToString('D'))"
  Assert-SmokeDirectory $installDirectory
  try {
    Write-Step "Running the NSIS self-check and silent installation."
    Invoke-BoundedProcess -FilePath $InstallerPath `
      -ArgumentList @("/S", "/D=$installDirectory") `
      -TimeoutSeconds 300 `
      -WorkingDirectory (Split-Path $InstallerPath)

    $requiredFiles = @(
      "spycut.exe",
      "ffmpeg.exe",
      "ffprobe.exe",
      "uninstall.exe",
      "licenses\FFmpeg-NOTICE.md",
      "licenses\COPYING.LGPLv2.1"
    )
    foreach ($relativePath in $requiredFiles) {
      $installedPath = Join-Path $installDirectory $relativePath
      if (-not (Test-Path $installedPath -PathType Leaf)) {
        throw "Installed package is missing: $relativePath"
      }
    }

    Write-Step "Silent installation passed; running the uninstaller."
    $uninstaller = Join-Path $installDirectory "uninstall.exe"
    Invoke-BoundedProcess -FilePath $uninstaller `
      -ArgumentList @("/S", "_?=$installDirectory") `
      -TimeoutSeconds 300 `
      -WorkingDirectory $installDirectory

    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while ((Test-Path (Join-Path $installDirectory "spycut.exe")) -and [DateTime]::UtcNow -lt $deadline) {
      Start-Sleep -Milliseconds 250
    }
    if (Test-Path (Join-Path $installDirectory "spycut.exe")) {
      throw "Uninstall smoke test did not remove spycut.exe."
    }
    if (@(Get-ExistingSpyCutInstallations).Count -gt 0) {
      throw "Uninstall smoke test left a SpyCut uninstall registry entry."
    }
    Write-Step "Install and uninstall smoke tests passed."
  } finally {
    Assert-SmokeDirectory $installDirectory
    if (Test-Path $installDirectory) {
      Remove-Item -LiteralPath $installDirectory -Recurse -Force
    }
    if ((Test-Path $smokeRoot) -and @(Get-ChildItem $smokeRoot -Force).Count -eq 0) {
      Remove-Item -LiteralPath $smokeRoot -Force
    }
  }
}

Set-Location $root
Write-Step "Preparing pinned Windows FFmpeg sidecars."
$powershell = (Get-Process -Id $PID).Path
Invoke-BoundedProcess -FilePath $powershell `
  -ArgumentList @("-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", $prepareSidecars) `
  -TimeoutSeconds 900

if (-not $SkipChecks) {
  Write-Step "Installing locked JavaScript dependencies."
  Invoke-BoundedProcess -FilePath $pnpm -ArgumentList @("install", "--frozen-lockfile") -TimeoutSeconds 900
  Write-Step "Running the release test suite."
  Invoke-BoundedProcess -FilePath $pnpm -ArgumentList @("check") -TimeoutSeconds 1800
}

New-Item -ItemType Directory -Force -Path $bundleDirectory | Out-Null
Get-ChildItem $bundleDirectory -Filter "SpyCut_*_x64-setup.exe" -File -ErrorAction SilentlyContinue |
  Remove-Item -Force

Write-Step "Building NSIS on native Windows."
Invoke-BoundedProcess -FilePath $pnpm `
  -ArgumentList @("tauri", "build", "--config", $tauriConfig, "--bundles", "nsis") `
  -TimeoutSeconds 2400

$installers = @(Get-ChildItem $bundleDirectory -Filter "SpyCut_*_x64-setup.exe" -File)
if ($installers.Count -ne 1) {
  throw "Expected exactly one fresh NSIS installer, found $($installers.Count)."
}
$installer = $installers[0]
$hash = (Get-FileHash -Path $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$checksumPath = $installer.FullName + ".sha256"
Set-Content -Path $checksumPath -Encoding ascii -NoNewline -Value "$hash  $($installer.Name)`n"

if ($SmokeTest) {
  try {
    Test-Installer -InstallerPath $installer.FullName
  } catch {
    Remove-Item $installer.FullName, $checksumPath -Force -ErrorAction SilentlyContinue
    throw
  }
}

Write-Step "Package created and hashed:"
Write-Host "  $($installer.FullName)"
Write-Host "  $checksumPath"
Write-Host "  SHA-256 $hash"
