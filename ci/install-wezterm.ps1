$WEZTERM_VERSION = "20220101-133340-7edc5b5a"
$DOWNLOAD_URL = "https://github.com/wez/wezterm/releases/download/$WEZTERM_VERSION/WezTerm-windows-$WEZTERM_VERSION.zip"

if (-not (Test-Path -Path WezTerm.zip)) {
    Invoke-WebRequest -Uri $DOWNLOAD_URL -OutFile WezTerm.zip
}

if (-not (Test-Path -Path WezTerm-extracted)) {
    Expand-Archive -Path WezTerm.zip -DestinationPath WezTerm-extracted
}

$EXTRACTED = Get-ChildItem -Path WezTerm-extracted
if ($EXTRACTED.length -eq 1) {
    Move-Item -Path "WezTerm-Extracted\WezTerm-windows-$WEZTERM_VERSION" -Destination WezTerm
    Remove-Item -Recurse WezTerm-extracted
} else {
    Move-Item WezTerm-extracted WezTerm
}

if (Test-Path -Path wezterm.lua) {
    Move-Item -Path wezterm.lua -Destination WezTerm
}
