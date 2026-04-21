# Forge right prompt for fish shell
# Displays agent, model and token count information

function fish_right_prompt
    set -l forge_bin "$_FORGE_BIN"
    if test -z "$forge_bin"
        set forge_bin forge
    end

    # Pass session variables as environment to the rprompt command
    set -lx FORGE_SESSION__MODEL_ID "$_FORGE_SESSION_MODEL"
    set -lx FORGE_SESSION__PROVIDER_ID "$_FORGE_SESSION_PROVIDER"
    set -lx FORGE_REASONING__EFFORT "$_FORGE_SESSION_REASONING_EFFORT"
    set -lx _FORGE_CONVERSATION_ID "$_FORGE_CONVERSATION_ID"
    set -lx _FORGE_ACTIVE_AGENT "$_FORGE_ACTIVE_AGENT"
    $forge_bin fish rprompt 2>/dev/null
end

