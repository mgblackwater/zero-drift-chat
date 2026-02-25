$ErrorActionPreference = "Stop"

$repo = "mgblackwater/zero-drift-chat"
$asset = "zero-drift-chat-windows-x86_64.exe"
$installDir = "$env:LOCALAPPDATA\zero-drift-chat"

Write-Host "Installing zero-drift-chat..."

# Get latest release download URL
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$downloadUrl = $release.assets | Where-Object { $_.name -eq $asset } | Select-Object -ExpandProperty browser_download_url

if (-not $downloadUrl) {
    Write-Error "Could not find release asset '$asset'"
    exit 1
}

# Create install directory
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

# Download
$output = Join-Path $installDir "zero-drift-chat.exe"
Write-Host "Downloading from $downloadUrl..."
Invoke-WebRequest -Uri $downloadUrl -OutFile $output

Write-Host ""
Write-Host "Installed to $output"

# Add to PATH if not already there
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    Write-Host "Added $installDir to your PATH."
    Write-Host "Restart your terminal for PATH changes to take effect."
}

Write-Host ""
Write-Host "Run with: zero-drift-chat"
