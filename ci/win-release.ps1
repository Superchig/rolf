$ARCHIVE_PATH = "rolf-windows-amd64"

if (Test-Path -Path $ARCHIVE_PATH) {
    Remove-Item -Recurse $ARCHIVE_PATH
}
New-Item -Path $ARCHIVE_PATH -ItemType "directory"
Copy-Item @("target\release\rolf.exe", "LICENSE", "ci\win-scripts") -Destination $ARCHIVE_PATH -Recurse

# This checks if the GITHUB_SHA environment variable exists
if ($null -ne $env:GITHUB_SHA) {
    Write-Output $env:GITHUB_SHA >> "$ARCHIVE_PATH\commit-sha"
}

# We use Out-File in order to ensure that the corret encoding is used, as
# PowerShell's default utf16-le encoding output is incompatible with running
# bat files
Write-Output "PowerShell.exe -ExecutionPolicy Bypass win-scripts\install-wezterm.ps1" | Out-File "$ARCHIVE_PATH\install-wezterm.bat" -Encoding oem
