## Summary

Add external hook interceptor system with SHA-256 integrity verification, in-memory execution via memfd (Linux) to eliminate TOCTOU, and CLI management commands for trusting and deleting hook scripts.

## Context

External hook scripts allow users to intercept and modify tool calls before execution (e.g. rewriting shell commands, blocking dangerous operations). This PR introduces the full lifecycle: discovery, trust verification, secure execution, and CLI management.

The implementation was hardened through multiple review iterations to address security concerns around TOCTOU, path traversal, process lifecycle, and hook scope coverage.

## Changes

### Hook Interception (`e640a3b9`)
- `ExternalHookInterceptor` that spawns hook scripts as child processes
- JSON-based hook protocol: `{tool_name, tool_input}` in, `{decision, hookSpecificOutput}` out
- Three decisions: `allow` (with optional modification), `deny` (blocks execution), unknown (pass-through)
- Hooks execute sequentially in alphabetical order; each hook can modify arguments for the next

### SHA-256 Integrity Verification (`68c3765d`)
- `TrustStore` persisted at `~/.forge/hooks/trust.json` with per-hook SHA-256 hashes
- `HookTrustStatus` enum: `Trusted`, `Untrusted`, `Tampered`, `Missing`
- Startup verification: trusted hooks are loaded, tampered hooks are rejected with DANGER warnings
- Atomic trust store writes with read-only permissions as a speed bump

### UX Improvements (`7c5ce8e6`)
- Non-interactive startup: no prompts, all trust management via CLI
- `forge hook list` — shows all hooks with trust status
- `forge hook trust <path>` — computes hash and stores in trust DB
- `forge hook delete <path>` — removes file and trust record
- Hook summary displayed after startup banner

### Security Hardening (`9f4ea5ea`)
- **Memfd execution (Linux)**: Hook content is loaded into anonymous in-memory file descriptors at startup, sealed against modification, and executed via `/proc/self/fd/N` — zero disk I/O, zero TOCTOU risk at runtime
- **Temp-file fallback (non-Linux)**: Content written to temp files with auto-cleanup
- **Path traversal protection**: `validate_hook_path()` canonicalizes and checks that trust/delete targets remain within `~/.forge/hooks/`
- **Process cleanup**: `kill_on_drop(true)` on all hook child processes ensures timed-out hooks are terminated
- **Timeout isolation**: `timeout_secs` injected as constructor parameter instead of global env var
- **Full hook coverage**: Task tool calls and internal agent executor now go through the same interceptor pipeline

### Key Implementation Details
- `CachedHook` domain type holds file content in memory; source path retained for diagnostics only
- `ToolCallContext` propagates cached hooks through the entire tool call chain
- Orchestrator applies hooks to both task and non-task tool calls uniformly
- Internal agents spawned by the task tool inherit cached hooks from parent context

## Testing

```bash
# Compile check
cargo check -p forge_app -p forge_api -p forge_domain -p forge_main

# Run all tests
cargo test -p forge_app -p forge_domain -p forge_api

# Lint
cargo clippy -p forge_app -p forge_api -p forge_domain -p forge_main -- -D warnings
```

Key test coverage:
- Hook output deserialization (allow/deny/modify)
- `deny` decision blocks tool execution (returns error)
- Hook timeout cleans up child process within expected time
- Cached hook storage and propagation through `ToolCallContext`
- Trust store: hash determinism, tamper detection, save/load round-trip
- Path validation prevents traversal outside hooks directory
- Memfd creation and sealing (Linux only)

## Files Changed (hook-related)

| File | Role |
|------|------|
| `crates/forge_domain/src/cached_hook.rs` | New — in-memory hook content type |
| `crates/forge_domain/src/hook.rs` | `ToolCallInterceptor` trait + `Hook` lifecycle |
| `crates/forge_domain/src/tools/call/context.rs` | Hook propagation through tool context |
| `crates/forge_app/src/hooks/external.rs` | Interceptor + memfd execution + discovery |
| `crates/forge_app/src/hooks/trust.rs` | SHA-256 trust store + path validation |
| `crates/forge_app/src/hooks/loader.rs` | Startup verification and loading |
| `crates/forge_app/src/orch.rs` | Hook integration in tool call execution |
| `crates/forge_app/src/agent_executor.rs` | Hook inheritance for task tool |
| `crates/forge_app/src/app.rs` | Orchestrator wiring with cached hooks |
| `crates/forge_api/src/forge_api.rs` | Startup hook loading + session caching |
| `crates/forge_api/src/api.rs` | `hook_summary()` trait method |
| `crates/forge_main/src/cli.rs` | `forge hook` CLI commands |
| `crates/forge_main/src/ui.rs` | Hook command handlers with path validation |
