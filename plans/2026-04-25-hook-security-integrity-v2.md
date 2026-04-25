# Hook Security & Integrity Verification

## Objective

Add a security verification mechanism to the external hook system so that only user-trusted hook scripts are loaded and executed. This includes:
- SHA-256 integrity checking for all discovered hooks
- Interactive trust prompts when untrusted hooks are found at startup
- A new `forge hooks` CLI command group for managing hook trust (list, trust, delete)
- High-danger alerts when trusted hooks are modified (hash mismatch)

## Core Design: Load-Once-At-Startup

**关键设计决策：启动时一次性完成所有磁盘 I/O，运行时不再读取磁盘。**

当前问题：`ExternalHookInterceptor::intercept()` 在 `external.rs:183` **每次工具调用都重新扫描磁盘** (`discover_hooks()`)。这既是性能浪费，也创造了 TOCTOU（Time-of-check-to-time-of-use）攻击窗口。

新设计：
1. **启动时**（`app.rs` 构造 interceptor 时）：discover 所有 hook → 计算 SHA-256 → 比对 trust store → 处理 untrusted/tampered → 将验证通过的 hook 路径缓存到内存
2. **运行时**（每次 `intercept()` 调用）：直接使用内存中缓存的路径列表，零磁盘 I/O

```
启动时（一次性）:
  discover_hooks() → [hook1.sh, hook2.sh]
       ↓
  compute_hash() → [abc123, def456]
       ↓
  trust_store.check() → [Trusted, Tampered!]
       ↓
  处理: Trusted→缓存, Tampered→警告+跳过, Untrusted→交互提示
       ↓
  cached_hooks: [hook1.sh]  ← 只有可信的

运行时（每次工具调用）:
  intercept() → 直接使用 cached_hooks → 执行
```

这样：
- **安全性**：启动后锁定，运行期间无法通过替换脚本注入恶意代码
- **性能**：运行时零文件系统开销
- **一致性**：同一会话内 hook 行为完全可预测

## Architecture Overview

```
~/.forge/hooks/
├── toolcall-start.d/
│   ├── 01-my-hook.sh          ← hook scripts
│   └── 02-another-hook.py
└── trust.json                  ← centralized trust store (NEW)
```

**Trust store** (`~/.forge/hooks/trust.json`):
```json
{
  "version": 1,
  "hooks": {
    "toolcall-start.d/01-my-hook.sh": {
      "sha256": "abc123...",
      "trusted_at": "2026-04-25T12:00:00Z"
    }
  }
}
```

Keys are relative paths from `~/.forge/hooks/` to keep the store portable.

## Implementation Plan

### Phase 1: Trust Store & Integrity Module

- [ ] **1.1 Create `crates/forge_app/src/hooks/trust.rs`** — Core integrity module containing:
  - `TrustStore` struct with serde Serialize/Deserialize for `trust.json`
  - `TrustedHook` struct: `{ sha256: String, trusted_at: String }`
  - `HookTrustStatus` enum: `Trusted`, `Untrusted`, `Tampered { expected, actual }`, `Missing`
  - `compute_hash(path: &Path) -> Result<String>` — SHA-256 of file contents (same logic as RTK reference `integrity.rs:40-46`)
  - `TrustStore::load() -> Result<TrustStore>` — reads `~/.forge/hooks/trust.json`, returns default if missing
  - `TrustStore::save(&self) -> Result<()>` — writes trust store atomically
  - `TrustStore::check(&self, relative_path: &str, actual_path: &Path) -> HookTrustStatus`
  - `TrustStore::trust(&mut self, relative_path: &str, hook_path: &Path) -> Result<()>` — computes hash and saves
  - `TrustStore::untrust(&mut self, relative_path: &str) -> Result<()>` — removes entry
  - `TrustStore::list(&self) -> Vec<(String, Option<&TrustedHook>)>` — all known hooks with trust info
  - `trust_store_path() -> PathBuf` — resolves to `~/.forge/hooks/trust.json`
  - `hooks_base_dir() -> PathBuf` — resolves to `~/.forge/hooks/`

  Rationale: Centralized trust store is simpler than per-hook hash files (which the RTK reference uses for a single hook). A single JSON file allows atomic reads/writes and easy listing.

- [ ] **1.2 Add `trust` module to `crates/forge_app/src/hooks/mod.rs`** — Export `TrustStore`, `HookTrustStatus`, `TrustedHook`, `compute_hash`, `trust_store_path`, `hooks_base_dir`.

- [ ] **1.3 Write unit tests for `trust.rs`** — Test hash computation determinism, trust store load/save/check/trust/untrust, tampered detection, missing file handling. Use `tempfile::TempDir` and the project's three-step test pattern (fixture/actual/expected).

### Phase 2: Startup Verification & Cached Hook Loading

- [ ] **2.1 Refactor `ExternalHookInterceptor` to hold cached hook paths** — Change from unit struct to:
  ```rust
  pub struct ExternalHookInterceptor {
      cached_hooks: Vec<PathBuf>,
  }
  ```
  - `new(cached_hooks: Vec<PathBuf>) -> Self` — constructor takes pre-verified paths
  - `intercept()` uses `&self.cached_hooks` instead of calling `Self::discover_hooks()`
  - `discover_hooks()` remains as a standalone `pub fn` (not method) for reuse in startup logic and CLI commands
  - `run_hook()` remains as-is

  Rationale: By caching at construction time, `intercept()` becomes a pure in-memory operation with zero disk I/O. This eliminates the TOCTOU window entirely — after startup, no amount of file system tampering can affect the running session.

- [ ] **2.2 Create startup verification function `load_and_verify_hooks()`** — New async function in `trust.rs` or a new `crates/forge_app/src/hooks/loader.rs`:
  ```rust
  pub async fn load_and_verify_hooks<U: UserInfra>(
      event_name: &str,
      user_infra: Arc<U>,
  ) -> anyhow::Result<Vec<PathBuf>>
  ```
  Logic:
  1. Call `discover_hooks(event_name)` to find all candidate scripts
  2. Load `TrustStore`
  3. For each hook, compute hash and check against trust store:
     - `Trusted` (hash matches) → add to result
     - `Untrusted` (no entry in trust store) → prompt user via `UserInfra::select_one()` with three options: "Trust", "Delete", "Ignore". If "Trust", compute hash, save to trust store, add to result. If "Delete", delete the file, skip. If "Ignore", skip.
     - `Tampered` (hash mismatch) → print HIGH DANGER warning to stderr:
       ```
       ⚠ DANGER: Hook script has been modified!
         Hook: toolcall-start.d/01-my-hook.sh
         Expected: abc123...
         Actual:   def456...
         This hook will NOT be loaded.
         Re-trust: forge hooks trust toolcall-start.d/01-my-hook.sh
         Or delete: forge hooks delete toolcall-start.d/01-my-hook.sh
       ```
       Mark as untrusted in store, skip the hook.
     - `Missing` → skip
  4. Save updated trust store
  5. Return verified hook paths

- [ ] **2.3 Update `app.rs` hook assembly** — In `crates/forge_app/src/app.rs:150`, replace:
  ```rust
  let external_interceptor = ExternalHookInterceptor::new();
  ```
  with:
  ```rust
  let cached_hooks = load_and_verify_hooks("toolcall-start", services.clone()).await?;
  let external_interceptor = ExternalHookInterceptor::new(cached_hooks);
  ```
  This happens once at conversation start. The `services` already implements `UserInfra`.

- [ ] **2.4 Handle non-interactive (piped/non-TTY) mode** — In `load_and_verify_hooks()`, when stdin is not a TTY (`!std::io::stdin().is_terminal()`), untrusted hooks should be silently skipped (not loaded, not prompted). This prevents blocking in scripted/piped usage. Add a `tracing::warn!` message listing skipped hooks.

- [ ] **2.5 Write integration tests** — Test the full discovery + verification flow with mock `UserInfra` and temp directories containing hook scripts. Cover: all-trusted, untrusted-then-trusted, tampered-detection, delete-option, ignore-option, non-interactive-skip.

### Phase 3: `forge hooks` CLI Command

- [ ] **3.1 Add `HookCommandGroup` and `HookCommand` to `crates/forge_main/src/cli.rs`** — Following the established pattern (e.g., `McpCommandGroup`):

  ```
  forge hooks list              — List all hooks with trust status
  forge hooks trust <path>      — Trust a hook by relative name or full path
  forge hooks delete <path>     — Delete a hook file and remove from trust store
  ```

  Struct definitions:
  - `HookCommandGroup` with `#[command(subcommand)] command: HookCommand` and `--porcelain` flag
  - `HookCommand` enum with `List`, `Trust { name: String }`, `Delete { name: String }` variants

  Add `Hook(HookCommandGroup)` variant to `TopLevelCommand` enum at `cli.rs:80-152`.

- [ ] **3.2 Add dispatch in `crates/forge_main/src/ui.rs`** — Add `TopLevelCommand::Hook(hook_group)` arm in `handle_subcommands()` at `ui.rs:417-738`, delegating to a new `handle_hook_command()` method.

- [ ] **3.3 Implement `handle_hook_command()` in `ui.rs`** — Handler method that:
  - **`list`**: Load trust store, discover all hooks from `~/.forge/hooks/toolcall-start.d/`, display table with columns: Name, Status (Trusted/Untrusted/Tampered), SHA-256 (first 16 chars). Use colored output: green for Trusted, red for Tampered, yellow for Untrusted. In `--porcelain` mode, output TSV.
  - **`trust <name>`**: Resolve the hook path, compute SHA-256, save to trust store. Print confirmation with hash.
  - **`delete <name>`**: Resolve the hook path, delete the file, remove from trust store. Print confirmation.

- [ ] **3.4 Add CLI tests in `cli.rs`** — Test parsing of `forge hooks list`, `forge hooks trust my-hook.sh`, `forge hooks delete my-hook.sh`, `forge hooks list --porcelain`.

## Verification Criteria

- [ ] `forge hooks list` shows all discovered hooks with correct trust status indicators
- [ ] `forge hooks trust <name>` correctly computes SHA-256 and saves to trust store
- [ ] `forge hooks delete <name>` removes both the file and trust store entry
- [ ] Untrusted hooks trigger an interactive prompt (Trust/Delete/Ignore) at **startup time only**
- [ ] Tampered hooks (hash mismatch) produce a high-danger warning at **startup time only**
- [ ] After startup, `intercept()` uses cached paths — zero disk I/O, zero TOCTOU risk
- [ ] Replacing a hook script **while the program is running** has no effect on the current session
- [ ] Non-interactive mode silently skips untrusted hooks without blocking
- [ ] All new code has tests following the three-step (fixture/actual/expected) pattern
- [ ] `cargo check` passes with no new warnings
- [ ] `cargo insta test --accept` passes for affected crates

## Potential Risks and Mitigations

1. **Trust store corruption**
   Mitigation: Use atomic file writes (write to temp file, then rename). If `trust.json` is malformed, log a warning and treat all hooks as untrusted rather than crashing.

2. **Breaking change for existing hook users**
   Mitigation: Hooks that existed before this feature have `Untrusted` status (no baseline). On first run, users get prompted to trust them. This is intentional — the whole point is to verify existing hooks.

3. **UserInfra not available in startup context**
   Mitigation: The `ForgeApp` already has access to infrastructure through `Services` trait which includes `UserInfra`. Pass it through to `load_and_verify_hooks()`. For non-interactive contexts, fall back to skipping untrusted hooks silently.

4. **Hook script deleted after startup**
   Mitigation: The cached path still exists in memory. If the file is deleted, `run_hook()` will fail with a "file not found" error, which is already handled gracefully (treated as "allow" passthrough, same as current behavior for non-zero exit codes). This is acceptable — the hook was verified at startup, and if it disappears mid-session, failing open is reasonable.

5. **New hooks added while program is running**
   Mitigation: They won't be picked up until the next session start. This is **by design** — new hooks must go through the trust verification flow. Users who add hooks mid-session can use `forge hooks trust <name>` and restart.

## Alternative Approaches

1. **Per-hook hash files (like RTK reference)**: Store `.hook.sha256` alongside each hook script. Rejected because managing multiple dotfiles across directories is harder to list/audit and doesn't scale well with multiple event types.

2. **Config-based trust list in `.forge.toml`**: Add a `[hooks.trusted]` section to the TOML config. Rejected because hooks are global (`~/.forge/hooks/`), not per-project, and mixing trust data with user config creates confusion.

3. **No interactive prompt, CLI-only trust management**: Skip the runtime prompt and require users to run `forge hooks trust` manually. Simpler but worse UX — users won't know about untrusted hooks until they silently fail. The interactive prompt provides immediate visibility.

4. **Runtime re-verification on every call**: Compute hash on every `intercept()` call. Rejected due to unnecessary disk I/O and still having a TOCTOU window between hash check and script execution.

## Files to Create/Modify

| File | Action | Purpose |
|------|--------|---------|
| `crates/forge_app/src/hooks/trust.rs` | CREATE | Trust store & integrity module |
| `crates/forge_app/src/hooks/loader.rs` | CREATE | Startup verification & loading logic |
| `crates/forge_app/src/hooks/mod.rs` | MODIFY | Export trust + loader modules |
| `crates/forge_app/src/hooks/external.rs` | MODIFY | Cache hooks, remove per-call discovery |
| `crates/forge_app/src/app.rs` | MODIFY | Call load_and_verify_hooks at startup |
| `crates/forge_main/src/cli.rs` | MODIFY | Add HookCommandGroup/HookCommand |
| `crates/forge_main/src/ui.rs` | MODIFY | Add dispatch + handler for hooks command |
