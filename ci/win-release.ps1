$ARCHIVE_PATH = "rolf-windows-amd64"

cargo build --release

if (Test-Path -Path $ARCHIVE_PATH) {
    Remove-Item -Recurse $ARCHIVE_PATH
}
New-Item -Path $ARCHIVE_PATH -ItemType "directory"
Copy-Item @("target\release\rolf.exe", "LICENSE", "ci\wezterm.lua") -Destination $ARCHIVE_PATH

# This checks if the GITHUB_SHA environment variable exists
if ($env:GITHUB_SHA -ne $null) {
    Write-Output $env:GITHUB_SHA >> "$ARCHIVE_PATH\commit-sha"
}
