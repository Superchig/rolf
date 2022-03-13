$ARCHIVE_PATH = "rolf-windows-amd64"

if (Test-Path -Path $ARCHIVE_PATH) {
    Remove-Item -Recurse $ARCHIVE_PATH
}
New-Item -Path $ARCHIVE_PATH -ItemType "directory"
Copy-Item @("target\release\rolf.exe", "LICENSE", "ci\wezterm.lua", "ci\install-wezterm.bat", "ci\install-wezterm.ps1") -Destination $ARCHIVE_PATH

# This checks if the GITHUB_SHA environment variable exists
if ($env:GITHUB_SHA -ne $null) {
    Write-Output $env:GITHUB_SHA >> "$ARCHIVE_PATH\commit-sha"
}
