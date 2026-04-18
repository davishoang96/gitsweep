param(
    [Parameter(Mandatory=$true, Position=0)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version
)

$root = $PSScriptRoot

# package.json
$pkgPath = Join-Path $root "package.json"
$pkg = Get-Content $pkgPath -Raw | ConvertFrom-Json
$pkg.version = $Version
$pkg | ConvertTo-Json -Depth 10 | Set-Content $pkgPath -NoNewline

# src-tauri/tauri.conf.json
$tauriPath = Join-Path $root "src-tauri/tauri.conf.json"
$tauri = Get-Content $tauriPath -Raw | ConvertFrom-Json
$tauri.version = $Version
$tauri | ConvertTo-Json -Depth 10 | Set-Content $tauriPath -NoNewline

# src-tauri/Cargo.toml
$cargoPath = Join-Path $root "src-tauri/Cargo.toml"
(Get-Content $cargoPath) -replace '^version = ".*"', "version = `"$Version`"" | Set-Content $cargoPath

Write-Host "Updated version to $Version in:" -ForegroundColor Green
Write-Host "  - package.json"
Write-Host "  - src-tauri/tauri.conf.json"
Write-Host "  - src-tauri/Cargo.toml"
