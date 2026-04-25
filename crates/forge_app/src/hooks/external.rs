#[cfg(target_os = "linux")]
#[allow(unused_imports)] // IntoRawFd used only in tests
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use async_trait::async_trait;
use forge_domain::{Agent, CachedHook, ModelId, ToolCallFull, ToolCallInterceptor};
use serde::{Deserialize, Serialize};
use tracing::debug;

// ---------------------------------------------------------------------------
// CachedHook execution extension (memfd on Linux, temp-file fallback)
// ---------------------------------------------------------------------------

/// Extension trait that adds execution capability to [`CachedHook`].
trait CachedHookExt {
    /// Spawns the cached script content as a child process.
    fn spawn(&self) -> std::io::Result<tokio::process::Child>;
}

impl CachedHookExt for CachedHook {
    #[cfg(target_os = "linux")]
    fn spawn(&self) -> std::io::Result<tokio::process::Child> {
        let (exe_path, _fd_guard) = prepare_executable(self)?;
        Command::new(exe_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
    }

    #[cfg(not(target_os = "linux"))]
    fn spawn(&self) -> std::io::Result<tokio::process::Child> {
        let exe_path = prepare_executable(self)?;
        Command::new(exe_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
    }
}

/// Prepares an executable path for the cached content.
///
/// On Linux: creates a `memfd`, writes content, seals it, and returns
/// `/proc/self/fd/<n>` for execution. The returned `Memfd` guard keeps
/// the file descriptor alive — it must not be dropped until after the
/// child process has been spawned.
///
/// On non-Linux: writes content to a temp file and returns its path.
#[cfg(target_os = "linux")]
fn prepare_executable(hook: &CachedHook) -> std::io::Result<(PathBuf, Memfd)> {
    let name = hook
        .source()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "hook".to_string());

    // Create an anonymous in-memory file descriptor
    let fd = memfd_create(&name)?;
    let raw_fd = fd.as_raw_fd();

    // Write the cached content using libc write to avoid IO safety issues
    // (std::fs::File::from_raw_fd would claim ownership of the fd, conflicting
    // with our Memfd wrapper).
    let content = hook.content();
    let mut offset = 0;
    while offset < content.len() {
        let written = unsafe {
            libc::write(
                raw_fd,
                content[offset..].as_ptr() as *const libc::c_void,
                content.len() - offset,
            )
        };
        if written < 0 {
            return Err(std::io::Error::last_os_error());
        }
        offset += written as usize;
    }

    // Set executable permission so the kernel allows execve via /proc/self/fd/N
    let ret = unsafe { libc::fchmod(raw_fd, 0o700) };
    if ret < 0 {
        return Err(std::io::Error::last_os_error());
    }

    // Seal the memfd so the content cannot be modified
    seal_memfd(raw_fd)?;

    // Return /proc/self/fd/N path for execution, keeping the Memfd alive
    Ok((PathBuf::from(format!("/proc/self/fd/{raw_fd}")), fd))
}

#[cfg(not(target_os = "linux"))]
fn prepare_executable(hook: &CachedHook) -> std::io::Result<PathBuf> {
    use std::io::Write;

    let mut temp = tempfile::Builder::new()
        .prefix("forge-hook-")
        .suffix(
            hook.source()
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .as_deref()
                .unwrap_or(".sh"),
        )
        .tempfile()?;

    temp.write_all(hook.content())?;
    temp.as_file_mut().sync_all()?;

    // Keep the file on disk but remove the directory entry so it's
    // auto-cleaned when the last fd closes
    let (_, path) = temp.keep()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    }

    Ok(path)
}

// ---------------------------------------------------------------------------
// Linux memfd helpers
// ---------------------------------------------------------------------------

/// RAII wrapper around a memfd file descriptor.
#[cfg(target_os = "linux")]
struct Memfd(std::os::fd::OwnedFd);

#[cfg(target_os = "linux")]
impl Memfd {
    /// Takes ownership of the raw fd and closes it on drop.
    fn new(fd: std::os::fd::OwnedFd) -> Self {
        Self(fd)
    }
}

#[cfg(target_os = "linux")]
impl std::os::fd::AsFd for Memfd {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

#[cfg(target_os = "linux")]
impl Memfd {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.0.as_raw_fd()
    }
}

/// Creates a memfd and returns a RAII wrapper.
#[cfg(target_os = "linux")]
fn memfd_create(name: &str) -> std::io::Result<Memfd> {
    use std::ffi::CString;

    let c_name = CString::new(name).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "memfd name contains NUL")
    })?;

    // MFD_ALLOW_SEALING = 0x0002U
    // NOTE: We intentionally do NOT set MFD_CLOEXEC (0x0001). When the kernel
    // processes a shebang (e.g. #!/bin/bash), it opens the script path again
    // from the interpreter. If the fd had CLOEXEC, it would be closed after
    // execve and the interpreter would fail to read the script.
    let flags: libc::c_uint = 0x0002;

    // SAFETY: memfd_create is a Linux system call. `c_name` is a valid
    // null-terminated C string and flags are valid constants.
    let fd = unsafe { libc::memfd_create(c_name.as_ptr(), flags) };

    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: fd is a valid, owned file descriptor returned by memfd_create.
    Ok(Memfd::new(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) }))
}

/// Applies sealing to prevent further modifications to the memfd content.
#[cfg(target_os = "linux")]
fn seal_memfd(fd: std::os::fd::RawFd) -> std::io::Result<()> {
    // F_SEAL_SEAL | F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_WRITE
    const SEALS: libc::c_ulong = 0x0001 | 0x0002 | 0x0004 | 0x0008;

    // SAFETY: fcntl with F_ADD_SEALS is safe for valid memfd file descriptors.
    let ret = unsafe { libc::fcntl(fd, libc::F_ADD_SEALS, SEALS) };

    if ret < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Interceptor
// ---------------------------------------------------------------------------

/// Interceptor that executes external hook scripts to modify tool calls.
///
/// At construction time, the full content of every hook script is read into
/// memory (cached). At runtime, `intercept()` executes each script from an
/// anonymous in-memory file descriptor — zero disk I/O, zero TOCTOU risk.
///
/// # Hook protocol
///
/// Input (stdin):
/// ```json
/// {"tool_name": "shell", "tool_input": {"command": "git status"}}
/// ```
///
/// Output (stdout) -- allow with modification:
/// ```json
/// {"decision": "allow", "hookSpecificOutput": {"tool_input": {"command": "rtk git status"}}}
/// ```
///
/// Output (stdout) -- allow without modification:
/// ```json
/// {"decision": "allow"}
/// ```
///
/// Output (stdout) -- deny:
/// ```json
/// {"decision": "deny", "reason": "blocked by policy"}
/// ```
#[derive(Clone)]
pub struct ExternalHookInterceptor {
    cached_hooks: Vec<CachedHook>,
    timeout_secs: u64,
}

impl Default for ExternalHookInterceptor {
    fn default() -> Self {
        Self {
            cached_hooks: Vec::new(),
            timeout_secs: 30,
        }
    }
}

impl ExternalHookInterceptor {
    /// Creates a new external hook interceptor with hook content cached in
    /// memory.
    ///
    /// # Arguments
    ///
    /// * `cached_hooks` - Hook scripts whose content has been read into memory
    /// * `timeout_secs` - Optional timeout in seconds for hook execution (default: 30)
    pub fn new(cached_hooks: Vec<CachedHook>, timeout_secs: Option<u64>) -> Self {
        Self {
            cached_hooks,
            timeout_secs: timeout_secs.unwrap_or(30),
        }
    }

    /// Creates a new external hook interceptor from pre-cached hooks.
    pub fn from_arc(
        cached_hooks: std::sync::Arc<Vec<CachedHook>>,
        timeout_secs: Option<u64>,
    ) -> Self {
        Self {
            cached_hooks: (*cached_hooks).clone(),
            timeout_secs: timeout_secs.unwrap_or(30),
        }
    }

    /// Run a single cached hook script, piping JSON input and parsing JSON
    /// output.
    async fn run_hook(
        hook: &CachedHook,
        input: &HookInput,
        timeout_secs: u64,
    ) -> anyhow::Result<HookOutput> {
        let input_json = serde_json::to_string(input)?;

        debug!(hook = %hook.source().display(), "Executing external hook");

        let mut child = match hook.spawn() {
            Ok(c) => c,
            Err(e) => {
                debug!(
                    hook = %hook.source().display(),
                    error = %e,
                    "Failed to spawn hook, treating as allow"
                );
                return Ok(HookOutput {
                    decision: "allow".to_string(),
                    reason: None,
                    hook_specific_output: None,
                });
            }
        };

        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input_json.as_bytes()).await?;
        drop(stdin);

        // Wait for output with timeout
        let output = match timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
            .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                debug!(
                    hook = %hook.source().display(),
                    timeout_secs = timeout_secs,
                    "Hook execution timed out, treating as allow"
                );
                return Ok(HookOutput {
                    decision: "allow".to_string(),
                    reason: Some("Hook execution timed out".to_string()),
                    hook_specific_output: None,
                });
            }
        };

        // Log stderr at debug level for hook debugging
        if !output.stderr.is_empty() {
            let stderr_str = String::from_utf8_lossy(&output.stderr);
            debug!(
                hook = %hook.source().display(),
                stderr = %stderr_str,
                "Hook stderr output"
            );
        }

        if !output.status.success() {
            debug!(
                hook = %hook.source().display(),
                exit_code = ?output.status.code(),
                "Hook exited with non-zero status, skipping"
            );
            // Treat non-zero exit as "allow" (pass-through)
            return Ok(HookOutput {
                decision: "allow".to_string(),
                reason: None,
                hook_specific_output: None,
            });
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        match serde_json::from_str::<HookOutput>(&output_str) {
            Ok(hook_output) => Ok(hook_output),
            Err(e) => {
                debug!(
                    hook = %hook.source().display(),
                    error = %e,
                    "Hook output was not valid JSON, treating as allow"
                );
                Ok(HookOutput {
                    decision: "allow".to_string(),
                    reason: None,
                    hook_specific_output: None,
                })
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct HookInput {
    tool_name: String,
    tool_input: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
struct HookOutput {
    decision: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: Option<HookSpecificOutput>,
}

#[derive(Serialize, Deserialize, Clone)]
struct HookSpecificOutput {
    tool_input: serde_json::Value,
}

/// Returns the sorted list of hook scripts for a given event.
///
/// Scans `~/.forge/hooks/<event>.d/` for executable files, sorted
/// alphabetically by filename.
///
/// This is used by the startup loader and CLI commands. It is not called
/// during `intercept()` — the interceptor uses cached content instead.
pub fn discover_hooks(event_name: &str) -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let hook_dir = home
        .join(".forge")
        .join("hooks")
        .join(format!("{event_name}.d"));

    if !hook_dir.is_dir() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(&hook_dir) else {
        return Vec::new();
    };

    let mut hooks: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            // Only include files (not directories)
            path.is_file()
        })
        .filter(|path| {
            // On Unix, check if the file is executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(path)
                    .map(|m| m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            }
            #[cfg(not(unix))]
            {
                // On non-Unix, include files with common script extensions
                path.extension()
                    .is_some_and(|ext| ext == "sh" || ext == "bash" || ext == "py")
            }
        })
        .collect();

    // Sort alphabetically for deterministic execution order
    hooks.sort();
    hooks
}

#[async_trait]
impl ToolCallInterceptor for ExternalHookInterceptor {
    async fn intercept(
        &self,
        tool_call: &mut ToolCallFull,
        _agent: &Agent,
        _model_id: &ModelId,
    ) -> anyhow::Result<()> {
        let hooks = &self.cached_hooks;
        if hooks.is_empty() {
            return Ok(());
        }

        // Build initial input from the tool call
        let mut current_input = HookInput {
            tool_name: tool_call.name.as_str().to_string(),
            tool_input: serde_json::to_value(&tool_call.arguments)?,
        };

        for hook in hooks {
            let output = Self::run_hook(hook, &current_input, self.timeout_secs).await?;

            match output.decision.as_str() {
                "deny" => {
                    let reason = output.reason.as_deref().unwrap_or("no reason provided");
                    debug!(
                        hook = %hook.source().display(),
                        reason = reason,
                        "Hook denied tool call"
                    );
                    return Err(anyhow::anyhow!("Hook denied tool call: {}", reason));
                }
                "allow" => {
                    if let Some(specific) = &output.hook_specific_output {
                        // Hook modified the tool input -- update for next hook
                        // in the pipeline
                        current_input.tool_input = specific.tool_input.clone();
                    }
                    // Continue to next hook
                }
                other => {
                    debug!(
                        hook = %hook.source().display(),
                        decision = other,
                        "Unknown hook decision, treating as allow"
                    );
                }
            }
        }

        // Apply the final result back to the tool call
        if let Ok(updated_args) =
            serde_json::from_value::<forge_domain::ToolCallArguments>(current_input.tool_input)
        {
            tool_call.arguments = updated_args;
        } else {
            debug!("Failed to deserialize hook output, keeping original arguments");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_hooks_empty_dir() {
        // This test just ensures the function doesn't panic with missing dirs
        let hooks = discover_hooks("nonexistent-event");
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_hook_output_deserialize_allow() {
        let json = r#"{"decision":"allow"}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.decision, "allow");
        assert!(output.hook_specific_output.is_none());
    }

    #[test]
    fn test_hook_output_deserialize_allow_with_modification() {
        let json =
            r#"{"decision":"allow","hookSpecificOutput":{"tool_input":{"command":"rtk ls"}}}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.decision, "allow");
        assert!(output.hook_specific_output.is_some());
        let specific = output.hook_specific_output.unwrap();
        assert_eq!(specific.tool_input["command"], "rtk ls");
    }

    #[test]
    fn test_hook_output_deserialize_deny() {
        let json = r#"{"decision":"deny","reason":"blocked"}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.decision, "deny");
        assert_eq!(output.reason.as_deref(), Some("blocked"));
    }

    #[test]
    fn test_interceptor_with_empty_cached_hooks() {
        let interceptor = ExternalHookInterceptor::new(Vec::new(), None);
        assert!(interceptor.cached_hooks.is_empty());
    }

    #[test]
    fn test_cached_hook_from_path() {
        let dir = tempfile::tempdir().unwrap();
        let hook_path = dir.path().join("test_hook.sh");
        std::fs::write(&hook_path, "#!/bin/bash\necho hello").unwrap();

        let cached = CachedHook::from_path(hook_path.clone()).unwrap();
        assert_eq!(cached.source(), hook_path);
        assert_eq!(cached.content(), b"#!/bin/bash\necho hello");
    }

    #[tokio::test]
    async fn test_interceptor_deny_tool_call() {
        // Create a temporary hook script that outputs deny
        let dir = tempfile::tempdir().unwrap();
        let hook_path = dir.path().join("test_hook.sh");
        std::fs::write(
            &hook_path,
            r#"#!/bin/bash
echo '{"decision":"deny","reason":"test block"}'
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let cached = CachedHook::from_path(hook_path).unwrap();
        let interceptor = ExternalHookInterceptor::new(vec![cached], None);
        let mut tool_call = forge_domain::ToolCallFull::new("test_tool")
            .arguments(forge_domain::ToolCallArguments::from(serde_json::json!({})));
        // Create minimal agent for test (agent parameter is unused in intercept)
        let agent = forge_domain::Agent::new(
            forge_domain::AgentId::from("test-agent"),
            "test-provider".to_string().into(),
            forge_domain::ModelId::from("test-model"),
        );
        let model_id = forge_domain::ModelId::new("test-model");

        let result = interceptor.intercept(&mut tool_call, &agent, &model_id).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("denied tool call"));
        assert!(err.to_string().contains("test block"));
    }

    #[tokio::test]
    async fn test_hook_timeout_cleans_up_process() {
        // Create a hook that runs longer than the timeout
        let dir = tempfile::tempdir().unwrap();
        let hook_path = dir.path().join("slow_hook.sh");
        std::fs::write(&hook_path, "#!/bin/bash\nsleep 10").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let cached = CachedHook::from_path(hook_path).unwrap();
        // Pass timeout as injected parameter (1 second) instead of using env var
        let interceptor = ExternalHookInterceptor::new(vec![cached], Some(1));
        let mut tool_call = forge_domain::ToolCallFull::new("test_tool")
            .arguments(forge_domain::ToolCallArguments::from(serde_json::json!({})));
        let agent = forge_domain::Agent::new(
            forge_domain::AgentId::from("test-agent"),
            "test-provider".to_string().into(),
            forge_domain::ModelId::from("test-model"),
        );
        let model_id = forge_domain::ModelId::new("test-model");

        let start = std::time::Instant::now();
        let result = interceptor.intercept(&mut tool_call, &agent, &model_id).await;
        let duration = start.elapsed();

        // Verify timeout occurred (should take ~1 second, not 10)
        assert!(duration < std::time::Duration::from_secs(5));
        // Timeout should result in Ok(()) (treated as allow)
        assert!(result.is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_memfd_create_and_seal() {
        let fd = memfd_create("test-hook").expect("memfd_create should succeed");
        let raw = fd.as_raw_fd();

        // Write some data using libc write (borrows the fd, doesn't take ownership)
        let data = b"#!/bin/bash\necho hello";
        let written =
            unsafe { libc::write(raw, data.as_ptr() as *const libc::c_void, data.len()) };
        assert!(written >= 0, "write should succeed");

        // Seal it
        seal_memfd(raw).expect("seal_memfd should succeed");

        // Further writes should fail (sealed)
        let more_data = b"more data";
        let result =
            unsafe { libc::write(raw, more_data.as_ptr() as *const libc::c_void, more_data.len()) };
        assert!(result < 0, "Write after seal should fail");
    }
}
