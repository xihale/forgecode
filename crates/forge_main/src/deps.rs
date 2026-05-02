/// Startup dependency checker for external CLI tools.
///
/// Some features delegate to external binaries (e.g. `fzf` for interactive
/// selection). This module checks availability at startup and surfaces
/// actionable warnings or errors.

use std::process::Command;

/// Detected host platform for install-hint selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Termux,
    Debian,
    Macos,
    Unknown,
}

fn detect_platform() -> Platform {
    if std::env::var("TERMUX_VERSION").is_ok() {
        return Platform::Termux;
    }
    if cfg!(target_os = "macos") {
        return Platform::Macos;
    }
    // Heuristic: /etc/debian_version exists on Debian/Ubuntu derivatives
    if std::path::Path::new("/etc/debian_version").exists() {
        return Platform::Debian;
    }
    Platform::Unknown
}

/// Per-platform install commands for an external tool.
struct InstallHint {
    termux: &'static str,
    debian: &'static str,
    macos: &'static str,
}

impl InstallHint {
    /// Returns the install command appropriate for the given platform.
    fn for_platform(&self, platform: Platform) -> &'static str {
        match platform {
            Platform::Termux => self.termux,
            Platform::Debian => self.debian,
            Platform::Macos => self.macos,
            Platform::Unknown => self.debian, // reasonable fallback
        }
    }
}

/// An external tool that forge depends on at runtime.
struct Tool {
    name: &'static str,
    required: bool,
    reason: &'static str,
    install: InstallHint,
}

/// Returns the canonical list of external tools forge uses.
fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "fzf",
            required: true,
            reason: "interactive selection",
            install: InstallHint {
                termux: "pkg install fzf",
                debian: "sudo apt install fzf",
                macos: "brew install fzf",
            },
        },
        Tool {
            name: "bat",
            required: false,
            reason: "syntax-highlighted file previews",
            install: InstallHint {
                termux: "pkg install bat",
                debian: "sudo apt install bat",
                macos: "brew install bat",
            },
        },
        Tool {
            name: "git",
            required: false,
            reason: "sandbox worktrees & commit generation",
            install: InstallHint {
                termux: "pkg install git",
                debian: "sudo apt install git",
                macos: "brew install git",
            },
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
/// - **Optional** tools that are missing produce a warning per tool.
///
/// # Errors
///
/// Returns an error listing every missing required tool with install hints.
pub fn check(warnings: &mut Vec<String>) -> anyhow::Result<()> {
    let platform = detect_platform();
    let tools = tools();

    let missing_required: Vec<&Tool> = tools
        .iter()
        .filter(|tool| !is_available(tool.name) && tool.required)
        .collect();

    for tool in &tools {
        if is_available(tool.name) || tool.required {
            continue;
        }
        let cmd = tool.install.for_platform(platform);
        warnings.push(format!(
            "'{}' not found (needed for {}). Install: {}",
            tool.name, tool.reason, cmd
        ));
    }

    if missing_required.is_empty() {
        return Ok(());
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push("Missing required tools:".to_string());
    for tool in &missing_required {
        let cmd = tool.install.for_platform(platform);
        lines.push(format!("  {} — install: {}", tool.name, cmd));
    }
    Err(anyhow::anyhow!("{}", lines.join("\n")))
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
    fn test_detect_platform() {
        // Just verify it doesn't panic and returns a valid variant.
        let platform = detect_platform();
        assert!(matches!(
            platform,
            Platform::Termux | Platform::Debian | Platform::Macos | Platform::Unknown
        ));
    }

    #[test]
    fn test_install_hint_for_platform() {
        let hint = InstallHint {
            termux: "pkg install fzf",
            debian: "sudo apt install fzf",
            macos: "brew install fzf",
        };
        assert_eq!(hint.for_platform(Platform::Termux), "pkg install fzf");
        assert_eq!(hint.for_platform(Platform::Debian), "sudo apt install fzf");
        assert_eq!(hint.for_platform(Platform::Macos), "brew install fzf");
        assert_eq!(hint.for_platform(Platform::Unknown), "sudo apt install fzf");
    }
}
