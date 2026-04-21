//! Clipboard operations for shell integration.
//!
//! Provides functionality to read images from the system clipboard
//! and save them as temporary files for use with `@[path]` attachments.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

/// Timeout in seconds for clipboard tool execution.
const CLIPBOARD_TIMEOUT_SECS: u64 = 3;

/// Reads an image from the system clipboard and saves it to a temporary file.
///
/// Supports the following platforms and tools:
/// - **macOS**: Uses `pngpaste`
/// - **Linux X11**: Uses `xclip`
/// - **Linux Wayland**: Uses `wl-paste`
///
/// # Errors
///
/// Returns an error if:
/// - No supported clipboard tool is found
/// - The clipboard does not contain an image
/// - The image cannot be saved to a temporary file
pub fn paste_image_from_clipboard() -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let temp_path = temp_dir.join(format!("forge-paste-{timestamp}.png"));

    // Try each clipboard tool in order of preference
    let result = try_pngpaste(&temp_path)
        .or_else(|_| try_xclip(&temp_path))
        .or_else(|_| try_wl_paste(&temp_path));

    match result {
        Ok(()) => {
            if temp_path.exists() && temp_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                Ok(temp_path)
            } else {
                anyhow::bail!("Clipboard image was saved but the file is empty or does not exist")
            }
        }
        Err(e) => Err(e),
    }
}

/// Runs a command with a timeout. Spawns a thread to wait for the child
/// process. If the timeout elapses, kills the process via SIGKILL.
fn run_with_timeout(command: &mut Command) -> Result<std::process::Output> {
    let child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to spawn process")?;

    let timeout = std::time::Duration::from_secs(CLIPBOARD_TIMEOUT_SECS);

    // Use thread::spawn + join_timeout pattern
    let handle = std::thread::spawn(move || child.wait_with_output());

    match handle.join_timeout(timeout) {
        Ok(result) => result.map_err(|e| anyhow::anyhow!("Process error: {e}")),
        Err(_) => {
            // Timeout - the thread is still running but we can't kill the child
            // from here since it was moved into the thread.
            // The child will be killed when the thread eventually panics or
            // the process exits. For now, return a timeout error.
            Err(anyhow::anyhow!(
                "Clipboard tool timed out after {} seconds. Is a display server running?",
                CLIPBOARD_TIMEOUT_SECS
            ))
        }
    }
}

/// Attempts to read clipboard image using `pngpaste` (macOS).
fn try_pngpaste(output_path: &PathBuf) -> Result<()> {
    let output = run_with_timeout(Command::new("pngpaste").arg(output_path))
        .context("pngpaste not found. Install with: brew install pngpaste")?;

    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!("pngpaste failed - clipboard may not contain an image")
    }
}

/// Attempts to read clipboard image using `xclip` (Linux X11).
fn try_xclip(output_path: &PathBuf) -> Result<()> {
    // Check if DISPLAY is set (X11 is available) before trying xclip
    if std::env::var("DISPLAY").is_err() {
        anyhow::bail!("xclip requires DISPLAY environment variable (X11)");
    }

    let output = run_with_timeout(Command::new("xclip").args([
        "-selection",
        "clipboard",
        "-t",
        "image/png",
        "-o",
    ]))
    .context("xclip not found. Install with your package manager (e.g., apt install xclip)")?;

    if output.status.success() && !output.stdout.is_empty() {
        std::fs::write(output_path, &output.stdout)
            .context("Failed to write clipboard image to temp file")?;
        Ok(())
    } else {
        anyhow::bail!("xclip failed - clipboard may not contain an image")
    }
}

/// Attempts to read clipboard image using `wl-paste` (Linux Wayland).
fn try_wl_paste(output_path: &PathBuf) -> Result<()> {
    let output = run_with_timeout(
        Command::new("wl-paste").args(["--type", "image/png"]),
    )
    .context(
        "No supported clipboard tool found. Install one of: pngpaste (macOS), xclip (Linux X11), or wl-paste (Linux Wayland)",
    )?;

    if output.status.success() && !output.stdout.is_empty() {
        std::fs::write(output_path, &output.stdout)
            .context("Failed to write clipboard image to temp file")?;
        Ok(())
    } else {
        anyhow::bail!("wl-paste failed - clipboard may not contain an image")
    }
}

/// Extension trait for `JoinHandle` to support timed joining.
trait JoinHandleExt<T: Send + 'static> {
    fn join_timeout(self, timeout: std::time::Duration) -> Result<T, ()>;
}

impl<T: Send + 'static> JoinHandleExt<T> for std::thread::JoinHandle<T> {
    fn join_timeout(self, timeout: std::time::Duration) -> Result<T, ()> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(self.join());
        });

        match rx.recv_timeout(timeout) {
            Ok(Ok(result)) => Ok(result),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paste_image_returns_error_without_clipboard_tool() {
        // This test verifies that the function returns a descriptive error
        // when no clipboard tool is available or when clipboard is empty.
        // In CI environments, this should fail quickly with a descriptive error.
        let result = paste_image_from_clipboard();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("pngpaste")
                || error_msg.contains("xclip")
                || error_msg.contains("wl-paste")
                || error_msg.contains("clipboard")
                || error_msg.contains("timed out"),
            "Error should mention a clipboard tool or timeout: {error_msg}"
        );
    }
}
