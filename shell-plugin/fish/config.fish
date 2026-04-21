# Configuration variables for forge fish plugin
# Using set -g to keep variables global but scoped to plugin

set -g _FORGE_BIN "$FORGE_BIN"
if test -z "$_FORGE_BIN"
    set -g _FORGE_BIN forge
end
set -g _FORGE_CONVERSATION_PATTERN ":"
set -g _FORGE_MAX_COMMIT_DIFF "$FORGE_MAX_COMMIT_DIFF"
if test -z "$_FORGE_MAX_COMMIT_DIFF"
    set -g _FORGE_MAX_COMMIT_DIFF 100000
end
set -g _FORGE_DELIMITER '\s\s+'
set -g _FORGE_PREVIEW_WINDOW "--preview-window=bottom:75%:wrap:border-sharp"

# Detect bat command - use bat if available, otherwise fall back to cat
if command -v bat &>/dev/null
    set -g _FORGE_CAT_CMD "bat --color=always --style=numbers,changes --line-range=:500"
else
    set -g _FORGE_CAT_CMD "cat"
end

# Commands cache - loaded lazily on first use
set -g _FORGE_COMMANDS ""

# Hidden variables to be used only via the ForgeCLI
set -g _FORGE_CONVERSATION_ID
set -g _FORGE_ACTIVE_AGENT

# Previous conversation ID for :conversation - (like cd -)
set -g _FORGE_PREVIOUS_CONVERSATION_ID

# Session-scoped model and provider overrides (set via :model / :m).
set -g _FORGE_SESSION_MODEL
set -g _FORGE_SESSION_PROVIDER

# Session-scoped reasoning effort override (set via :reasoning-effort / :re).
set -g _FORGE_SESSION_REASONING_EFFORT

# Terminal context capture settings
set -g _FORGE_TERM "$FORGE_TERM"
if test -z "$_FORGE_TERM"
    set -g _FORGE_TERM true
end
set -g _FORGE_TERM_MAX_COMMANDS "$FORGE_TERM_MAX_COMMANDS"
if test -z "$_FORGE_TERM_MAX_COMMANDS"
    set -g _FORGE_TERM_MAX_COMMANDS 5
end
set -g _FORGE_TERM_OSC133 "$FORGE_TERM_OSC133"
if test -z "$_FORGE_TERM_OSC133"
    set -g _FORGE_TERM_OSC133 auto
end

# Ring buffer lists for context capture
set -g _FORGE_TERM_COMMANDS
set -g _FORGE_TERM_EXIT_CODES
set -g _FORGE_TERM_TIMESTAMPS
