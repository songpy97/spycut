$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$releaseTag = "autobuild-2026-07-31-14-10"
$asset = "ffmpeg-N-125875-g5d4d3bdc61-win64-lgpl.zip"
$expectedSha256 = "5d65df0c0ca5346d82df8ade9c2e12db45d1f978f18ff908b42f03f5223dfc90"
$url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/$releaseTag/$asset"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("spycut-ffmpeg-" + [Guid]::NewGuid())

try {
  New-Item -ItemType Directory -Path $temporary | Out-Null
  $archive = Join-Path $temporary $asset
  Write-Host "Downloading pinned Windows LGPL FFmpeg build..."
  Invoke-WebRequest -Uri $url -OutFile $archive -TimeoutSec 180
  $actualSha256 = (Get-FileHash -Path $archive -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualSha256 -ne $expectedSha256) {
    throw "FFmpeg archive SHA-256 mismatch."
  }
  Expand-Archive -Path $archive -DestinationPath (Join-Path $temporary "unpacked")
  $ffmpeg = Get-ChildItem -Path (Join-Path $temporary "unpacked") -Recurse -Filter ffmpeg.exe | Select-Object -First 1
  $ffprobe = Get-ChildItem -Path (Join-Path $temporary "unpacked") -Recurse -Filter ffprobe.exe | Select-Object -First 1
  if (-not $ffmpeg -or -not $ffprobe) { throw "FFmpeg executables were not found in the archive." }

  $binaryDirectory = Join-Path $root "src-tauri\binaries"
  New-Item -ItemType Directory -Force -Path $binaryDirectory | Out-Null
  Copy-Item $ffmpeg.FullName (Join-Path $binaryDirectory "ffmpeg-x86_64-pc-windows-msvc.exe")
  Copy-Item $ffprobe.FullName (Join-Path $binaryDirectory "ffprobe-x86_64-pc-windows-msvc.exe")
  Write-Host "Prepared pinned Windows x64 LGPL sidecars."
} finally {
  if (Test-Path $temporary) { Remove-Item -Recurse -Force $temporary }
}
