# ForgeCode Windows Installer
# Usage: irm https://raw.githubusercontent.com/BbangMxn/ForgeCode/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

Write-Host "🔧 ForgeCode Installer" -ForegroundColor Cyan
Write-Host ""

# Detect architecture
$arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "i686" }
$target = "$arch-pc-windows-msvc"

# Get latest release
$repo = "BbangMxn/ForgeCode"
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$version = $release.tag_name

Write-Host "Latest version: $version" -ForegroundColor Green

# Find download URL
$asset = $release.assets | Where-Object { $_.name -like "*$target*" }
if (-not $asset) {
    Write-Host "Error: No release found for $target" -ForegroundColor Red
    exit 1
}

$url = $asset.browser_download_url
$fileName = $asset.name

# Download
$tempDir = Join-Path $env:TEMP "forge-install"
$zipPath = Join-Path $tempDir $fileName

Write-Host "Downloading $fileName..."
New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
Invoke-WebRequest -Uri $url -OutFile $zipPath

# Extract
Write-Host "Extracting..."
$extractPath = Join-Path $tempDir "extracted"
Expand-Archive -Path $zipPath -DestinationPath $extractPath -Force

# Install to user bin directory
$installDir = Join-Path $env:USERPROFILE ".forge\bin"
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

$exePath = Get-ChildItem -Path $extractPath -Filter "forge.exe" -Recurse | Select-Object -First 1
Copy-Item $exePath.FullName -Destination $installDir -Force

# Add to PATH if not already
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    Write-Host "Added $installDir to PATH" -ForegroundColor Yellow
}

# Cleanup
Remove-Item -Recurse -Force $tempDir

Write-Host ""
Write-Host "✓ ForgeCode $version installed successfully!" -ForegroundColor Green
Write-Host ""
Write-Host "Installation path: $installDir\forge.exe" -ForegroundColor Cyan
Write-Host ""
Write-Host "To get started:" -ForegroundColor Yellow
Write-Host "  1. Open a new terminal (to refresh PATH)"
Write-Host "  2. Run: forge"
Write-Host ""
