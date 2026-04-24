#!/usr/bin/env zsh

# Configuration action handlers (agent, provider, model, tools, skill)

# Action handler: Select agent
function _forge_action_agent() {
    local input_text="$1"
    
    echo
    
    # If an agent ID is provided directly, use it
    if [[ -n "$input_text" ]]; then
        local agent_id="$input_text"
        
        # Validate that the agent exists (skip header line)
        local agent_exists=$($_FORGE_BIN list agents --porcelain 2>/dev/null | tail -n +2 | grep -q "^${agent_id}\b" && echo "true" || echo "false")
        if [[ "$agent_exists" == "false" ]]; then
            _forge_log error "Agent '\033[1m${agent_id}\033[0m' not found"
            return 0
        fi
        
        # Set the agent as active
        _FORGE_ACTIVE_AGENT="$agent_id"
        
        # Print log about agent switching
        _forge_log success "Switched to agent \033[1m${agent_id}\033[0m"
        
        return 0
    fi
    
    # Use forge select agent for interactive picking
    local agent_id
    agent_id=$(_forge_select_with_query "$input_text" agent)
    
    if [[ -n "$agent_id" ]]; then
        _FORGE_ACTIVE_AGENT="$agent_id"
        _forge_log success "Switched to agent \033[1m${agent_id}\033[0m"
    fi
}

# Helper: Open an fzf model picker and print the raw selected line.
#
# Model list columns (from `forge list models --porcelain`):
#   1:model_id  2:model_name  3:provider(display)  4:provider_id(raw)  5:context  6:tools  7:image
# The picker hides model_id (field 1) and provider_id (field 4) via --with-nth.
#
# Arguments:
#   $1  prompt_text      - fzf prompt label (e.g. "Model ❯ ")
#   $2  current_model    - model_id to pre-position the cursor on (may be empty)
#   $3  input_text       - optional pre-fill query for fzf
#   $4  current_provider - provider value to disambiguate when model names collide (may be empty)
#   $5  provider_field   - which porcelain field to match the provider against
#                          (3 for display name, 4 for raw id)
#
# Outputs the raw selected line to stdout, or nothing if cancelled.
function _forge_pick_model() {
    local prompt_text="$1"
    local current_model="$2"
    local input_text="$3"
    local current_provider="${4:-}"
    local provider_field="${5:-}"

    local raw_output output
    raw_output=$(CLICOLOR_FORCE=0 NO_COLOR=1 TERM=dumb $_FORGE_BIN list models --porcelain </dev/null 2>/dev/null)
    output=$(printf '%s\n' "$raw_output" | tr '\r' '\n' | awk 'BEGIN { seen = 0 } /^ID[[:space:]]+MODEL[[:space:]]+PROVIDER/ { seen = 1 } seen { print }')

    if [[ -z "$output" && -n "$raw_output" ]]; then
        output="$raw_output"
    fi

    if [[ -z "$output" ]]; then
        return 1
    fi

    local fzf_args=(
        --delimiter="$_FORGE_DELIMITER"
        --prompt="$prompt_text"
        --with-nth="2,3,5.."
    )

    if [[ -n "$input_text" ]]; then
        fzf_args+=(--query="$input_text")
    fi

    if [[ -n "$current_model" ]]; then
        # Match on both model_id (field 1) and provider to disambiguate
        # when the same model name exists across multiple providers
        local index
        if [[ -n "$current_provider" && -n "$provider_field" ]]; then
            index=$(_forge_find_index "$output" "$current_model" 1 "$provider_field" "$current_provider")
        else
            index=$(_forge_find_index "$output" "$current_model" 1)
        fi
        fzf_args+=(--bind="start:pos($index)")
    fi

    printf '%s\n' "$output" | _forge_fzf --header-lines=1 "${fzf_args[@]}"
}

# Action handler: Select model (across all configured providers)
# When the selected model belongs to a different provider, switches it first.
function _forge_action_model() {
    local input_text="$1"
    echo

    local model_id provider_id
    if _forge_select_model_pair_global "$input_text"; then
        model_id="${reply[1]}"
        provider_id="${reply[2]}"
        _forge_exec config set model "$provider_id" "$model_id"
    fi
}

# Action handler: Select model for shell mode.
# Persists to config via `forge config set shell` and sets session variables
# so the current terminal session uses the new model immediately.
function _forge_action_shell_model() {
    local input_text="$1"
    echo

    local selected
    selected=$(_forge_pick_model "Shell Model ❯ " "" "$input_text")

    if [[ -n "$selected" ]]; then
        # Field 1 = model_id (raw), field 4 = provider_id (raw)
        local model_id provider_id
        # Extract fields separately to handle display names with spaces
        model_id=$(echo "$selected" | awk -F '  +' '{print $1}')
        provider_id=$(echo "$selected" | awk -F '  +' '{print $4}')

        model_id=${model_id//[[:space:]]/}
        provider_id=${provider_id//[[:space:]]/}

        _FORGE_SESSION_MODEL="$model_id"
        _FORGE_SESSION_PROVIDER="$provider_id"

        _forge_exec config set shell "$provider_id" "$model_id"
    fi
}

# Action handler: Select model for commit message generation
# Calls `forge config set commit <provider_id> <model_id>` on selection.
function _forge_action_commit_model() {
    local input_text="$1"
    echo

    local model_id provider_id
    if _forge_select_model_pair "$input_text"; then
        model_id="${reply[1]}"
        provider_id="${reply[2]}"
        _forge_exec config set commit "$provider_id" "$model_id"
    fi
}

# Action handler: Select model for command suggestion generation
# Calls `forge config set suggest <provider_id> <model_id>` on selection.
function _forge_action_suggest_model() {
    local input_text="$1"
    echo

    local model_id provider_id
    if _forge_select_model_pair "$input_text"; then
        model_id="${reply[1]}"
        provider_id="${reply[2]}"
        _forge_exec config set suggest "$provider_id" "$model_id"
    fi
}

# Action handler: Sync workspace for codebase search
function _forge_action_sync() {
    echo
    # Use _forge_exec_interactive so that the consent prompt (and any other
    # interactive prompts) can access /dev/tty even though ZLE owns the
    # terminal's stdin/stdout pipes.
    # --init initializes the workspace first if it has not been set up yet
    _forge_exec_interactive workspace sync --init
}

# Action handler: inits workspace for codebase search
function _forge_action_sync_init() {
    echo
    # Use _forge_exec_interactive so that the consent prompt can access /dev/tty
    _forge_exec_interactive workspace init
}

# Action handler: Show sync status of workspace files
function _forge_action_sync_status() {
    echo
    _forge_exec workspace status "."
}

# Action handler: Show workspace info with sync details
function _forge_action_sync_info() {
    echo
    _forge_exec workspace info "."
}

# Action handler: Select model for the current session only.
# Sets _FORGE_SESSION_MODEL and _FORGE_SESSION_PROVIDER in the shell environment
# so that every subsequent forge invocation uses those values via --model /
# --provider flags without touching the permanent global configuration.
function _forge_action_session_model() {
    local input_text="$1"
    echo

    local current_model current_provider provider_index
    # Use session overrides as the starting selection if already set,
    # otherwise fall back to the globally configured values.
    if [[ -n "$_FORGE_SESSION_MODEL" ]]; then
        current_model="$_FORGE_SESSION_MODEL"
        provider_index=4
    else
        current_model=$($_FORGE_BIN config get model 2>/dev/null)
        provider_index=3
    fi
    if [[ -n "$_FORGE_SESSION_PROVIDER" ]]; then
        current_provider="$_FORGE_SESSION_PROVIDER"
        provider_index=4
    else
        current_provider=$($_FORGE_BIN config get provider 2>/dev/null)
        provider_index=3
    fi

    local selected
    selected=$(_forge_pick_model "Session Model ❯ " "$current_model" "$input_text" "$current_provider" "$provider_index")

    if [[ -n "$selected" ]]; then
        local model_id provider_display provider_id
        # Extract fields separately to handle display names with spaces
        model_id=$(echo "$selected" | awk -F '  +' '{print $1}')
        provider_display=$(echo "$selected" | awk -F '  +' '{print $3}')
        provider_id=$(echo "$selected" | awk -F '  +' '{print $4}')
        model_id=${model_id//[[:space:]]/}
        provider_id=${provider_id//[[:space:]]/}

        _FORGE_SESSION_MODEL="$model_id"
        _FORGE_SESSION_PROVIDER="$provider_id"

        _forge_exec config set model "$provider_id" "$model_id"

        _forge_log success "Session model set to \033[1m${model_id}\033[0m (provider: \033[1m${provider_id}\033[0m)"
}

# Action handler: Reload config by resetting all session-scoped overrides.
# Clears _FORGE_SESSION_MODEL, _FORGE_SESSION_PROVIDER, and
# _FORGE_SESSION_REASONING_EFFORT so that every subsequent forge invocation
# falls back to the permanent global configuration.
function _forge_action_config_reload() {
    echo

    if [[ -z "$_FORGE_SESSION_MODEL" && -z "$_FORGE_SESSION_PROVIDER" && -z "$_FORGE_SESSION_REASONING_EFFORT" ]]; then
        _forge_log info "No session overrides active (already using global config)"
        return 0
    fi

    _FORGE_SESSION_MODEL=""
    _FORGE_SESSION_PROVIDER=""
    _FORGE_SESSION_REASONING_EFFORT=""

    _forge_log success "Session overrides cleared — using global config"
}

# Action handler: Select reasoning effort for the current session only.
# Sets _FORGE_SESSION_REASONING_EFFORT in the shell environment so that
# every subsequent forge invocation uses the selected value via the
# FORGE_REASONING__EFFORT env var without modifying the permanent config.
function _forge_action_reasoning_effort() {
    local input_text="$1"
    echo

    local selected
    selected=$(_forge_select_with_query "$input_text" reasoning-effort)

    if [[ -n "$selected" ]]; then
        _FORGE_SESSION_REASONING_EFFORT="$selected"
        _forge_log success "Session reasoning effort set to \033[1m${selected}\033[0m"
    fi
}

# Action handler: Set reasoning effort in global config.
# Calls `forge config set reasoning-effort <effort>` on selection,
# writing the chosen effort level permanently to ~/forge/.forge.toml.
function _forge_action_config_reasoning_effort() {
    local input_text="$1"
    echo

    local selected
    selected=$(_forge_select_with_query "$input_text" reasoning-effort)

    if [[ -n "$selected" ]]; then
        _forge_exec config set reasoning-effort "$selected"
    fi
}

# Action handler: Show config list
function _forge_action_config() {
    echo
    _forge_exec config list
}

# Action handler: Open the global forge config file in an editor
function _forge_action_config_edit() {
    echo

    # Determine editor in order of preference: FORGE_EDITOR > EDITOR > nano
    local editor_cmd="${FORGE_EDITOR:-${EDITOR:-nano}}"

    # Validate editor exists
    if ! command -v "${editor_cmd%% *}" &>/dev/null; then
        _forge_log error "Editor not found: $editor_cmd (set FORGE_EDITOR or EDITOR)"
        return 1
    fi

    # Resolve config file path via the forge binary (honours FORGE_CONFIG,
    # new ~/.forge path, and legacy ~/forge fallback automatically)
    local config_file
    config_file=$($_FORGE_BIN config path 2>/dev/null)
    if [[ -z "$config_file" ]]; then
        _forge_log error "Failed to resolve config path from '$_FORGE_BIN config path'"
        return 1
    fi

    local config_dir
    config_dir=$(dirname "$config_file")

    # Ensure the config directory exists
    if [[ ! -d "$config_dir" ]]; then
        mkdir -p "$config_dir" || {
            _forge_log error "Failed to create $config_dir directory"
            return 1
        }
    fi

    # Create the config file if it does not yet exist
    if [[ ! -f "$config_file" ]]; then
        touch "$config_file" || {
            _forge_log error "Failed to create $config_file"
            return 1
        }
    fi

    # Open editor with its own TTY session
    (eval "$editor_cmd '$config_file'" </dev/tty >/dev/tty 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        _forge_log error "Editor exited with error code $exit_code"
    fi

    _forge_reset
}

# Action handler: Show tools
function _forge_action_tools() {
    echo
    # Ensure FORGE_ACTIVE_AGENT always has a value, default to "forge"
    local agent_id="${_FORGE_ACTIVE_AGENT:-forge}"
    _forge_exec list tools "$agent_id"
}

# Action handler: Show skills
function _forge_action_skill() {
    echo
    _forge_exec list skill
}
