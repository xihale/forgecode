/// Startup dependency checker for external CLI tools.
///
/// Some features delegate to external binaries (e.g. `fzf` for interactive
/// selection). This module checks availability at startup and surfaces
/// actionable warnings or errors.

use std::process::Command;

/// A tool that forge optionally or necessarily depends on at runtime.
struct Tool {
    name: &'static str,
    required: bool,
    /// Short description of what breaks without this tool.
    reason: &'static str,
    /// Suggested install command (Termux / Debian / macOS).
    install_hint: &'static str,
}

/// Returns the canonical list of external tools forge uses.
fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "fzf",
            required: true,
            reason: "Interactive selection (model, provider, conversation)",
            install_hint: "pkg install fzf       # Termux\n       apt install fzf       # Debian/Ubuntu\n       brew install fzf      # macOS",
        },
        Tool {
            name: "bat",
            required: false,
            reason: "Syntax-highlighted file previews in completion",
            install_hint: "pkg install bat       # Termux\n       apt install bat       # Debian/Ubuntu\n       brew install bat      # macOS",
        },
        Tool {
            name: "git",
            required: false,
            reason: "Sandbox worktrees, branch info, commit generation",
            install_hint: "pkg install git       # Termux\n       apt install git       # Debian/Ubuntu\n       brew install git      # macOS",
        },
    ]
}

/// Returns `true` when the binary is reachable on `$PATH`.
fn is_available(name: &str) -> bool {
    Command::new("which").arg(name).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Checks all external tool dependencies.
///
/// - **Required** tools that are missing cause an immediate error return.
/// - **Optional** tools that are missing produce a warning line appended to
///   `warnings`.
///
/// # Errors
///
/// Returns an error listing every missing required tool with install hints.
pub fn check(warnings: &mut Vec<String>) -> anyhow::Result<()> {
    let tools = tools();
    let missing_required: Vec<&Tool> = tools
        .iter()
        .filter(|tool| !is_available(tool.name))
        .filter(|tool| tool.required)
        .collect();

    for tool in &tools {
        if is_available(tool.name) {
            continue;
        }
        if !tool.required {
            warnings.push(format!(
                "⚠ Optional tool '{}' not found — {}.\n    Install: {}",
                tool.name, tool.reason, tool.install_hint
            ));
        }
    }

    if missing_required.is_empty() {
        return Ok(());
    }

    let mut msg = "Missing required external tools:\n".to_string();
    for tool in &missing_required {
        msg.push_str(&format!(
            "\n  ✗ {} — {}\n    Install:\n    {}\n",
            tool.name, tool.reason, tool.install_hint
        ));
    }
    Err(anyhow::anyhow!("{}", msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_available_known_binary() {
        let actual = is_available("which");
        assert!(actual, "'which' should always be on PATH");
    }

    #[test]
    fn test_is_available_nonexistent_binary() {
        let actual = is_available("forge_nonexistent_tool_xyz");
        assert!(!actual, "made-up binary should not exist");
    }

    #[test]
    fn test_check_collects_optional_warnings() {
        let mut warnings = Vec::new();
        let _ = check(&mut warnings);
    }
}
