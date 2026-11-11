# Records the README hero GIF.
#
#   ./scripts/demo.sh --record
#
# Requires VHS (https://github.com/charmbracelet/vhs), ttyd and ffmpeg, plus the
# Nerd Font named below. Everything happens in /tmp/kemori-demo, never in this
# repository.

Set Shell bash
Set FontFamily "CaskaydiaCove Nerd Font Mono"
Set FontSize 15
Set Width 1420
Set Height 760
Set Padding 14
Set MarginFill "#08070c"
Set BorderRadius 10
Set TypingSpeed 40ms
Set Framerate 30
Set Theme { "name": "kemori", "black": "#16131d", "red": "#ea83a5", "green": "#90b99f", "yellow": "#e6b99d", "blue": "#aca1cf", "magenta": "#b9aeda", "cyan": "#9fd6cf", "white": "#f2eef7", "brightBlack": "#5a5368", "brightRed": "#f29cb8", "brightGreen": "#a8cfb4", "brightYellow": "#f0cbb3", "brightBlue": "#c4b9e0", "brightMagenta": "#cdc3ea", "brightCyan": "#b3e3dc", "brightWhite": "#ffffff", "background": "#0b0910", "foreground": "#e7e1ee", "selection": "#2a2238", "cursor": "#e29eca" }

Env HOME "/tmp/kemori-demo/home"
Env XDG_CONFIG_HOME "/tmp/kemori-demo/home/.config"
Env XDG_DATA_HOME "/tmp/kemori-demo/home/.local/share"
Env XDG_STATE_HOME "/tmp/kemori-demo/home/.local/state"
Env XDG_CACHE_HOME "/tmp/kemori-demo/home/.cache"

Output docs/images/demo.gif

Hide
Type@1ms "cd /tmp/kemori-demo/aurora && kemori ."
Enter
Sleep 1200ms
Show

# Kemori notices the missing language server and offers to install it.
Sleep 1200ms

# Not yet — look at the home screen first.
Escape
Sleep 800ms

# Cycle the welcome scene.
Type@120ms "v"
Sleep 2400ms

# Quick open a file.
Ctrl+P
Sleep 700ms
Type@70ms "main"
Sleep 700ms
Enter
Sleep 1600ms

# Dismiss the project-setup offer and let the editor sit.
Escape
Sleep 600ms

Sleep 3200ms
Ctrl+Q
Sleep 400ms
