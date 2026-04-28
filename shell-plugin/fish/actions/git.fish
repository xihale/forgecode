# Git integration action handlers

# Action handler: Directly commit changes with AI-generated message
function _forge_action_commit
    set -l additional_context "$argv[1]"
    echo

    if test -n "$additional_context"
        FORCE_COLOR=true CLICOLOR_FORCE=1 $_FORGE_BIN commit --max-diff "$_FORGE_MAX_COMMIT_DIFF" $additional_context
    else
        FORCE_COLOR=true CLICOLOR_FORCE=1 $_FORGE_BIN commit --max-diff "$_FORGE_MAX_COMMIT_DIFF"
    end
    _forge_reset
end

# Action handler: Previews AI-generated commit message
function _forge_action_commit_preview
    set -l additional_context "$argv[1]"
    set -l commit_message
    echo

    if test -n "$additional_context"
        set commit_message (FORCE_COLOR=true CLICOLOR_FORCE=1 $_FORGE_BIN commit --preview --max-diff "$_FORGE_MAX_COMMIT_DIFF" $additional_context)
    else
        set commit_message (FORCE_COLOR=true CLICOLOR_FORCE=1 $_FORGE_BIN commit --preview --max-diff "$_FORGE_MAX_COMMIT_DIFF")
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
