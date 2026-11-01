# Installs sqlsea for the current user. It copies sqlsea.exe to
# %LOCALAPPDATA%\Programs\sqlsea and adds sqlsea to the Start menu.
# You do not need administrator rights. To remove sqlsea, use -Remove.
#
#   powershell -ExecutionPolicy Bypass -File install.ps1 [-Remove]
param([switch]$Remove)
$ErrorActionPreference = 'Stop'

$dir = Join-Path $env:LOCALAPPDATA 'Programs\sqlsea'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'sqlsea.lnk'

if ($Remove) {
    Remove-Item -Force $shortcut -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    Write-Output 'Removed sqlsea.'
    exit 0
}

$exe = Join-Path $PSScriptRoot 'sqlsea.exe'
if (-not (Test-Path $exe)) { throw "sqlsea.exe was not found next to install.ps1." }

New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item -Force $exe $dir

$link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
$link.TargetPath = Join-Path $dir 'sqlsea.exe'
$link.WorkingDirectory = $dir
$link.Description = 'Browse tables, edit rows and run SQL'
$link.Save()

Write-Output "Installed sqlsea to $dir. It is in the Start menu now."
