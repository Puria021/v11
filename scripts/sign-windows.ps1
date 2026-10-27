# Authenticode signing for Tauri's signCommand. Called as: sign-windows.ps1 <file>
# Needs SIGN_PFX (path to the .pfx) and SIGN_PASSWORD (its password) in the environment.
param([Parameter(Mandatory = $true)][string]$File)
$ErrorActionPreference = 'Stop'

if (-not $env:SIGN_PFX -or -not (Test-Path $env:SIGN_PFX)) { throw 'SIGN_PFX is not set or the certificate file is missing.' }

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1
if (-not $signtool) { throw 'signtool.exe was not found on this runner.' }

& $signtool.FullName sign /f $env:SIGN_PFX /p $env:SIGN_PASSWORD /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /d 'SEO Master' $File
if ($LASTEXITCODE -ne 0) { throw "signtool sign failed for $File" }

& $signtool.FullName verify /pa $File
if ($LASTEXITCODE -ne 0) { throw "signature verification failed for $File" }
Write-Host "Signed: $File"
