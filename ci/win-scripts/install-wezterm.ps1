$WEZTERM_VERSION = "20220101-133340-7edc5b5a"
$DOWNLOAD_URL = "https://github.com/wez/wezterm/releases/download/$WEZTERM_VERSION/WezTerm-windows-$WEZTERM_VERSION.zip"

if (-not (Test-Path -Path WezTerm.zip)) {
    if (Test-Path -Path ..\WezTerm.zip) {
	Copy-Item -Path ..\WezTerm.zip -Destination .
    } else {
	Invoke-WebRequest -Uri $DOWNLOAD_URL -OutFile WezTerm.zip
    }
}

if (-not (Test-Path -Path WezTerm-extracted)) {
    Expand-Archive -Path WezTerm.zip -DestinationPath WezTerm-extracted
    Remove-Item WezTerm.zip
}

$EXTRACTED = Get-ChildItem -Path WezTerm-extracted
if ($EXTRACTED.length -eq 1) {
    Move-Item -Path "WezTerm-extracted\WezTerm-windows-$WEZTERM_VERSION" -Destination WezTerm
    Remove-Item -Recurse WezTerm-extracted
} else {
    Move-Item WezTerm-extracted WezTerm
}

if (Test-Path -Path win-scripts\wezterm.lua) {
    Copy-Item -Path win-scripts\wezterm.lua -Destination WezTerm
}

Write-Output "WezTerm\wezterm.exe start -- .\rolf.exe" | Out-File "start-demo.bat" -Encoding oem

# if (Test-Path -Path install-wezterm.bat) {
#     Remove-Item -Path install-wezterm.bat
# }
