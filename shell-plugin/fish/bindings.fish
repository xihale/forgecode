# Key bindings for forge fish plugin

# Bind Enter to our custom accept-line that transforms :commands
bind \r __forge_accept_line
bind \n __forge_accept_line

# Bind Tab to our custom completion widget
bind \t __forge_tab_complete

# Custom bracketed-paste handler that wraps dropped file paths in @[] syntax.
# Fish handles bracketed paste internally, but we override fish_clipboard_paste
# to add path formatting for forge commands.
function __forge_clipboard_paste --description "Paste from clipboard with forge path wrapping"
    # Get clipboard content
    set -l paste_content
    if command -v pbpaste &>/dev/null
        set paste_content (pbpaste 2>/dev/null)
    else if command -v xclip &>/dev/null
        set paste_content (xclip -selection clipboard -o 2>/dev/null)
    else if command -v wl-paste &>/dev/null
        set paste_content (wl-paste 2>/dev/null)
    end

    if test -z "$paste_content"
        return 0
    end

    # Insert the pasted content
    commandline -i -- "$paste_content"

    # Only auto-wrap when the line is a forge command (starts with ':').
    if string match -q ':*' -- (commandline -b)
        set -l buf (commandline -b)
        set -l formatted ("$_FORGE_BIN" zsh format --buffer "$buf")
        if test -n "$formatted" -a "$formatted" != "$buf"
            commandline -r -- $formatted
        end
    end

    commandline -f repaint
end

# Override the default paste binding
bind \cv __forge_clipboard_paste
