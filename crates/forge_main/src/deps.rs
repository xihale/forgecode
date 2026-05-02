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
    Arch,
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
    if std::path::Path::new("/etc/arch-release").exists() {
        return Platform::Arch;
    }
    if std::path::Path::new("/etc/debian_version").exists() {
        return Platform::Debian;
    }
    Platform::Unknown
}

/// Per-platform install commands for an external tool.
struct InstallHint {
    termux: &'static str,
    debian: &'static str,
    arch: &'static str,
    macos: &'static str,
}

impl InstallHint {
    /// Returns the install command for the given platform, or `None` if unknown.
    fn for_platform(&self, platform: Platform) -> Option<&'static str> {
        match platform {
            Platform::Termux => Some(self.termux),
            Platform::Debian => Some(self.debian),
            Platform::Arch => Some(self.arch),
            Platform::Macos => Some(self.macos),
            Platform::Unknown => None,
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
                arch: "sudo pacman -S fzf",
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
                arch: "sudo pacman -S bat",
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
                arch: "sudo pacman -S git",
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
        let hint = tool
            .install
            .for_platform(platform)
            .map(|cmd| format!(" Install: {cmd}"))
            .unwrap_or_default();
        warnings.push(format!(
            "'{}' not found (needed for {}).{}",
            tool.name, tool.reason, hint
        ));
    }

    if missing_required.is_empty() {
        return Ok(());
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push("Missing required tools:".to_string());
    for tool in &missing_required {
        let hint = tool
            .install
            .for_platform(platform)
            .map(|cmd| format!(" install: {cmd}"))
            .unwrap_or_default();
        lines.push(format!("  {}{hint}", tool.name));
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
            Platform::Termux | Platform::Debian | Platform::Arch | Platform::Macos | Platform::Unknown
        ));
    }

    #[test]
    fn test_install_hint_for_platform() {
        let hint = InstallHint {
            termux: "pkg install fzf",
            debian: "sudo apt install fzf",
            arch: "sudo pacman -S fzf",
            macos: "brew install fzf",
        };
        assert_eq!(hint.for_platform(Platform::Termux), Some("pkg install fzf"));
        assert_eq!(hint.for_platform(Platform::Debian), Some("sudo apt install fzf"));
        assert_eq!(hint.for_platform(Platform::Arch), Some("sudo pacman -S fzf"));
        assert_eq!(hint.for_platform(Platform::Macos), Some("brew install fzf"));
        assert_eq!(hint.for_platform(Platform::Unknown), None);
    }
}
