# Core utility functions for forge fish plugin

# Lazy loader for commands cache
function _forge_get_commands
    if test -z "$_FORGE_COMMANDS"
        set -g _FORGE_COMMANDS (CLICOLOR_FORCE=0 $_FORGE_BIN list commands --porcelain 2>/dev/null)
    end
    echo $_FORGE_COMMANDS
end

# Private fzf function with common options for consistent UX
function _forge_fzf
    fzf --reverse --exact --cycle --select-1 --height 80% --no-scrollbar --ansi --color="header:bold" $argv
end

# Helper function to execute forge commands consistently
function _forge_exec
    set -l agent_id "$_FORGE_ACTIVE_AGENT"
    if test -z "$agent_id"
        set agent_id forge
    end
    set -l cmd $_FORGE_BIN --agent $agent_id

    if test -n "$FORGE_SHELL_PROMPT"
        set cmd $cmd --shell-prompt
    end

    # Expose terminal context lists as US-separated (\x1F) env vars so that
    # the Rust TerminalContextService can read them via get_env_var.
    if test "$_FORGE_TERM" = "true" -a (count $_FORGE_TERM_COMMANDS) -gt 0
        # Join the ring-buffer lists with the ASCII Unit Separator (\x1f).
        set -lx _FORGE_TERM_COMMANDS (string join \x1f $_FORGE_TERM_COMMANDS)
        set -lx _FORGE_TERM_EXIT_CODES (string join \x1f $_FORGE_TERM_EXIT_CODES)
        set -lx _FORGE_TERM_TIMESTAMPS (string join \x1f $_FORGE_TERM_TIMESTAMPS)
    end

    if test -n "$_FORGE_SESSION_MODEL"
        set -lx FORGE_SESSION__MODEL_ID $_FORGE_SESSION_MODEL
    end
    if test -n "$_FORGE_SESSION_PROVIDER"
        set -lx FORGE_SESSION__PROVIDER_ID $_FORGE_SESSION_PROVIDER
    end
    if test -n "$_FORGE_SESSION_REASONING_EFFORT"
        set -lx FORGE_REASONING__EFFORT $_FORGE_SESSION_REASONING_EFFORT
    end

    $cmd $argv
end

# Like _forge_exec but connects stdin/stdout to /dev/tty so that interactive
# prompts (rustyline, fzf, etc.) work correctly when forge is launched as a
# child of a key binding. Fish owns the terminal and replaces the process's
# stdin/stdout with its own pipes, so without this redirect any readline
# library would see a non-tty stdin and return EOF immediately.
# Do NOT use inside command substitutions - use _forge_exec instead.
function _forge_exec_interactive
    set -l agent_id "$_FORGE_ACTIVE_AGENT"
    if test -z "$agent_id"
        set agent_id forge
    end
    set -l cmd $_FORGE_BIN --agent $agent_id

    if test -n "$FORGE_SHELL_PROMPT"
        set cmd $cmd --shell-prompt
    end

    if test "$_FORGE_TERM" = "true" -a (count $_FORGE_TERM_COMMANDS) -gt 0
        set -lx _FORGE_TERM_COMMANDS (string join \x1f $_FORGE_TERM_COMMANDS)
        set -lx _FORGE_TERM_EXIT_CODES (string join \x1f $_FORGE_TERM_EXIT_CODES)
        set -lx _FORGE_TERM_TIMESTAMPS (string join \x1f $_FORGE_TERM_TIMESTAMPS)
    end

    if test -n "$_FORGE_SESSION_MODEL"
        set -lx FORGE_SESSION__MODEL_ID $_FORGE_SESSION_MODEL
    end
    if test -n "$_FORGE_SESSION_PROVIDER"
        set -lx FORGE_SESSION__PROVIDER_ID $_FORGE_SESSION_PROVIDER
    end
    if test -n "$_FORGE_SESSION_REASONING_EFFORT"
        set -lx FORGE_REASONING__EFFORT $_FORGE_SESSION_REASONING_EFFORT
    end

    $cmd $argv </dev/tty >/dev/tty
end

function _forge_reset
    commandline -r ''
    commandline -f repaint
end

# Helper function to find the index of a value in a list (1-based)
# Returns the index if found, 1 otherwise
function _forge_find_index
    set -l output "$argv[1]"
    set -l value_to_find "$argv[2]"
    set -l field_number "$argv[3]"
    if test -z "$field_number"
        set field_number 1
    end
    set -l field_number2 "$argv[4]"
    set -l value_to_find2 "$argv[5]"

    set -l index 1
    set -l line_num 0
    for line in (echo "$output" | string split \n)
        set line_num (math $line_num + 1)
        # Skip the header line (first line)
        if test $line_num -eq 1
            continue
        end

        set -l field_value (echo "$line" | awk -F '  +' "{print \$$field_number}")
        if test "$field_value" = "$value_to_find"
            if test -n "$field_number2" -a -n "$value_to_find2"
                set -l field_value2 (echo "$line" | awk -F '  +' "{print \$$field_number2}")
                if test "$field_value2" = "$value_to_find2"
                    echo $index
                    return 0
                end
            else
                echo $index
                return 0
            end
        end
        set index (math $index + 1)
    end

    echo 1
    return 0
end

# Helper function to print messages with consistent formatting based on log level
function _forge_log
    set -l level "$argv[1]"
    set -l message "$argv[2]"
    set -l timestamp (printf '\033[90m[%s]\033[0m' (date '+%H:%M:%S'))

    switch "$level"
        case error
            printf '\033[31m⏺\033[0m %s \033[31m%s\033[0m\n' "$timestamp" "$message"
        case info
            printf '\033[37m⏺\033[0m %s \033[37m%s\033[0m\n' "$timestamp" "$message"
        case success
            printf '\033[33m⏺\033[0m %s \033[37m%s\033[0m\n' "$timestamp" "$message"
        case warning
            printf '\033[93m⚠️\033[0m %s \033[93m%s\033[0m\n' "$timestamp" "$message"
        case debug
            printf '\033[36m⏺\033[0m %s \033[90m%s\033[0m\n' "$timestamp" "$message"
        case '*'
            echo "$message"
    end
end

# Helper function to check if a workspace is indexed
function _forge_is_workspace_indexed
    set -l workspace_path "$argv[1]"
    $_FORGE_BIN workspace info "$workspace_path" >/dev/null 2>&1
    return $status
end

# Start background sync job for current workspace if not already running
function _forge_start_background_sync
    set -l sync_enabled "$FORGE_SYNC_ENABLED"
    if test -z "$sync_enabled"
        set sync_enabled true
    end

    if test -n "$FORGE_SHELL_PROMPT"
        set -l shell_sync_enabled "$FORGE_SHELL_BEHAVIOR_SYNC"
        if test -n "$shell_sync_enabled"
            set sync_enabled "$shell_sync_enabled"
        end
    end

    if test "$sync_enabled" != "true"
        return 0
    end

    set -l workspace_path (pwd -P)

    # Check if workspace is indexed before attempting sync
    begin
        exec >/dev/null 2>&1 </dev/null
        if not _forge_is_workspace_indexed "$workspace_path"
            return 0
        end
        $_FORGE_BIN workspace sync "$workspace_path"
    end &
end

# Start background update check if not already running
function _forge_start_background_update
    if test -n "$FORGE_UPDATE_DISABLED"
        return 0
    end

    begin
        exec >/dev/null 2>&1 </dev/null
        $_FORGE_BIN update --no-confirm
    end &
end
