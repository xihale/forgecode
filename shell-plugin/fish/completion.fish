# Custom completion handler that handles both :commands and @ completion

function __forge_tab_complete
    set -l cmd (commandline -ct)
    set -l full_buffer (commandline -b)

    # Handle @ completion (files and directories)
    if string match -q '@*' -- $cmd
        set -l filter_text (string replace '@' '' -- $cmd)
        set -l fzf_args --preview="if test -d {}; ls -la --color=always {} 2>/dev/null; or ls -la {}; else; $_FORGE_CAT_CMD {}; end" $_FORGE_PREVIEW_WINDOW

        set -l file_list ($_FORGE_BIN list files --porcelain)
        set -l selected
        if test -n "$filter_text"
            set selected (echo "$file_list" | _forge_fzf --query "$filter_text" $fzf_args)
        else
            set selected (echo "$file_list" | _forge_fzf $fzf_args)
        end

        if test -n "$selected"
            set selected "@[$selected]"
            # Replace the current word with the selected file
            commandline -t "$selected"
        end

        commandline -f repaint
        return 0
    end

    # Handle :command completion
    if string match -qr '^:[a-zA-Z0-9_-]*$' -- $full_buffer
        # Extract the text after the colon for filtering
        set -l filter_text (string replace ':' '' -- $full_buffer)

        # Lazily load the commands list
        set -l commands_list (_forge_get_commands)
        if test -n "$commands_list"
            set -l selected
            if test -n "$filter_text"
                set selected (echo "$commands_list" | _forge_fzf --header-lines=1 --delimiter="$_FORGE_DELIMITER" --nth=1 --query "$filter_text" --prompt="Command ❯ ")
            else
                set selected (echo "$commands_list" | _forge_fzf --header-lines=1 --delimiter="$_FORGE_DELIMITER" --nth=1 --prompt="Command ❯ ")
            end

            if test -n "$selected"
                # Extract just the command name (first word before any description)
                set -l command_name (echo "$selected" | string replace -r ' .*' '')
                # Replace the current buffer with the selected command
                commandline -r ":$command_name "
            end
        end

        commandline -f repaint
        return 0
    end

    # Fall back to default completion
    commandline -f complete
end
