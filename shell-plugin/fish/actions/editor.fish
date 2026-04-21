# Editor and command suggestion action handlers

# Action handler: Open external editor for command composition
function _forge_action_editor
    set -l initial_text "$argv[1]"
    echo

    # Determine editor in order of preference: FORGE_EDITOR > EDITOR > nano
    set -l editor_cmd "$FORGE_EDITOR"
    if test -z "$editor_cmd"
        set editor_cmd "$EDITOR"
        if test -z "$editor_cmd"
            set editor_cmd nano
        end
    end

    # Validate editor exists
    if not command -v (string split ' ' -- $editor_cmd)[1] &>/dev/null
        _forge_log error "Editor not found: $editor_cmd (set FORGE_EDITOR or EDITOR)"
        return 1
    end

    # Create .forge directory if it doesn't exist
    set -l forge_dir ".forge"
    if not test -d "$forge_dir"
        mkdir -p "$forge_dir"
        or begin
            _forge_log error "Failed to create .forge directory"
            return 1
        end
    end

    # Create temporary file with git-like naming: FORGE_EDITMSG.md
    set -l temp_file "$forge_dir/FORGE_EDITMSG.md"
    touch "$temp_file"
    or begin
        _forge_log error "Failed to create temporary file"
        return 1
    end

    # Pre-populate with initial text if provided
    if test -n "$initial_text"
        echo "$initial_text" > "$temp_file"
    end

    # Open editor in subshell with its own TTY session
    begin
        eval "$editor_cmd '$temp_file'"
    end </dev/tty >/dev/tty 2>&1
    set -l editor_exit_code $status

    if test $editor_exit_code -ne 0
        _forge_log error "Editor exited with error code $editor_exit_code"
        _forge_reset
        return 1
    end

    # Read and process content
    set -l content (cat "$temp_file" | tr -d '\r')

    if test -z "$content"
        _forge_log info "Editor closed with no content"
        commandline -r ''
        commandline -f repaint
        return 0
    end

    # Insert into command line with : prefix
    commandline -r ": $content"
    commandline -f end-of-line
    commandline -f repaint
end

# Action handler: Generate shell command from natural language
function _forge_action_suggest
    set -l description "$argv[1]"

    if test -z "$description"
        _forge_log error "Please provide a command description"
        return 0
    end

    echo

    # Generate the command
    set -l generated_command
    set generated_command (FORCE_COLOR=true CLICOLOR_FORCE=1 _forge_exec suggest "$description")

    if test -n "$generated_command"
        commandline -r "$generated_command"
        commandline -f end-of-line
        commandline -f repaint
    else
        _forge_log error "Failed to generate command"
    end
end
