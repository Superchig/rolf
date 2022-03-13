local wezterm = require('wezterm');

return {
  color_scheme = "Gruvbox Dark",
  default_prog = { "powershell.exe" },
  hide_tab_bar_if_only_one_tab = true,
  initial_cols = 160,
  initial_rows = 48,
  enable_kitty_graphics = true,
  warn_about_missing_glyphs = false,

  keys = {
    {key="t", mods="ALT", action=wezterm.action{SpawnTab="CurrentPaneDomain"}},
    {key="h", mods="ALT", action=wezterm.action{ActivatePaneDirection="Left"}},
    {key="l", mods="ALT", action=wezterm.action{ActivatePaneDirection="Right"}},
    {key="j", mods="ALT", action=wezterm.action{ActivatePaneDirection="Down"}},
    {key="k", mods="ALT", action=wezterm.action{ActivatePaneDirection="Up"}},
    {key="1", mods="ALT", action=wezterm.action{ActivateTab=0}},
    {key="2", mods="ALT", action=wezterm.action{ActivateTab=1}},
    {key="3", mods="ALT", action=wezterm.action{ActivateTab=2}},
    {key="4", mods="ALT", action=wezterm.action{ActivateTab=3}},
    {key="5", mods="ALT", action=wezterm.action{ActivateTab=4}},
    {key="6", mods="ALT", action=wezterm.action{ActivateTab=5}},
    {key="7", mods="ALT", action=wezterm.action{ActivateTab=6}},
    {key="8", mods="ALT", action=wezterm.action{ActivateTab=7}},
    {key="9", mods="ALT", action=wezterm.action{ActivateTab=8}},
    {key="0", mods="ALT", action=wezterm.action{ActivateTab=9}},
    {key="H", mods="CTRL", action=wezterm.action{ActivateTabRelative=-1}},
    {key="L", mods="CTRL", action=wezterm.action{ActivateTabRelative=1}},
    {key="Enter", mods="ALT|SHIFT", action=wezterm.action{SplitVertical={domain="CurrentPaneDomain"}}},
    {key="Enter", mods="ALT", action=wezterm.action{SplitHorizontal={domain="CurrentPaneDomain"}}},
    {key="\"", mods="CTRL|ALT", action=wezterm.action{SplitVertical={domain="CurrentPaneDomain"}}},
    {key="%", mods="CTRL|ALT", action=wezterm.action{SplitHorizontal={domain="CurrentPaneDomain"}}}
  },
}
