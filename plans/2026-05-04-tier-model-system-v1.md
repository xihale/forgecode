
# Tier Model System for ForgeCode

## Objective

Introduce a **tier** (层级) mechanism that replaces the current hardcoded `session`/`shell`/`commit`/`suggest` config fields with named tiers (`lite`, `normal`, `heavy`, `sage`). Each entry point (shell, commit, sage, muse) maps to a specific tier, and users can configure each tier independently with a provider+model pair.

## Current Architecture

### How it works today

```
ForgeConfig {
    session: Option<ModelConfig>,   // default for interactive sessions
    shell:   Option<ModelConfig>,   // override for shell mode (:model)
    commit:  Option<ModelConfig>,   // override for :commit
    suggest: Option<ModelConfig>,   // override for :suggest
}
```

**Resolution chain:**
- `get_session_config()` → shell mode ? `shell ?? session` : `session`
- `get_commit_config()` → `commit` (no fallback)
- `get_suggest_config()` → `suggest` (no fallback)
- Agent loading: agents without explicit provider/model inherit from `get_session_config()`
- `AgentProviderResolver` → agent.provider/model (baked at load time) or `get_session_config()`

**Entry points → config field mapping:**
| Entry point | Config field used |
|---|---|
| Interactive chat (UI) | `session` |
| Shell `: text` | `shell ?? session` |
| `:commit` | `commit ?? session` |
| `:suggest` | `suggest ?? session` |
| `:sage` | `session` (via agent) |
| `:muse` | `session` (via agent) |

## Proposed Architecture

### New tier system

```toml
# ~/.forge/.forge.toml

[tiers.lite]
provider_id = "OpenRouter"
model_id = "tencent/hy3-preview:free"

[tiers.normal]
provider_id = "DeepSeek"
model_id = "deepseek-chat"

[tiers.heavy]
provider_id = "Anthropic"
model_id = "claude-sonnet-4-20250514"

[tiers.sage]
provider_id = "OpenAI"
model_id = "o3"
```

### Entry point → tier mapping

| Entry point | Tier | Rationale |
|---|---|---|
| `: text` (shell default) | `lite` | Fast, cheap model for quick shell prompts |
| `:forge text` / interactive | `normal` | Balanced model for coding tasks |
| `:commit` | `lite` | Simple generation, cheap model suffices |
| `:suggest` | `lite` | Simple generation, cheap model suffices |
| `:sage text` | `sage` | Research needs powerful reasoning |
| `:muse text` | `heavy` | Planning needs strong model |

### Backward compatibility

The old fields (`session`, `shell`, `commit`, `suggest`) are deprecated but still read for migration:

- `session` → maps to `normal` tier
- `shell` → maps to `lite` tier
- `commit` → maps to `lite` tier
- `suggest` → maps to `lite` tier

If both old and new config exist, new takes precedence.

## Implementation Plan

### Phase 1: Config Layer — Add tier definitions

- [ ] **1.1** Add `TierConfig` struct to `forge_config` (mirrors existing `ModelConfig` with `provider_id` + `model_id`)
- [ ] **1.2** Add `TiersConfig` struct as a `HashMap<String, TierConfig>` with predefined tier names as constants (`LITE`, `NORMAL`, `HEAVY`, `SAGE`)
- [ ] **1.3** Add `tiers: Option<TiersConfig>` field to `ForgeConfig` (alongside existing fields)
- [ ] **1.4** Add migration logic: when `tiers` is absent but `session`/`shell`/`commit`/`suggest` exist, auto-populate tiers from old fields
- [ ] **1.5** Add `ForgeConfig::get_tier(&self, name: &str) -> Option<&TierConfig>` method that checks tiers first, then falls back to legacy fields

### Phase 2: Domain Layer — Tier enum and resolution

- [ ] **2.1** Add `Tier` enum to `forge_domain` with variants: `Lite`, `Normal`, `Heavy`, `Sage`
- [ ] **2.2** Add `Tier::default_for_agent(agent_id: &AgentId) -> Tier` method:
  - `forge` → `Normal`
  - `sage` → `Sage`
  - `muse` → `Heavy`
- [ ] **2.3** Add `Tier::default_for_command(command: &str) -> Tier` method:
  - `commit` → `Lite`
  - `suggest` → `Lite`
  - shell prompt → `Lite`
- [ ] **2.4** Add `ConfigOperation::SetTierConfig { tier: String, config: ModelConfig }` variant
- [ ] **2.5** Add `ConfigOperation::ClearTierConfig { tier: String }` variant (or use `Option<ModelConfig>` like `SetCommitConfig`)

### Phase 3: Service Layer — Tier-aware config resolution

- [ ] **3.1** Add `get_tier_config(tier: &str) -> Option<ModelConfig>` to `AppConfigService` trait
- [ ] **3.2** Implement in `ForgeAppConfigService` using `ForgeConfig::get_tier()`
- [ ] **3.3** Refactor `get_session_config()` to resolve via tier: `get_tier_config("normal")` (falling back to legacy `session` field)
- [ ] **3.4** Refactor `get_commit_config()` to resolve via tier: `get_tier_config("lite")` (falling back to legacy `commit` field)
- [ ] **3.5** Refactor `get_suggest_config()` to resolve via tier: `get_tier_config("lite")` (falling back to legacy `suggest` field)
- [ ] **3.6** Refactor `get_shell_config()` to resolve via tier: `get_tier_config("lite")` (falling back to legacy `shell` field)

### Phase 4: Agent Loading — Per-agent tier resolution

- [ ] **4.1** Modify `ForgeAgentRepository::get_agents()` to resolve tier per agent based on `Tier::default_for_agent()`
- [ ] **4.2** Each agent gets its provider/model from the tier it maps to, not a single `session` config
- [ ] **4.3** Agents with explicit `provider`/`model` in their definition continue to override tier defaults

### Phase 5: CLI — `forge config set tier`

- [ ] **5.1** Add `Tier { name, provider, model }` variant to `ConfigSetField` enum in `cli.rs`
- [ ] **5.2** Add `Tier { name }` variant to `ConfigGetField` enum
- [ ] **5.3** Implement `handle_config_set` for the `Tier` variant
- [ ] **5.4** Implement `handle_config_get` for the `Tier` variant
- [ ] **5.5** Add `forge config list` output to show tier mappings

### Phase 6: Shell Plugin — `:tier` command and entry-point mapping

- [ ] **6.1** Add `:tier` (`:t` is taken by `:tools`, use something else) shell command for selecting a model for a specific tier (e.g., `:tier lite` opens model picker, sets `tiers.lite`)
- [ ] **6.2** Update `_forge_action_shell_model` (`:model`) to set `tiers.lite` instead of `shell`
- [ ] **6.3** Update `_forge_action_commit_model` (`:ccm`) to set `tiers.lite` instead of `commit`
- [ ] **6.4** Update `_forge_action_suggest_model` (`:csm`) to set `tiers.lite` instead of `suggest`
- [ ] **6.5** Update `_forge_action_model` (`:cm`) to set `tiers.normal` instead of `session`
- [ ] **6.6** Apply same changes to fish plugin (`shell-plugin/fish/actions/config.fish`)
- [ ] **6.7** Update `_forge_action_commit` and `_forge_action_commit_preview` to pass `FORGE_TIER=lite` env var (or rely on the service layer defaulting correctly)

### Phase 7: Environment variable override

- [ ] **7.1** Support `FORGE_TIERS__LITE__PROVIDER_ID` and `FORGE_TIERS__LITE__MODEL_ID` env var overrides (already works via `config` crate's `__` separator)
- [ ] **7.2** Add session-level override via `_FORGE_SESSION_TIER_LITE_MODEL` etc. in shell plugin (or simpler: `_FORGE_SESSION_TIER_<NAME>` env var)

### Phase 8: Update existing tests and add new tests

- [ ] **8.1** Update `app_config.rs` tests to verify tier-based resolution
- [ ] **8.2** Add tests for backward compatibility (old fields → tier mapping)
- [ ] **8.3** Add tests for tier priority (new tier config overrides legacy field)
- [ ] **8.4** Add tests for per-agent tier resolution
- [ ] **8.5** Update snapshot tests affected by config changes

## Verification Criteria

- [ ] `forge config set tier lite OpenRouter tencent/hy3-preview:free` works
- [ ] `forge config get tier lite` returns the correct provider/model
- [ ] `:model` in shell sets `tiers.lite` and is immediately used for shell prompts
- [ ] `:commit` uses the `lite` tier model (not hardcoded `commit` field)
- [ ] `:sage` uses the `sage` tier model
- [ ] `:muse` uses the `heavy` tier model
- [ ] Old config with `session`/`shell`/`commit`/`suggest` still works (backward compat)
- [ ] `forge config list` shows all tier mappings
- [ ] All existing tests pass
- [ ] `cargo check` succeeds

## Potential Risks and Mitigations

1. **Backward compatibility breakage**
   Mitigation: Keep legacy fields as fallback; migration is automatic when reading config. Only write to new `tiers` field going forward.

2. **Complexity in resolution chain**
   Mitigation: Centralize tier resolution in a single method (`ForgeConfig::get_tier()`) rather than scattering logic across multiple services.

3. **Agent definition files hardcoding provider/model**
   Mitigation: Agent-level provider/model already override session defaults; this pattern naturally extends to tier defaults. No change needed.

4. **Shell plugin state management**
   Mitigation: Session env vars (`_FORGE_SESSION_MODEL` etc.) continue to work as session-level overrides. The tier system just changes what the "default" is.

5. **Config file format migration**
   Mitigation: Read old format indefinitely. Only write new format. Add `forge config migrate` if needed.

## Alternative Approaches

1. **Keep flat fields, add aliases**: Instead of a `tiers` map, add fields like `tier_lite`, `tier_normal`, etc. Simpler config but less extensible.

2. **Agent-level tier field**: Let each agent definition specify its tier in YAML frontmatter. More flexible but requires editing agent files for tier changes.

3. **Named profiles instead of tiers**: A general-purpose "profile" system where users define profiles and assign them to entry points. More powerful but more complex to configure.

4. **Minimal change: just rename `session`→`normal`, `shell`→`lite`**: Least effort but doesn't add the `sage`/`heavy` tiers or the per-agent mapping flexibility.
