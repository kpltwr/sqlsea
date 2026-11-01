# Makes a Windows package of sqlsea: dist\sqlsea-<version>-windows-<arch>.zip.
# The zip holds sqlsea.exe, install.ps1, the README and the licenses.
#
# You need the Rust MSVC toolchain and the Visual Studio C++ build tools.
# The build tools include the Windows SDK, which puts the icon in sqlsea.exe.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\package.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..\..')

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"$' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x86_64' }

# The build writes "~" for your user folder in the file paths that the
# binary keeps for error messages. Your user name is then not in the package.
$env:RUSTFLAGS = "$env:RUSTFLAGS --remap-path-prefix=$env:USERPROFILE=~"
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
