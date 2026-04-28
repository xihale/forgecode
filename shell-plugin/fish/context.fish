# Terminal context capture for forge fish plugin
#
# Provides three layers of terminal context:
# 1. preexec/postexec hooks: ring buffer of recent commands + exit codes
# 2. OSC 133 emission: semantic terminal markers for compatible terminals
# 3. Terminal-specific output capture: Kitty > WezTerm > tmux

# OSC 133 helpers

# Cache for OSC 133 detection result
set -g _FORGE_TERM_OSC133_CACHED ""

function _forge_osc133_should_emit
    if test -n "$_FORGE_TERM_OSC133_CACHED"
        test "$_FORGE_TERM_OSC133_CACHED" = "1"
        and return 0
        or return 1
    end
    switch "$_FORGE_TERM_OSC133"
        case on
            set -g _FORGE_TERM_OSC133_CACHED 1
            return 0
        case off
            set -g _FORGE_TERM_OSC133_CACHED 0
            return 1
        case auto
            # Kitty sets KITTY_PID
            if test -n "$KITTY_PID"
                set -g _FORGE_TERM_OSC133_CACHED 1
                return 0
            end
            # Detect by TERM_PROGRAM
            switch "$TERM_PROGRAM"
                case WezTerm iTerm.app vscode
                    set -g _FORGE_TERM_OSC133_CACHED 1
                    return 0
            end
            # Foot terminal
            if string match -q 'foot*' -- $TERM
                set -g _FORGE_TERM_OSC133_CACHED 1
                return 0
            end
            # Ghostty
            if test "$TERM_PROGRAM" = "ghostty"
                set -g _FORGE_TERM_OSC133_CACHED 1
                return 0
            end
            # Unknown terminal: don't emit
            set -g _FORGE_TERM_OSC133_CACHED 0
            return 1
        case '*'
            set -g _FORGE_TERM_OSC133_CACHED 0
            return 1
    end
end

# Emits an OSC 133 marker if the terminal supports it
function _forge_osc133_emit
    _forge_osc133_should_emit
    or return 0
    printf '\e]133;%s\a' "$argv[1]"
end

# Ring buffer storage uses parallel lists declared in config.fish
# Pending command state:
set -g _FORGE_TERM_PENDING_CMD ""
set -g _FORGE_TERM_PENDING_TS ""

# Called before each command executes.
function __forge_context_preexec --on-event fish_preexec
    test "$_FORGE_TERM" != "true"
    and return
    set -g _FORGE_TERM_PENDING_CMD "$argv[1]"
    set -g _FORGE_TERM_PENDING_TS (date +%s)
    # OSC 133 B: prompt end / command start
    _forge_osc133_emit "B"
    # OSC 133 C: command output start
    _forge_osc133_emit "C"
end

# Called after each command completes, before the next prompt is drawn.
function __forge_context_postexec --on-event fish_postexec
    set -l last_exit $status

    # OSC 133 D: command finished with exit code.
    _forge_osc133_emit "D;$last_exit"

    test "$_FORGE_TERM" != "true"
    and return

    # Only record if we have a pending command from preexec
    if test -n "$_FORGE_TERM_PENDING_CMD"
        set -a _FORGE_TERM_COMMANDS "$_FORGE_TERM_PENDING_CMD"
        set -a _FORGE_TERM_EXIT_CODES "$last_exit"
        set -a _FORGE_TERM_TIMESTAMPS "$_FORGE_TERM_PENDING_TS"

        # Trim ring buffer to max size
        while test (count $_FORGE_TERM_COMMANDS) -gt $_FORGE_TERM_MAX_COMMANDS
            set -e _FORGE_TERM_COMMANDS[1]
            set -e _FORGE_TERM_EXIT_CODES[1]
            set -e _FORGE_TERM_TIMESTAMPS[1]
        end

        set -g _FORGE_TERM_PENDING_CMD ""
        set -g _FORGE_TERM_PENDING_TS ""
    end

    # OSC 133 A: prompt start (for the next prompt)
    _forge_osc133_emit "A"
end
