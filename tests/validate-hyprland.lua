-- Evaluate the preset against a non-executing API stub. This checks Lua syntax,
-- duplicate shortcuts, expected bindings and preset structure, not compositor behavior.
local bindings, callbacks, settings = {}, {}, {}
local function dispatcher(kind)
    return function(args) return { kind = kind, args = args } end
end
hl = {
    monitor = function(options) assert(options.mode == "preferred") end,
    env = function(name, value) assert(type(name) == "string" and type(value) == "string") end,
    config = function(options) settings = options end,
    curve = function(name, options) assert(name == "smooth" and options.type == "bezier") end,
    animation = function(options) assert(options.bezier == "smooth") end,
    bind = function(key, action)
        assert(not bindings[key], "Duplicate shortcut: " .. key)
        assert(type(action) == "table")
        bindings[key] = action
    end,
    on = function(event, callback) assert(event == "hyprland.start"); callbacks[event] = callback end,
    exec_cmd = function() error("Autostart must not execute while validating") end,
    dsp = {
        exec_cmd = dispatcher("exec"),
        focus = dispatcher("focus"),
        window = {
            close = dispatcher("close"), float = dispatcher("float"),
            move = dispatcher("move"), drag = dispatcher("drag"), resize = dispatcher("resize"),
        },
    },
}
dofile(assert(arg[1], "Pass the preset path"))
assert(settings.general.gaps_in == 5 and settings.general.gaps_out == 15)
assert(settings.decoration.rounding == 15 and settings.decoration.blur.passes == 2)
assert(settings.input.touchpad.natural_scroll)
assert(bindings["SUPER + V"].kind == "float")
assert(bindings["SUPER + SHIFT + V"].kind == "exec")
assert(bindings["SUPER + SHIFT + right"].kind == "move")
for i = 1, 10 do
    assert(bindings["SUPER + " .. i % 10].args.workspace == i)
    assert(bindings["SUPER + SHIFT + " .. i % 10].args.workspace == i)
end
assert(callbacks["hyprland.start"])
local count = 0
for _ in pairs(bindings) do count = count + 1 end
assert(count == 44, "Expected 44 unique shortcuts, got " .. count)
print("Hyprland Lua preset: 44 unique shortcuts and settings validated without starting a compositor")
