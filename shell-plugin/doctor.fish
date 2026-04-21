#!/usr/bin/env fish

# Fish Doctor - Diagnostic tool for Forge shell environment
# Checks for common configuration issues and environment setup

# Counters
set -l passed 0
set -l failed 0
set -l warnings 0

function print_section
    echo ""
    printf '\033[1m%s\033[0m\n' "$argv[1]"
end

function print_result
    set -l result_status $argv[1]
    set -l message $argv[2]
    set -l detail $argv[3]

    switch $result_status
        case pass
            printf '  \033[0;32m[OK]\033[0m %s\n' "$message"
            set passed (math $passed + 1)
        case fail
            printf '  \033[0;31m[ERROR]\033[0m %s\n' "$message"
            if test -n "$detail"
                printf '  \033[2m· %s\033[0m\n' "$detail"
            end
            set failed (math $failed + 1)
        case warn
            printf '  \033[0;33m[WARN]\033[0m %s\n' "$message"
            if test -n "$detail"
                printf '  \033[2m· %s\033[0m\n' "$detail"
            end
            set warnings (math $warnings + 1)
        case info
            printf '  \033[2m· %s\033[0m\n' "$message"
        case code
            printf '  \033[2m· %s\033[0m\n' "$message"
        case instruction
            printf '  \033[2m· %s\033[0m\n' "$message"
    end
end

printf '\033[1mFORGE ENVIRONMENT DIAGNOSTICS (FISH)\033[0m\n'

# 1. Check Fish version
print_section "Shell Environment"
set -l fish_version $version
if test -n "$fish_version"
    set -l major (string split . -- $fish_version)[1]
    set -l minor (string split . -- $fish_version)[2]
    if test $major -ge 3
        print_result pass "fish: $fish_version"
    else
        print_result warn "fish: $fish_version" "Recommended: 3.6+"
    end
else
    print_result fail "Unable to detect Fish version"
end

# Check terminal information
if test -n "$TERM_PROGRAM"
    if test -n "$TERM_PROGRAM_VERSION"
        print_result pass "Terminal: $TERM_PROGRAM $TERM_PROGRAM_VERSION"
    else
        print_result pass "Terminal: $TERM_PROGRAM"
    end
else if test -n "$TERM"
    print_result pass "Terminal: $TERM"
else
    print_result info "Terminal: unknown"
end

# 2. Check if forge is installed and in PATH
print_section "Forge Installation"

if command -v forge &>/dev/null
    set -l forge_path (command -v forge)
    set -l forge_version (forge --version 2>&1 | head -n1 | awk '{print $2}')
    if test -n "$forge_version"
        print_result pass "forge: $forge_version"
        print_result info "$forge_path"
    else
        print_result pass "forge: installed"
        print_result info "$forge_path"
    end
else
    print_result fail "Forge binary not found in PATH" "Installation: curl -fsSL https://forgecode.dev/cli | sh"
end

# 3. Check shell plugin
print_section "Plugin"

if test -n "$_FORGE_PLUGIN_LOADED"
    print_result pass "Forge plugin loaded"
else
    print_result fail "Forge plugin not loaded"
    print_result instruction "Add to your ~/.config/fish/config.fish:"
    print_result code "forge fish plugin | source"
    print_result instruction "Or run: forge fish setup"
end

# 4. Check Forge theme
print_section "Forge Right Prompt"

if test -n "$_FORGE_THEME_LOADED"
    print_result pass "Forge theme loaded"
else
    print_result warn "Forge theme not loaded"
    print_result instruction "Add to your ~/.config/fish/config.fish:"
    print_result code "forge fish theme | source"
    print_result instruction "Or run: forge fish setup"
end

# 5. Check dependencies
print_section "Dependencies"

# Check for fzf
if command -v fzf &>/dev/null
    set -l fzf_version (fzf --version 2>&1 | head -n1 | awk '{print $1}')
    if test -n "$fzf_version"
        print_result pass "fzf: $fzf_version"
    else
        print_result pass "fzf: installed"
    end
else
    print_result fail "fzf not found" "Required for interactive features. See: https://github.com/junegunn/fzf#installation"
end

# Check for bat
if command -v bat &>/dev/null
    set -l bat_version (bat --version 2>&1 | awk '{print $2}')
    if test -n "$bat_version"
        print_result pass "bat: $bat_version"
    else
        print_result pass "bat: installed"
    end
else
    print_result warn "bat not found" "Enhanced preview. See: https://github.com/sharkdp/bat#installation"
end

# 6. Check system configuration
print_section "System"

if test -n "$FORGE_EDITOR"
    print_result pass "FORGE_EDITOR: $FORGE_EDITOR"
    if test -n "$EDITOR"
        print_result info "EDITOR also set: $EDITOR (ignored)"
    end
else if test -n "$EDITOR"
    print_result pass "EDITOR: $EDITOR"
    print_result info "TIP: Set FORGE_EDITOR for forge-specific editor"
else
    print_result warn "No editor configured" "export EDITOR=vim or export FORGE_EDITOR=vim"
end

# 7. Check font and Nerd Font support
print_section "Nerd Font"

if test -n "$NERD_FONT"
    if test "$NERD_FONT" = "1" -o "$NERD_FONT" = "true"
        print_result pass "NERD_FONT: enabled"
    else
        print_result warn "NERD_FONT: disabled ($NERD_FONT)"
        print_result instruction "Enable Nerd Font by setting:"
        print_result code "export NERD_FONT=1"
    end
else
    print_result pass "Nerd Font: enabled (default)"
    print_result info "Forge will auto-detect based on terminal capabilities"
end

# Summary
echo ""

if test $failed -eq 0 -a $warnings -eq 0
    printf '\033[0;32m[OK]\033[0m \033[1mAll checks passed\033[0m \033[2m(%d)\033[0m\n' $passed
    exit 0
else if test $failed -eq 0
    printf '\033[0;33m[WARN]\033[0m \033[1m%d warnings\033[0m \033[2m(%d passed)\033[0m\n' $warnings $passed
    exit 0
else
    printf '\033[0;31m[ERROR]\033[0m \033[1m%d failed\033[0m \033[2m(%d warnings, %d passed)\033[0m\n' $failed $warnings $passed
    exit 1
end