# Git integration action handlers

# Action handler: Directly commit changes with AI-generated message
function _forge_action_commit
    set -l additional_context "$argv[1]"

    # Use set -lx to expose session env vars to the subcommand.
    # This avoids the empty-argument bug that occurs when $session_env
    # expands to "" for an empty list.
    begin
        set -lx FORCE_COLOR true
        set -lx CLICOLOR_FORCE 1
        if test -n "$_FORGE_SESSION_MODEL"
            set -lx FORGE_SESSION__MODEL_ID "$_FORGE_SESSION_MODEL"
        end
        if test -n "$_FORGE_SESSION_PROVIDER"
            set -lx FORGE_SESSION__PROVIDER_ID "$_FORGE_SESSION_PROVIDER"
        end

        if test -n "$additional_context"
            $_FORGE_BIN commit --max-diff "$_FORGE_MAX_COMMIT_DIFF" $additional_context
        else
            $_FORGE_BIN commit --max-diff "$_FORGE_MAX_COMMIT_DIFF"
        end
    end
    _forge_reset
end

# Action handler: Previews AI-generated commit message
function _forge_action_commit_preview
    set -l additional_context "$argv[1]"
    set -l commit_message

    # Use set -lx to expose session env vars to the subcommand.
    begin
        set -lx FORCE_COLOR true
        set -lx CLICOLOR_FORCE 1
        if test -n "$_FORGE_SESSION_MODEL"
            set -lx FORGE_SESSION__MODEL_ID "$_FORGE_SESSION_MODEL"
        end
        if test -n "$_FORGE_SESSION_PROVIDER"
            set -lx FORGE_SESSION__PROVIDER_ID "$_FORGE_SESSION_PROVIDER"
        end

        if test -n "$additional_context"
            set commit_message ($_FORGE_BIN commit --preview --max-diff "$_FORGE_MAX_COMMIT_DIFF" $additional_context)
        else
            set commit_message ($_FORGE_BIN commit --preview --max-diff "$_FORGE_MAX_COMMIT_DIFF")
        end
    end

    if test -n "$commit_message"
        # Check if there are staged changes to determine commit strategy
        if git diff --staged --quiet
            # No staged changes: commit all tracked changes with -a flag
            commandline -r "git commit -am $commit_message"
        else
            # Staged changes exist: commit only what's staged
            commandline -r "git commit -m $commit_message"
        end
        commandline -f end-of-line
        commandline -f repaint
    else
        _forge_reset
    end
end
