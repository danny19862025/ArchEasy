-- Hyprland desktop preset, adapted from Projects/dotfiles/dotfiles/hypr.
-- Lua format for Hyprland 0.55+; checked against the 0.56.2 example.
-- https://wiki.hypr.land/configuring/
-- Change these commands to choose your terminal, file manager and launcher.
local terminal = "ghostty || kitty"
local file_manager = "thunar || dolphin"
local menu = "wofi --show drun --width 500 --height 420 || rofi -show drun"

hl.monitor({ output = "", mode = "preferred", position = "auto", scale = "auto" })
hl.env("XCURSOR_SIZE", "24")
hl.env("HYPRCURSOR_SIZE", "24")

hl.config({
    general = {
        gaps_in = 5,
        gaps_out = 15,
        border_size = 2,
        layout = "dwindle",
        col = {
            active_border = { colors = { "rgba(33ccffee)", "rgba(ff99c8ee)" }, angle = 45 },
            inactive_border = "rgba(595959aa)",
        },
    },
    decoration = {
        rounding = 15,
        blur = { enabled = true, size = 8, passes = 2 },
    },
    animations = { enabled = true },
    input = {
        follow_mouse = 1,
        touchpad = {
            natural_scroll = true,
            tap_to_click = true,
            clickfinger_behavior = true,
            drag_lock = true,
        },
    },
    misc = { force_default_wallpaper = 0, disable_hyprland_logo = true },
})

hl.curve("smooth", { type = "bezier", points = { { 0.25, 0.1 }, { 0.25, 1.0 } } })
hl.animation({ leaf = "windows", enabled = true, speed = 7, bezier = "smooth" })
hl.animation({ leaf = "fade", enabled = true, speed = 7, bezier = "smooth" })
hl.animation({ leaf = "workspaces", enabled = true, speed = 6, bezier = "smooth" })

-- Each shortcut has one purpose; conflicting AGS/clipboard bindings are removed.
hl.bind("SUPER + Q", hl.dsp.exec_cmd(terminal))
hl.bind("SUPER + C", hl.dsp.window.close())
hl.bind("SUPER + E", hl.dsp.exec_cmd(file_manager))
hl.bind("SUPER + R", hl.dsp.exec_cmd(menu))
hl.bind("SUPER + SPACE", hl.dsp.exec_cmd(menu))
hl.bind("SUPER + V", hl.dsp.window.float({ action = "toggle" }))
hl.bind("SUPER + SHIFT + V", hl.dsp.exec_cmd("cliphist list | wofi --dmenu | cliphist decode | wl-copy"))
hl.bind("SUPER + M", hl.dsp.exec_cmd("command -v hyprshutdown >/dev/null 2>&1 && hyprshutdown || hyprctl dispatch 'hl.dsp.exit()'"))

for _, direction in ipairs({ "left", "right", "up", "down" }) do
    hl.bind("SUPER + " .. direction, hl.dsp.focus({ direction = direction }))
    hl.bind("SUPER + SHIFT + " .. direction, hl.dsp.window.move({ direction = direction }))
end
for workspace = 1, 10 do
    local key = workspace % 10
    hl.bind("SUPER + " .. key, hl.dsp.focus({ workspace = workspace }))
    hl.bind("SUPER + SHIFT + " .. key, hl.dsp.window.move({ workspace = workspace }))
end
hl.bind("SUPER + mouse:272", hl.dsp.window.drag(), { mouse = true })
hl.bind("SUPER + mouse:273", hl.dsp.window.resize(), { mouse = true })

hl.bind("XF86AudioRaiseVolume", hl.dsp.exec_cmd("wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+"), { locked = true, repeating = true })
hl.bind("XF86AudioLowerVolume", hl.dsp.exec_cmd("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-"), { locked = true, repeating = true })
hl.bind("XF86AudioMute", hl.dsp.exec_cmd("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"), { locked = true })
hl.bind("XF86MonBrightnessUp", hl.dsp.exec_cmd("brightnessctl set +5%"), { locked = true, repeating = true })
hl.bind("XF86MonBrightnessDown", hl.dsp.exec_cmd("brightnessctl set 5%-"), { locked = true, repeating = true })
hl.bind("PRINT", hl.dsp.exec_cmd("grim -g \"$(slurp)\" - | wl-copy"))

-- Start optional tools only when installed. No external scripts or cache files
-- are required by this preset. Wallpaper and status-bar themes can be added later.
hl.on("hyprland.start", function()
    hl.exec_cmd("command -v waybar >/dev/null 2>&1 && waybar")
    hl.exec_cmd("command -v swaync >/dev/null 2>&1 && swaync")
    hl.exec_cmd("command -v wl-paste >/dev/null 2>&1 && command -v cliphist >/dev/null 2>&1 && wl-paste --type text --watch cliphist store")
    hl.exec_cmd("command -v wl-paste >/dev/null 2>&1 && command -v cliphist >/dev/null 2>&1 && wl-paste --type image --watch cliphist store")
    hl.exec_cmd("if systemctl --user cat hyprpolkitagent.service >/dev/null 2>&1; then systemctl --user start hyprpolkitagent.service; elif test -x /usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1; then exec /usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1; fi")
end)
