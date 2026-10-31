# Builds a Windows release of sqlsea into dist\: a zip holding sqlsea.exe
# (one file, icon and fonts inside), install.ps1, the README and licences.
#
# Needs the Rust MSVC toolchain and the Visual Studio C++ build tools, which
# also provide the Windows SDK used to embed the icon.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\package.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..\..')

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"$' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x86_64' }

cargo build --release --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$name = "sqlsea-$version-windows-$arch"
$stage = Join-Path 'dist\stage' $name
Remove-Item -Recurse -Force 'dist\stage' -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item 'target\release\sqlsea.exe', 'packaging\windows\install.ps1', 'README.md', 'LICENSE-MIT', 'LICENSE-APACHE' $stage

$zip = "dist\$name.zip"
Compress-Archive -Path $stage -DestinationPath $zip -Force
Remove-Item -Recurse -Force 'dist\stage'
Write-Output $zip
