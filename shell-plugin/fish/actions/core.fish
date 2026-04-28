# Core action handlers for basic forge operations

# Action handler: Start a new conversation
function _forge_action_new
    set -l input_text "$argv[1]"

    # Clear conversation and save as previous (like cd -)
    _forge_clear_conversation
    set -g _FORGE_ACTIVE_AGENT "forge"

    echo

    # If input_text is provided, send it to the new conversation
    if test -n "$input_text"
        # Generate new conversation ID and switch to it
        set -l new_id ($_FORGE_BIN conversation new)
        _forge_switch_conversation "$new_id"

        # Execute the forge command with the input text
        _forge_exec_interactive -p "$input_text" --cid "$_FORGE_CONVERSATION_ID"

        # Start background sync job if enabled and not already running
        _forge_start_background_sync
        # Start background update check
        _forge_start_background_update
    else
        # Only show banner if no input text (starting fresh conversation)
        _forge_exec banner
    end
end

# Action handler: Show session info
function _forge_action_info
    echo
    if test -n "$_FORGE_CONVERSATION_ID"
        _forge_exec info --cid "$_FORGE_CONVERSATION_ID"
    else
        _forge_exec info
    end
end

# Action handler: Dump conversation
function _forge_action_dump
    set -l input_text "$argv[1]"
    if test "$input_text" = "html"
        _forge_handle_conversation_command "dump" "--html"
    else
        _forge_handle_conversation_command "dump"
    end
end

# Action handler: Compact conversation
function _forge_action_compact
    _forge_handle_conversation_command "compact"
end

# Action handler: Retry last message
function _forge_action_retry
    _forge_handle_conversation_command "retry"
end

# Action handler: Show available commands (mirrors :help in the REPL)
function _forge_action_help
    echo
    $_FORGE_BIN list command
end

# Helper function to handle conversation commands that require an active conversation
function _forge_handle_conversation_command
    set -l subcommand "$argv[1]"
    # Remaining args become extra parameters
    set -l extra_args $argv[2..-1]

    echo

    # Check if FORGE_CONVERSATION_ID is set
    if test -z "$_FORGE_CONVERSATION_ID"
        _forge_log error "No active conversation. Start a conversation first or use :conversation to see existing ones"
        return 0
    end

    # Execute the conversation command with conversation ID and any extra arguments
    _forge_exec conversation "$subcommand" "$_FORGE_CONVERSATION_ID" $extra_args
end

# Action handler: Paste image from clipboard
function _forge_action_paste_image
    set -l temp_path ($_FORGE_BIN clipboard paste-image 2>/dev/null)
    if test -n "$temp_path"
        commandline -r ":@[$temp_path] "
    end
    commandline -f repaint
end
