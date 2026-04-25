# Hook Security & Integrity Verification

## Objective

Add a security verification mechanism to the external hook system so that only user-trusted hook scripts are loaded and executed. This includes:
- SHA-256 integrity checking for all discovered hooks
- Interactive trust prompts when untrusted hooks are found at runtime
- A new `forge hooks` CLI command group for managing hook trust (list, trust, delete)
- High-danger alerts when trusted hooks are modified (hash mismatch)

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

### Phase 2: Runtime Hook Verification in ExternalHookInterceptor

- [ ] **2.1 Refactor `ExternalHookInterceptor` to accept a `UserInfra` dependency** — Change from unit struct `ExternalHookInterceptor` to a tuple struct `ExternalHookInterceptor<U>(Arc<U>)` where `U: UserInfra`. Add `new(infra: Arc<U>) -> Self`. This follows the project's service pattern (tuple struct with `Arc<U>`).

  Rationale: The interceptor needs to prompt the user when untrusted hooks are discovered. The `UserInfra` trait (`crates/forge_app/src/infra.rs:167-200`) provides `select_one()` for interactive selection.

- [ ] **2.2 Add trust verification to `discover_hooks()`** — Rename to `discover_and_verify_hooks()` or add a new method. The method should:
  1. Load `TrustStore`
  2. Discover all hook files as before
  3. For each hook, check `TrustStore::check()`:
     - `Trusted` → include in results
     - `Untrusted` → prompt user via `UserInfra::select_one()` with three options: "Trust", "Delete", "Ignore". If "Trust", compute hash, save to trust store, include in results. If "Delete", delete the file. If "Ignore", skip.
     - `Tampered { expected, actual }` → print HIGH DANGER warning to stderr: "Hook script has been modified! Expected hash: {expected}, Actual: {actual}. Recommendation: delete or re-trust via `forge hooks trust`." Then mark as untrusted in the store and skip the hook.
     - `Missing` → skip (hook was deleted externally)
  4. Return only verified-trusted hooks

- [ ] **2.3 Update `app.rs` hook assembly** — In `crates/forge_app/src/app.rs:150`, pass `UserInfra` to `ExternalHookInterceptor::new()`. The `ForgeApp` already has access to services that implement `UserInfra` through the infrastructure layer.

- [ ] **2.4 Handle non-interactive (piped/non-TTY) mode** — When stdin is not a TTY (checked via `std::io::stdin().is_terminal()`), untrusted hooks should be silently skipped (not loaded, not prompted). This prevents blocking in scripted/piped usage. Add a log message at debug level.

- [ ] **2.5 Write integration tests** — Test the full discovery + verification flow with mock `UserInfra` and temp directories containing hook scripts. Cover: all-trusted, untrusted-then-trusted, tampered-detection, delete-option, ignore-option.

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

### Phase 4: Startup Runtime Check

- [ ] **4.1 Add a startup integrity scan** — In `crates/forge_app/src/app.rs`, before the `ForgeApp::chat()` method constructs the `ExternalHookInterceptor`, perform a startup scan:
  1. Load trust store
  2. For each trusted hook, verify hash matches
  3. If any hook is tampered, print high-danger warning to stderr and mark as untrusted in the store
  4. Save updated trust store

  This catches tampering even before the user triggers a tool call. The scan should be fast (just hash computation for a few small scripts).

- [ ] **4.2 Wire the startup check into the app initialization flow** — Call the integrity scan in `app.rs` before constructing the orchestrator, or as a separate method called from the UI layer during `init_state()`.

## Verification Criteria

- [ ] `forge hooks list` shows all discovered hooks with correct trust status indicators
- [ ] `forge hooks trust <name>` correctly computes SHA-256 and saves to trust store
- [ ] `forge hooks delete <name>` removes both the file and trust store entry
- [ ] Untrusted hooks trigger an interactive prompt (Trust/Delete/Ignore) at runtime
- [ ] Tampered hooks (hash mismatch) produce a high-danger warning and are NOT executed
- [ ] Only trusted hooks with matching hashes are executed
- [ ] Non-interactive mode silently skips untrusted hooks without blocking
- [ ] Startup scan detects and reports tampered hooks before any tool call
- [ ] All new code has tests following the three-step (fixture/actual/expected) pattern
- [ ] `cargo check` passes with no new warnings
- [ ] `cargo insta test --accept` passes for affected crates

## Potential Risks and Mitigations

1. **Trust store corruption**
   Mitigation: Use atomic file writes (write to temp file, then rename). If `trust.json` is malformed, log a warning and treat all hooks as untrusted rather than crashing.

2. **Race condition between trust check and hook execution**
   Mitigation: Compute hash immediately before execution in the interceptor, not just at startup. The startup scan is an additional layer, not the sole check.

3. **Breaking change for existing hook users**
   Mitigation: Hooks that existed before this feature have `Untrusted` status (no baseline). On first run, users get prompted to trust them. This is intentional — the whole point is to verify existing hooks.

4. **UserInfra not available in ExternalHookInterceptor context**
   Mitigation: The `ForgeApp` already has access to infrastructure through `Services` trait which includes `UserInfra`. Pass it through the constructor. For non-interactive contexts, fall back to skipping untrusted hooks silently.

5. **Performance impact of hash computation**
   Mitigation: Hook scripts are typically small (< 10KB). SHA-256 computation on such files is negligible (< 1ms). Only compute at discovery time, not on every tool call.

## Alternative Approaches

1. **Per-hook hash files (like RTK reference)**: Store `.hook.sha256` alongside each hook script. Rejected because managing multiple dotfiles across directories is harder to list/audit and doesn't scale well with multiple event types.

2. **Config-based trust list in `.forge.toml`**: Add a `[hooks.trusted]` section to the TOML config. Rejected because hooks are global (`~/.forge/hooks/`), not per-project, and mixing trust data with user config creates confusion.

3. **No interactive prompt, CLI-only trust management**: Skip the runtime prompt and require users to run `forge hooks trust` manually. Simpler but worse UX — users won't know about untrusted hooks until they silently fail. The interactive prompt provides immediate visibility.

## Files to Create/Modify

| File | Action | Purpose |
|------|--------|---------|
| `crates/forge_app/src/hooks/trust.rs` | CREATE | Trust store & integrity module |
| `crates/forge_app/src/hooks/mod.rs` | MODIFY | Export trust module |
| `crates/forge_app/src/hooks/external.rs` | MODIFY | Add trust verification to interceptor |
| `crates/forge_app/src/app.rs` | MODIFY | Pass UserInfra, add startup scan |
| `crates/forge_main/src/cli.rs` | MODIFY | Add HookCommandGroup/HookCommand |
| `crates/forge_main/src/ui.rs` | MODIFY | Add dispatch + handler for hooks command |
