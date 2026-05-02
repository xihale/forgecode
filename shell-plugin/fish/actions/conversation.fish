# Conversation management action handlers

# Helper function to switch to a conversation and track previous (like cd -)
function _forge_switch_conversation
    set -l new_conversation_id "$argv[1]"

    # Only update previous if we're switching to a different conversation
    if test -n "$_FORGE_CONVERSATION_ID" -a "$_FORGE_CONVERSATION_ID" != "$new_conversation_id"
        set -g _FORGE_PREVIOUS_CONVERSATION_ID "$_FORGE_CONVERSATION_ID"
    end

    # Set the new conversation as active
    set -g _FORGE_CONVERSATION_ID "$new_conversation_id"
end

# Helper function to reset/clear conversation and track previous (like cd -)
function _forge_clear_conversation
    if test -n "$_FORGE_CONVERSATION_ID"
        set -g _FORGE_PREVIOUS_CONVERSATION_ID "$_FORGE_CONVERSATION_ID"
    end
    set -g _FORGE_CONVERSATION_ID ""
end

# Action handler: List/switch conversations
function _forge_action_conversation
    set -l input_text "$argv[1]"

    echo

    # Handle toggling to previous conversation (like cd -)
    if test "$input_text" = "-"
        if test -z "$_FORGE_PREVIOUS_CONVERSATION_ID"
            set input_text ""
        else
            # Swap current and previous
            set -l temp "$_FORGE_CONVERSATION_ID"
            set -g _FORGE_CONVERSATION_ID "$_FORGE_PREVIOUS_CONVERSATION_ID"
            set -g _FORGE_PREVIOUS_CONVERSATION_ID "$temp"

            echo
            _forge_exec conversation show "$_FORGE_CONVERSATION_ID"
            _forge_exec conversation info "$_FORGE_CONVERSATION_ID"

            _forge_log success "Switched to conversation $_FORGE_CONVERSATION_ID"
            return 0
        end
    end

    # If an ID is provided directly, use it
    if test -n "$input_text"
        set -l conversation_id "$input_text"
        _forge_switch_conversation "$conversation_id"

        echo
        _forge_exec conversation show "$conversation_id"
        _forge_exec conversation info "$conversation_id"

        _forge_log success "Switched to conversation $conversation_id"
        return 0
    end

    # Get conversations list
    set -l conversations_output
    set conversations_output ($_FORGE_BIN conversation list --porcelain 2>/dev/null)

    if test -n "$conversations_output"
        set -l current_id "$_FORGE_CONVERSATION_ID"

        set -l prompt_text "Conversation ❯ "
        set -l fzf_args --prompt="$prompt_text" --delimiter="$_FORGE_DELIMITER" --with-nth="2,3" --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}" $_FORGE_PREVIEW_WINDOW

        if test -n "$current_id"
            set -l index (_forge_find_index "$conversations_output" "$current_id" 1)
            set fzf_args $fzf_args --bind="start:pos($index)"
        end

        set -l selected_conversation
        set selected_conversation (echo "$conversations_output" | _forge_fzf --header-lines=1 $fzf_args)

        if test -n "$selected_conversation"
            set -l conversation_id (echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')
            _forge_switch_conversation "$conversation_id"

            echo
            _forge_exec conversation show "$conversation_id"
            _forge_exec conversation info "$conversation_id"

            _forge_log success "Switched to conversation $conversation_id"
        end
    else
        _forge_log error "No conversations found"
    end
end

# Action handler: Clone conversation
function _forge_action_clone
    set -l input_text "$argv[1]"
    set -l clone_target "$input_text"

    echo

    if test -n "$clone_target"
        _forge_clone_and_switch "$clone_target"
        return 0
    end

    set -l conversations_output
    set conversations_output ($_FORGE_BIN conversation list --porcelain 2>/dev/null)

    if test -z "$conversations_output"
        _forge_log error "No conversations found"
        return 0
    end

    set -l current_id "$_FORGE_CONVERSATION_ID"

    set -l prompt_text "Clone Conversation ❯ "
    set -l fzf_args --prompt="$prompt_text" --delimiter="$_FORGE_DELIMITER" --with-nth="2,3" --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}" $_FORGE_PREVIEW_WINDOW

    if test -n "$current_id"
        set -l index (_forge_find_index "$conversations_output" "$current_id")
        set fzf_args $fzf_args --bind="start:pos($index)"
    end

    set -l selected_conversation
    set selected_conversation (echo "$conversations_output" | _forge_fzf --header-lines=1 $fzf_args)

    if test -n "$selected_conversation"
        set -l conversation_id (echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')
        _forge_clone_and_switch "$conversation_id"
    end
end

# Action handler: Copy last assistant message to OS clipboard
function _forge_action_copy
    echo

    if test -z "$_FORGE_CONVERSATION_ID"
        _forge_log error "No active conversation. Start a conversation first or use :conversation to see existing ones"
        return 0
    end

    set -l content
    set content ($_FORGE_BIN conversation show --md "$_FORGE_CONVERSATION_ID" 2>/dev/null)

    if test -z "$content"
        _forge_log error "No assistant message found in the current conversation"
        return 0
    end

    # Copy to clipboard
    if command -v pbcopy &>/dev/null
        echo -n "$content" | pbcopy
    else if command -v xclip &>/dev/null
        echo -n "$content" | xclip -selection clipboard
    else if command -v xsel &>/dev/null
        echo -n "$content" | xsel --clipboard --input
    else
        _forge_log error "No clipboard utility found (pbcopy, xclip, or xsel required)"
        return 0
    end

    set -l line_count (echo "$content" | wc -l | string trim)
    set -l byte_count (echo -n "$content" | wc -c | string trim)

    _forge_log success "Copied to clipboard [$line_count lines, $byte_count bytes]"
end

# Action handler: Rename current conversation
function _forge_action_rename
    set -l input_text "$argv[1]"

    echo

    if test -z "$_FORGE_CONVERSATION_ID"
        _forge_log error "No active conversation. Start a conversation first or use :conversation to select one"
        return 0
    end

    if test -z "$input_text"
        _forge_log error "Usage: :rename <name>"
        return 0
    end

    _forge_exec conversation rename "$_FORGE_CONVERSATION_ID" $input_text
end

# Action handler: Rename a conversation (interactive picker or by ID)
function _forge_action_conversation_rename
    set -l input_text "$argv[1]"

    echo

    if test -n "$input_text"
        set -l conversation_id (string replace -r ' .*' '' -- $input_text)
        set -l new_name (string replace -r '^\S+\s+' '' -- $input_text)

        if test "$conversation_id" = "$new_name"
            _forge_log error "Usage: :conversation-rename <id> <name>"
            return 0
        end

        _forge_exec conversation rename "$conversation_id" $new_name
        return 0
    end

    set -l conversations_output
    set conversations_output ($_FORGE_BIN conversation list --porcelain 2>/dev/null)

    if test -z "$conversations_output"
        _forge_log error "No conversations found"
        return 0
    end

    set -l current_id "$_FORGE_CONVERSATION_ID"

    set -l prompt_text "Rename Conversation ❯ "
    set -l fzf_args --prompt="$prompt_text" --delimiter="$_FORGE_DELIMITER" --with-nth="2,3" --preview="CLICOLOR_FORCE=1 $_FORGE_BIN conversation info {1}; echo; CLICOLOR_FORCE=1 $_FORGE_BIN conversation show {1}" $_FORGE_PREVIEW_WINDOW

    if test -n "$current_id"
        set -l index (_forge_find_index "$conversations_output" "$current_id" 1)
        set fzf_args $fzf_args --bind="start:pos($index)"
    end

    set -l selected_conversation
    set selected_conversation (echo "$conversations_output" | _forge_fzf --header-lines=1 $fzf_args)

    if test -n "$selected_conversation"
        set -l conversation_id (echo "$selected_conversation" | sed -E 's/  .*//' | tr -d '\n')

        echo -n "Enter new name: "
        set -l new_name (read < /dev/tty)

        if test -n "$new_name"
            _forge_exec conversation rename "$conversation_id" $new_name
        else
            _forge_log error "No name provided, rename cancelled"
        end
    end
end

# Helper function to clone and switch to conversation
function _forge_clone_and_switch
    set -l clone_target "$argv[1]"

    set -l original_conversation_id "$_FORGE_CONVERSATION_ID"

    _forge_log info "Cloning conversation $clone_target"
    set -l clone_output
    set clone_output ($_FORGE_BIN conversation clone "$clone_target" 2>&1)
    set -l clone_exit_code $status

    if test $clone_exit_code -eq 0
        set -l new_id (echo "$clone_output" | grep -oE '[a-f0-9-]{36}' | tail -1)

        if test -n "$new_id"
            _forge_switch_conversation "$new_id"
            _forge_log success "└─ Switched to conversation $new_id"

            if test "$clone_target" != "$original_conversation_id"
                echo
                _forge_exec conversation show "$new_id"
                echo
                _forge_exec conversation info "$new_id"
            end
        else
            _forge_log error "Failed to extract new conversation ID from clone output"
        end
    else
        _forge_log error "Failed to clone conversation: $clone_output"
    end
end

# Action handler: Branch conversation at a selected message
function _forge_action_branch
    echo

    if test -z "$_FORGE_CONVERSATION_ID"
        _forge_log error "No active conversation. Start a conversation first or use :conversation to select one"
        return 0
    end

    # Get message tree for the current conversation
    set -l tree_output
    set tree_output ($_FORGE_BIN conversation tree "$_FORGE_CONVERSATION_ID" 2>/dev/null)

    if test -z "$tree_output"
        _forge_log error "No messages found in the current conversation"
        return 0
    end

    # Use fzf to select a message to branch at
    set -l prompt_text "Branch at Message ❯ "
    set -l selected_line
    set selected_line (echo "$tree_output" | _forge_fzf --prompt="$prompt_text" --no-multi --preview-window=hidden)

    if test -z "$selected_line"
        return 0
    end

    # Extract index from the selected line (first column)
    set -l index (echo "$selected_line" | awk '{print $1}')

    if test -z "$index"
        _forge_log error "Could not parse message index"
        return 0
    end

    # Execute branch command using index
    _forge_log info "Branching conversation at message index $index"
    set -l branch_output
    set branch_output ($_FORGE_BIN conversation branch "$_FORGE_CONVERSATION_ID" --at-index "$index" --porcelain 2>&1)
    set -l branch_exit_code $status

    if test $branch_exit_code -eq 0
        set -l new_id "$branch_output"

        if test -n "$new_id"
            _forge_switch_conversation "$new_id"
            _forge_log success "Branched to new conversation $new_id"

            echo
            _forge_exec conversation show "$new_id"
        else
            _forge_log error "Failed to extract new conversation ID from branch output"
        end
    else
        _forge_log error "Failed to branch conversation: $branch_output"
    end
end
