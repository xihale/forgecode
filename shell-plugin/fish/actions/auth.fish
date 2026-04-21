# Authentication action handlers

# Action handler: Login to provider
function _forge_action_login
    set -l input_text "$argv[1]"
    echo
    set -l selected
    set selected (_forge_select_provider "" "" "" "$input_text")
    if test -n "$selected"
        set -l provider (echo "$selected" | awk -F '  +' '{print $2}')
        _forge_exec_interactive provider login "$provider"
    end
end

# Action handler: Logout from provider
function _forge_action_logout
    set -l input_text "$argv[1]"
    echo
    set -l selected
    set selected (_forge_select_provider '\[yes\]' "" "" "$input_text")
    if test -n "$selected"
        set -l provider (echo "$selected" | awk -F '  +' '{print $2}')
        _forge_exec provider logout "$provider"
    end
end
