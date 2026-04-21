# Forge Fish Shell Plugin
# Modular plugin - sources all required modules

# Configuration variables
source (dirname (status filename))/fish/config.fish

# Core utilities (includes logging)
source (dirname (status filename))/fish/helpers.fish

# Terminal context capture (preexec/postexec hooks, OSC 133)
source (dirname (status filename))/fish/context.fish

# Completion widget
source (dirname (status filename))/fish/completion.fish

# Action handlers
source (dirname (status filename))/fish/actions/core.fish
source (dirname (status filename))/fish/actions/config.fish
source (dirname (status filename))/fish/actions/conversation.fish
source (dirname (status filename))/fish/actions/git.fish
source (dirname (status filename))/fish/actions/editor.fish
source (dirname (status filename))/fish/actions/auth.fish
source (dirname (status filename))/fish/actions/provider.fish

# Main dispatcher
source (dirname (status filename))/fish/dispatcher.fish

# Key bindings
source (dirname (status filename))/fish/bindings.fish
