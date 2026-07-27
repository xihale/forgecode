use std::borrow::Cow;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use convert_case::{Case, Casing};
use derive_setters::Setters;
use forge_api::{AgentId, Effort, ModelId, Usage};
use nu_ansi_term::{Color, Style};

use crate::display_constants::markers;
use crate::editor::{AgentState, EffortState};
use crate::utils::humanize_number;

// Nerd font symbols — left prompt
const DIR_SYMBOL: &str = "\u{ea83}"; // 󪃃  folder icon
const BRANCH_SYMBOL: &str = "\u{f418}"; //   branch icon
const SUCCESS_SYMBOL: &str = "\u{f013e}"; // 󰄾  chevron

// Nerd font symbols — right prompt (ZSH rprompt)
const AGENT_SYMBOL: &str = "\u{f167a}";
const MODEL_SYMBOL: &str = "\u{ec19}";

/// Terminal width at which the reasoning effort label switches from the
/// compact three-letter form (e.g. `MED`) to the full uppercase label
/// (e.g. `MEDIUM`). Matches [`crate::zsh::rprompt`] so the CLI and zsh
/// integration render identically on equivalent terminals.
const WIDE_TERMINAL_THRESHOLD: usize = 100;

/// Very Specialized Prompt for the Agent Chat
#[derive(Clone, Setters)]
#[setters(strip_option, borrow_self)]
pub struct ForgePrompt {
    pub cwd: PathBuf,
    pub usage: Option<Usage>,
    pub agent_id: AgentId,
    pub model: Option<ModelId>,
    /// Currently configured reasoning effort level for the active model,
    /// rendered to the right of the model when set. `Effort::None` is
    /// suppressed (see [`ForgePrompt::render_prompt_right`]).
    pub reasoning_effort: Option<Effort>,
    /// Shared effort state cycled by Ctrl+T in the editor. When set, its
    /// `current` takes precedence over `reasoning_effort` for rendering so the
    /// prompt reflects the in-editor selection immediately.
    pub effort_state: Option<Arc<Mutex<EffortState>>>,
    /// Fallback effort (read from the API) used when `effort_state` is unset.
    pub effort: Option<Effort>,
    /// Shared agent state cycled by Ctrl+Q in the editor.
    pub agent_state: Option<Arc<Mutex<AgentState>>>,
    pub git_branch: Option<String>,
    /// Context window length for the active model, used to render the
    /// token-usage percentage (e.g. `47k (23%)`) in the right prompt.
    pub context_length: Option<u64>,
}

impl ForgePrompt {
    /// Creates a new `ForgePrompt`, resolving the git branch once at
    /// construction time.
    pub fn new(cwd: PathBuf, agent_id: AgentId) -> Self {
        let git_branch = get_git_branch();
        Self {
            cwd,
            usage: None,
            agent_id,
            model: None,
            reasoning_effort: None,
            effort_state: None,
            effort: None,
            agent_state: None,
            git_branch,
            context_length: None,
        }
    }

    pub fn refresh(&mut self) -> &mut Self {
        let git_branch = get_git_branch();
        self.git_branch = git_branch;
        self
    }

    pub fn render_prompt_left(&self) -> Cow<'_, str> {
        // Left prompt layout:
        //
        //   AGENT_NAME  󪃃 dir   branch
        //   󰄾
        //
        // Colors:
        //   agent  → bold white  (identifies the active agent)
        //   dir    → bold cyan
        //   branch → bold green
        //   chevron → bold green

        let dir_style = Style::new().fg(Color::Cyan).bold();
        let branch_style = Style::new().fg(Color::LightGreen).bold();
        let chevron_style = Style::new().fg(Color::LightGreen).bold();

        let current_dir = self
            .cwd
            .file_name()
            .and_then(|name| name.to_str())
            .map(String::from)
            .unwrap_or_else(|| markers::EMPTY.to_string());

        let mut result = String::with_capacity(80);

        // Directory — folder icon + name, bold cyan
        write!(
            result,
            "{}",
            dir_style.paint(format!("{DIR_SYMBOL} {current_dir}"))
        )
        .unwrap();

        // Git branch — branch icon + name, bold green (only when present and
        // different from the directory name, matching existing behaviour)
        if let Some(branch) = self.git_branch.as_deref()
            && branch != current_dir
        {
            write!(
                result,
                " {}",
                branch_style.paint(format!("{BRANCH_SYMBOL} {branch}"))
            )
            .unwrap();
        }

        // Second line: success chevron
        write!(result, "\n{} ", chevron_style.paint(SUCCESS_SYMBOL)).unwrap();

        Cow::Owned(result)
    }

    pub fn render_prompt_right(&self) -> Cow<'_, str> {
        // Right prompt layout: agent · tokens · cost · model
        // Active (tokens > 0): bright white for agent/tokens, green for cost
        // Inactive (no tokens): all segments dimmed

        let total_tokens = self.usage.as_ref().map(|u| u.total_tokens);
        let active = total_tokens.map(|t| *t > 0).unwrap_or(false);

        let agent_color = if active {
            Color::LightGray
        } else {
            Color::DarkGray
        };
        let mut result = String::with_capacity(64);

        // Agent name — read the current agent from the shared AgentState when
        // available (Ctrl+Q may have cycled it since the prompt was built).
        let agent_id = self
            .agent_state
            .as_ref()
            .and_then(|s| s.lock().ok())
            .map(|s| s.current.clone())
            .unwrap_or_else(|| self.agent_id.clone());
        let agent_str = format!(
            "{AGENT_SYMBOL} {}",
            agent_id.as_str().to_case(Case::UpperSnake)
        );
        write!(
            result,
            " {}",
            Style::new().bold().fg(agent_color).paint(&agent_str)
        )
        .unwrap();

        // Token count with context-window usage percentage (only when active).
        // e.g. `47k (23%)` — the percentage is shown only when a context
        // length is known and the usage is non-zero.
        if let Some(tokens) = total_tokens
            && active
        {
            let prefix = match tokens {
                forge_api::TokenCount::Actual(_) => "",
                forge_api::TokenCount::Approx(_) => "~",
            };
            let used = *tokens;
            let pct = self.context_length.filter(|cl| *cl > 0).map(|cl| {
                let p = (used as u64).min(cl) * 100 / cl;
                format!(" ({p}%)")
            });
            let count_str = format!("{}{}{}", prefix, humanize_number(used), pct.as_deref().unwrap_or(""));
            write!(
                result,
                " {}",
                Style::new().bold().fg(Color::LightGray).paint(&count_str)
            )
            .unwrap();
        }

        // Cost (only shown when active)
        if let Some(cost) = self.usage.as_ref().and_then(|u| u.cost)
            && active
        {
            let cost_str = format!("\u{f155}{cost:.2}");
            write!(
                result,
                " {}",
                Style::new().bold().fg(Color::Green).paint(&cost_str)
            )
            .unwrap();
        }

        // Model with nerd font symbol
        if let Some(model) = self.model.as_ref() {
            let model_str = model.to_string();
            let short_model = model_str.split('/').next_back().unwrap_or(model.as_str());
            let model_label = format!("{MODEL_SYMBOL} {short_model}");
            let color = if active {
                Color::LightMagenta
            } else {
                Color::DarkGray
            };
            write!(result, " {}", Style::new().fg(color).paint(&model_label)).unwrap();
        }

        // Reasoning effort — rendered to the right of the model, matching the
        // ZSH rprompt. `Effort::None` is suppressed (see zsh/rprompt.rs). On
        // narrow terminals the label collapses to its first three characters
        // so the prompt stays compact.
        //
        // When `effort_state` is set (editor Ctrl+T cycling), its `current`
        // takes precedence over the API-sourced `reasoning_effort`/`effort`,
        // and the label is only shown when more than one effort level is
        // supported (otherwise cycling is meaningless).
        let (effort, supported_count) = if let Some(ref state) = self.effort_state {
            let state = state.lock().ok();
            (
                state
                    .as_ref()
                    .and_then(|s| s.current.clone())
                    .or_else(|| self.effort.clone().or(self.reasoning_effort.clone())),
                state.as_ref().map(|s| s.supported.len()),
            )
        } else {
            (self.effort.clone().or(self.reasoning_effort.clone()), None)
        };

        if let Some(ref effort) = effort
            && !matches!(effort, Effort::None)
            && supported_count.unwrap_or(2) > 1
        {
            let effort_label = effort_label(effort, term_width());
            let color = if active {
                Color::Yellow
            } else {
                Color::DarkGray
            };
            write!(result, " {}", Style::new().fg(color).paint(&effort_label)).unwrap();
        }

        Cow::Owned(result)
    }

    pub fn render_prompt_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
}

/// Gets the current git branch name if available
fn get_git_branch() -> Option<String> {
    let repo = gix::discover(".").ok()?;
    let head = repo.head().ok()?;
    head.referent_name().map(|r| r.shorten().to_string())
}

/// Returns the current terminal width in columns, falling back to 80 when
/// the size cannot be detected.
fn term_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80)
}

/// Formats an [`Effort`] as a bracketed short label (e.g. `[M]`, `[XH]`),
/// matching the compact right-prompt style. `width` is accepted for
/// signature compatibility with callers but does not change the output —
/// the bracketed short form is used on all terminal widths.
fn effort_label(effort: &Effort, _width: usize) -> String {
    format!("[{}]", effort.short_name())
}

#[cfg(test)]
mod tests {
    use nu_ansi_term::Style;
    use pretty_assertions::assert_eq;

    use super::*;

    impl Default for ForgePrompt {
        fn default() -> Self {
            ForgePrompt {
                cwd: PathBuf::from("."),
                usage: None,
                agent_id: AgentId::default(),
            model: None,
            reasoning_effort: None,
            effort_state: None,
            effort: None,
            agent_state: None,
            git_branch: None,
            context_length: None,
            }
        }
    }

    enum PromptHistorySearchStatus {
        Passing,
        Failing,
    }

    struct PromptHistorySearch {
        status: PromptHistorySearchStatus,
        term: String,
    }

    fn render_prompt_history_search_indicator(
        history_search: PromptHistorySearch,
    ) -> Cow<'static, str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "",
            PromptHistorySearchStatus::Failing => "failing ",
        };

        let mut result = String::with_capacity(32);
        if history_search.term.is_empty() {
            write!(result, "({prefix}reverse-search) ").unwrap();
        } else {
            write!(
                result,
                "({}reverse-search: {}) ",
                prefix, history_search.term
            )
            .unwrap();
        }

        Cow::Owned(Style::new().fg(Color::White).paint(&result).to_string())
    }

    #[test]
    fn test_render_prompt_left() {
        let prompt = ForgePrompt::default();
        let actual = prompt.render_prompt_left();

        // Starship directory icon present
        assert!(actual.contains(DIR_SYMBOL));
        // Starship success chevron present
        assert!(actual.contains(SUCCESS_SYMBOL));
    }

    #[test]
    fn test_render_prompt_left_with_branch() {
        let prompt = ForgePrompt { git_branch: Some("main".to_string()), ..Default::default() };
        let actual = prompt.render_prompt_left();

        // Agent name is on the right prompt, not the left
        // Branch icon and name present
        assert!(actual.contains(BRANCH_SYMBOL));
        assert!(actual.contains("main"));
    }

    #[test]
    fn test_render_prompt_right_inactive() {
        // No tokens → dimmed agent + model, no token/cost segments
        let mut prompt = ForgePrompt::default();
        let _ = prompt.model(ModelId::new("gpt-4"));

        let actual = prompt.render_prompt_right();
        // Agent symbol and name present
        assert!(actual.contains(AGENT_SYMBOL));
        assert!(actual.contains("FORGE"));
        // Model symbol and name present
        assert!(actual.contains(MODEL_SYMBOL));
        assert!(actual.contains("gpt-4"));
        // No token count text in inactive state (no humanized number segment)
        assert!(!actual.contains("1k") && !actual.contains("~"));
    }

    #[test]
    fn test_render_prompt_right_active_with_tokens() {
        // Tokens > 0 → active colours; approx tokens show "~" prefix
        let usage = Usage {
            prompt_tokens: forge_api::TokenCount::Actual(10),
            completion_tokens: forge_api::TokenCount::Actual(20),
            total_tokens: forge_api::TokenCount::Approx(30),
            ..Default::default()
        };
        let mut prompt = ForgePrompt::default();
        let _ = prompt.usage(usage);

        let actual = prompt.render_prompt_right();
        assert!(actual.contains("~30"));
        assert!(actual.contains(AGENT_SYMBOL));
    }

    #[test]
    fn test_render_prompt_history_search_indicator_passing() {
        let history_search = PromptHistorySearch {
            status: PromptHistorySearchStatus::Passing,
            term: "test".to_string(),
        };
        let actual = render_prompt_history_search_indicator(history_search);
        let expected = Style::new()
            .fg(Color::White)
            .paint("(reverse-search: test) ")
            .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_history_search_indicator_failing() {
        let history_search = PromptHistorySearch {
            status: PromptHistorySearchStatus::Failing,
            term: "test".to_string(),
        };
        let actual = render_prompt_history_search_indicator(history_search);
        let expected = Style::new()
            .fg(Color::White)
            .paint("(failing reverse-search: test) ")
            .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_history_search_indicator_empty_term() {
        let history_search = PromptHistorySearch {
            status: PromptHistorySearchStatus::Passing,
            term: "".to_string(),
        };
        let actual = render_prompt_history_search_indicator(history_search);
        let expected = Style::new()
            .fg(Color::White)
            .paint("(reverse-search) ")
            .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_right_strips_provider_prefix() {
        // Model ID like "anthropic/claude-3" should show only "claude-3"
        let usage = Usage {
            prompt_tokens: forge_api::TokenCount::Actual(10),
            completion_tokens: forge_api::TokenCount::Actual(20),
            total_tokens: forge_api::TokenCount::Actual(30),
            ..Default::default()
        };
        let mut prompt = ForgePrompt::default();
        let _ = prompt.usage(usage);
        let _ = prompt.model(ModelId::new("anthropic/claude-3"));

        let actual = prompt.render_prompt_right();
        assert!(actual.contains("claude-3"));
        assert!(!actual.contains("anthropic/claude-3"));
        assert!(actual.contains("30"));
    }

    #[test]
    fn test_render_prompt_right_with_cost() {
        // Cost shown when active
        let usage = Usage {
            total_tokens: forge_api::TokenCount::Actual(1500),
            cost: Some(0.01),
            ..Default::default()
        };
        let mut prompt = ForgePrompt::default();
        let _ = prompt.usage(usage);

        let actual = prompt.render_prompt_right();
        assert!(actual.contains("0.01"));
        assert!(actual.contains("1.5k"));
    }

    #[test]
    fn test_render_prompt_right_token_usage_percentage() {
        // `47k (23%)` — percentage shown when context_length is set.
        let usage = Usage {
            total_tokens: forge_api::TokenCount::Actual(47_000),
            ..Default::default()
        };
        let mut prompt = ForgePrompt::default();
        let _ = prompt.usage(usage).context_length(200_000);

        let actual = prompt.render_prompt_right();
        assert!(actual.contains("47k"));
        assert!(actual.contains("(23%)"));

        // Without a context length the percentage is suppressed.
        let mut prompt = ForgePrompt::default();
        let _ = prompt.usage(usage);
        let actual = prompt.render_prompt_right();
        assert!(!actual.contains("%"));
    }

    #[test]
    fn test_render_prompt_right_with_reasoning_effort() {
        // When reasoning effort is set, its bracketed short label appears
        // after the model segment.
        let mut prompt = ForgePrompt::default();
        let _ = prompt.model(ModelId::new("gpt-4"));
        let _ = prompt.reasoning_effort(Effort::High);

        let actual = prompt.render_prompt_right();
        assert!(actual.contains("[H]"));
    }

    #[test]
    fn test_render_prompt_right_hides_effort_none() {
        // `Effort::None` carries no useful info — it must not be rendered.
        let mut prompt = ForgePrompt::default();
        let _ = prompt.model(ModelId::new("gpt-4"));
        let _ = prompt.reasoning_effort(Effort::None);

        let actual = prompt.render_prompt_right();
        assert!(!actual.to_uppercase().contains("NONE"));
    }

    #[test]
    fn test_effort_label_narrow_vs_wide() {
        // Bracketed short form on all widths.
        assert_eq!(effort_label(&Effort::Medium, 80), "[M]");
        assert_eq!(effort_label(&Effort::Medium, WIDE_TERMINAL_THRESHOLD), "[M]");
        assert_eq!(effort_label(&Effort::XHigh, 80), "[XH]");
    }
}
