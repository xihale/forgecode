use std::borrow::Cow;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use console::{measure_text_width, strip_ansi_codes};
use forge_api::{AgentId, Effort, Environment};
use nu_ansi_term::Style;
use rustyline::completion::{Completer, Pair};
use rustyline::config::{ColorMode, CompletionType, Config};
use rustyline::error::ReadlineError as RustyReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{
    Cmd, ConditionalEventHandler, Context as RustylineContext, Editor, EventHandler, Helper,
    KeyCode, KeyEvent, Modifiers, Prompt as RustylinePrompt, RepeatCount,
};

use super::completer::InputCompleter;
use super::zsh::paste::wrap_pasted_text;
use crate::clipboard::paste_image_from_clipboard;
use crate::highlighter::ForgeHighlighter;
use crate::model::ForgeCommandManager;
use crate::prompt::ForgePrompt;

const HISTORY_CAPACITY: usize = 1024 * 1024;

/// Shared reasoning-effort state, cycled by Ctrl+T in the editor and read by
/// the prompt renderer. The local `current` is synced to/from the API on each
/// prompt iteration (see [`crate::input::Console`]).
#[derive(Debug, Clone, Default)]
pub struct EffortState {
    /// The currently selected effort (updated by Ctrl+T, read by the prompt
    /// renderer and `Console`).
    pub current: Option<Effort>,
    /// Effort levels supported by the active model.
    pub supported: Vec<Effort>,
}

impl EffortState {
    /// Cycles `current` through `supported`, wrapping around.
    pub fn cycle(&mut self) {
        if self.supported.is_empty() {
            return;
        }

        let efforts = &self.supported;
        let current = self.current.as_ref().cloned().unwrap_or(Effort::Medium);
        let next = if let Some(pos) = efforts.iter().position(|e| e == &current) {
            efforts[(pos + 1) % efforts.len()].clone()
        } else {
            efforts.first().cloned().unwrap_or(Effort::Medium)
        };
        self.current = Some(next);
    }
}

/// Shared state for Ctrl+Q agent cycling.
#[derive(Debug, Clone, Default)]
pub struct AgentState {
    /// The currently selected agent (updated by Ctrl+Q, read by prompt renderer
    /// and `Console`).
    pub current: AgentId,
}

impl AgentState {
    /// Creates a new `AgentState` with the given initial agent.
    pub fn new(initial: AgentId) -> Self {
        Self { current: initial }
    }

    /// Cycles through the standard agent order: forge ↔ muse.
    pub fn cycle(&mut self) {
        self.current = if self.current == AgentId::FORGE {
            AgentId::MUSE
        } else {
            AgentId::FORGE
        };
    }
}

/// Interactive terminal editor used by the Forge prompt.
pub struct ForgeEditor {
    editor: Editor<ForgeHelper, DefaultHistory>,
    history_file: PathBuf,
    pending_buffer: Option<String>,
}

/// Result of reading one prompt interaction from the terminal.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadResult {
    Success(String),
    Empty,
    Continue,
    Exit,
}

impl ForgeEditor {
    /// Creates a new interactive editor with history, completion, and
    /// highlighting.
    pub fn new(
        env: Environment,
        custom_history_path: Option<PathBuf>,
        manager: Arc<ForgeCommandManager>,
        effort_state: Arc<Mutex<EffortState>>,
        agent_state: Arc<Mutex<AgentState>>,
    ) -> Self {
        let history_file = env.history_path(custom_history_path.as_ref());
        let helper = ForgeHelper::new(env.cwd, manager);
        let config = Config::builder()
            .max_history_size(HISTORY_CAPACITY)
            .expect("rustyline history capacity should be valid")
            .completion_type(CompletionType::List)
            .completion_show_all_if_ambiguous(true)
            .color_mode(ColorMode::Forced)
            .enable_signals(true)
            .build();
        let mut editor = Editor::<ForgeHelper, DefaultHistory>::with_config(config)
            .expect("rustyline editor should initialize for an interactive terminal");
        editor.bind_sequence(
            KeyEvent(KeyCode::Enter, Modifiers::ALT),
            EventHandler::Simple(Cmd::Newline),
        );
        editor.bind_sequence(
            KeyEvent(KeyCode::Char('k'), Modifiers::CTRL),
            EventHandler::Simple(Cmd::ClearScreen),
        );
        editor.bind_sequence(
            KeyEvent(KeyCode::Char('K'), Modifiers::CTRL),
            EventHandler::Simple(Cmd::ClearScreen),
        );
        // Ctrl+T: cycle reasoning effort (local state; synced to API on Enter).
        editor.bind_sequence(
            KeyEvent(KeyCode::Char('t'), Modifiers::CTRL),
            EventHandler::Conditional(Box::new(EffortCycleHandler(effort_state))),
        );
        // Ctrl+Q: cycle between forge and muse agent.
        editor.bind_sequence(
            KeyEvent(KeyCode::Char('q'), Modifiers::CTRL),
            EventHandler::Conditional(Box::new(AgentCycleHandler(agent_state))),
        );
        // Ctrl+V: paste image from clipboard as @[path] attachment.
        editor.bind_sequence(
            KeyEvent(KeyCode::Char('v'), Modifiers::CTRL),
            EventHandler::Conditional(Box::new(ImagePasteHandler)),
        );
        editor.set_helper(Some(helper));
        let _ = editor.load_history(&history_file);
        Self { editor, history_file, pending_buffer: None }
    }

    fn normalize_result(&mut self, buffer: String) -> ReadResult {
        let result = normalize_result_text(buffer);
        if let ReadResult::Success(text) = &result {
            let _ = self.editor.add_history_entry(text.as_str());
            let _ = self.editor.save_history(&self.history_file);
        }
        result
    }

    /// Reads one logical input from the terminal.
    pub fn prompt(&mut self, prompt: &mut ForgePrompt) -> anyhow::Result<ReadResult> {
        let prompt_text = render_prompt(prompt);
        let initial = self.pending_buffer.take().unwrap_or_default();
        let readline = if initial.is_empty() {
            self.editor.readline(&prompt_text)
        } else {
            self.editor
                .readline_with_initial(&prompt_text, (&initial, ""))
        };
        prompt.refresh();

        match readline {
            Ok(buffer) => Ok(self.normalize_result(buffer)),
            Err(RustyReadlineError::Interrupted) => Ok(ReadResult::Continue),
            Err(RustyReadlineError::Eof) => Ok(ReadResult::Exit),
            Err(error) => Err(anyhow::anyhow!(ReadLineError(error))),
        }
    }

    /// Sets the buffer content to be pre-filled on the next prompt.
    pub fn set_buffer(&mut self, content: String) {
        self.pending_buffer = Some(content);
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to read line from terminal: {0}")]
pub struct ReadLineError(RustyReadlineError);

fn normalize_result_text(buffer: String) -> ReadResult {
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        return ReadResult::Empty;
    }
    ReadResult::Success(wrap_pasted_text(trimmed))
}

fn render_prompt(prompt: &ForgePrompt) -> ResponsivePrompt {
    let left = prompt.render_prompt_left();
    let indicator = prompt.render_prompt_indicator();

    // `raw` is what rustyline measures to position the cursor; `styled` is what
    // it prints. `raw` MUST be free of ANSI escapes: rustyline's Windows
    // console backend computes cursor columns by counting grapheme widths of
    // `raw` (it cannot interpret escape sequences and debug-asserts against
    // them), so any styling left in `raw` is counted as visible width and
    // pushes the cursor past where the text actually is. The left prompt and
    // indicator are styled via `nu_ansi_term`, so strip those codes for `raw`.
    // The right prompt is positioned off to the side with cursor save/restore
    // and is not part of the input-line geometry, so it is excluded from `raw`
    // entirely.
    let raw = strip_ansi_codes(&format!("{left}{indicator}")).into_owned();

    // Snapshot the ForgePrompt so `styled()` can re-render the right prompt on
    // every repaint. `effort_state`/`agent_state` are `Arc<Mutex<...>>`, so the
    // clone is a shallow copy and reads the live values cycled by Ctrl+T/Q —
    // letting the right prompt reflect the in-editor selection immediately,
    // without waiting for Enter.
    let prompt = Arc::new(prompt.clone());
    let styled_cache = RefCell::new(String::new());
    ResponsivePrompt {
        raw,
        left: left.into_owned(),
        indicator: indicator.into_owned(),
        prompt,
        styled_cache,
    }
}

fn render_right_prompt(right: &str) -> String {
    let width = measure_text_width(strip_ansi_codes(right).as_ref());
    format!("\x1b[s\x1b[999C\x1b[{width}D{right}\x1b[K\x1b[u")
}

/// Builds the styled prompt string from its static left/indicator parts and a
/// freshly rendered right prompt. Returns `Cow` to avoid allocating when the
/// right prompt is empty.
fn build_styled(left: &str, indicator: &str, right: &str) -> String {
    if right.trim().is_empty() {
        return format!("{left}{indicator}");
    }
    if let Some((first_line, remaining)) = left.split_once('\n') {
        let right = render_right_prompt(right);
        format!("{first_line}{right}\n{remaining}{indicator}")
    } else {
        let right = render_right_prompt(right);
        format!("{left}{right}{indicator}")
    }
}

struct ResponsivePrompt {
    /// Static, ANSI-stripped geometry used by rustyline for cursor math.
    /// Excludes the right prompt (it is positioned independently and is not
    /// part of the input-line geometry).
    raw: String,
    /// Static left prompt (with ANSI styling) and indicator.
    left: String,
    indicator: String,
    /// Snapshot of the prompt used to re-render the right prompt on each
    /// repaint, picking up Ctrl+T/Q state changes via its shared
    /// `effort_state`/`agent_state`.
    prompt: Arc<ForgePrompt>,
    /// Latest `styled()` output. `styled()` returns `&str` borrowing `&self`,
    /// so we cache the rendered string here and hand out a reference. Rustyline
    /// calls `styled()` synchronously from a single thread during repaint, so
    /// no concurrent access occurs.
    styled_cache: RefCell<String>,
}

impl RustylinePrompt for ResponsivePrompt {
    fn raw(&self) -> &str {
        &self.raw
    }

    fn styled(&self) -> &str {
        // Re-render the right prompt every call so Ctrl+T/Q changes show up
        // immediately on repaint (without waiting for Enter).
        let right = self.prompt.render_prompt_right();
        let right = right.trim_start();
        let styled = build_styled(&self.left, &self.indicator, right);
        let mut cache = self.styled_cache.borrow_mut();
        *cache = styled;
        // Return a reference into the RefCell's String. Safe because rustyline
        // invokes `styled()` single-threaded and does not hold overlapping
        // borrows: we drop the mutable borrow above before returning.
        drop(cache);
        // SAFETY: no other borrow of `styled_cache` is active (rustyline is
        // single-threaded in the prompt path), so the pointer stays valid
        // until the next `styled()` call overwrites it.
        unsafe { &*self.styled_cache.as_ptr() }
    }
}

struct ForgeHelper {
    completer: Mutex<InputCompleter>,
    highlighter: ForgeHighlighter,
    hinter: HistoryHinter,
}

impl ForgeHelper {
    fn new(cwd: PathBuf, command_manager: Arc<ForgeCommandManager>) -> Self {
        Self {
            completer: Mutex::new(InputCompleter::new(cwd, command_manager)),
            highlighter: ForgeHighlighter,
            hinter: HistoryHinter {},
        }
    }
}

impl Helper for ForgeHelper {}

impl Completer for ForgeHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &RustylineContext<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let mut completer = self
            .completer
            .lock()
            .expect("input completer mutex poisoned");
        let suggestions = completer.complete(line, pos);
        let start = suggestions
            .iter()
            .map(|suggestion| suggestion.span.start)
            .min()
            .unwrap_or(pos);
        let pairs = suggestions
            .into_iter()
            .map(|suggestion| {
                let replacement = if suggestion.append_whitespace {
                    format!("{} ", suggestion.value)
                } else {
                    suggestion.value
                };
                Pair { display: replacement.clone(), replacement }
            })
            .collect();
        Ok((start, pairs))
    }
}

impl Hinter for ForgeHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &RustylineContext<'_>) -> Option<Self::Hint> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Highlighter for ForgeHelper {
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        let styled = self.highlighter.highlight(line, pos);
        if styled.buffer.is_empty() {
            return Cow::Borrowed(line);
        }

        let default_style = Style::new();
        let mut rendered = String::with_capacity(line.len());
        for (style, text) in styled.buffer {
            if style == default_style {
                rendered.push_str(&text);
            } else {
                rendered.push_str(&style.paint(text).to_string());
            }
        }
        Cow::Owned(rendered)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(Style::new().dimmed().paint(hint).to_string())
    }
}

impl Validator for ForgeHelper {}

/// Ctrl+T handler: cycles the shared [`EffortState`] and forces a repaint so
/// the prompt reflects the new effort immediately.
struct EffortCycleHandler(Arc<Mutex<EffortState>>);

impl ConditionalEventHandler for EffortCycleHandler {
    fn handle(
        &self,
        _evt: &rustyline::Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &rustyline::EventContext,
    ) -> Option<Cmd> {
        if let Ok(mut state) = self.0.lock() {
            state.cycle();
        }
        Some(Cmd::Repaint)
    }
}

/// Ctrl+Q handler: cycles the shared [`AgentState`] between forge and muse.
struct AgentCycleHandler(Arc<Mutex<AgentState>>);

impl ConditionalEventHandler for AgentCycleHandler {
    fn handle(
        &self,
        _evt: &rustyline::Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &rustyline::EventContext,
    ) -> Option<Cmd> {
        if let Ok(mut state) = self.0.lock() {
            state.cycle();
        }
        Some(Cmd::Repaint)
    }
}

/// Ctrl+V handler: reads an image from the system clipboard and inserts it as
/// an `@[path]` attachment. If no image is available (or no clipboard tool is
/// installed), does nothing.
struct ImagePasteHandler;

impl ConditionalEventHandler for ImagePasteHandler {
    fn handle(
        &self,
        _evt: &rustyline::Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &rustyline::EventContext,
    ) -> Option<Cmd> {
        match paste_image_from_clipboard() {
            Ok(path) => Some(Cmd::Insert(1, format!("@[{}] ", path.display()))),
            Err(_) => Some(Cmd::Noop),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_normalize_result_wraps_existing_pasted_path() {
        let fixture = "/usr/bin/env".to_string();

        let actual = normalize_result_text(fixture);

        let expected = ReadResult::Success("@[/usr/bin/env]".to_string());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_render_prompt_raw_has_no_ansi_escapes() {
        use std::path::PathBuf;

        use forge_api::{AgentId, ModelId};

        // rustyline measures `raw()` to position the cursor and, on Windows,
        // cannot interpret ANSI escapes (it counts their bytes as visible
        // columns). `raw()` must therefore be free of escape sequences even
        // though the visible prompt is styled.
        let mut prompt = ForgePrompt::new(PathBuf::from("project"), AgentId::default());
        prompt.model(ModelId::new("anthropic/claude-opus-4"));

        let rendered = render_prompt(&prompt);

        assert!(
            !rendered.raw.contains('\u{1b}'),
            "raw prompt must not contain ANSI escape sequences: {:?}",
            rendered.raw
        );
        // The styled prompt, by contrast, does carry styling for display.
        assert!(rendered.styled().contains('\u{1b}'));
    }

    #[test]
    fn test_styled_reflects_live_effort_state_change() {
        use std::path::PathBuf;
        use std::sync::{Arc, Mutex};

        use forge_api::{AgentId, Effort, ModelId};

        // The right prompt is rendered lazily by `styled()`, reading the
        // shared `EffortState` each call. Cycling the effort between two
        // `styled()` calls must change the rendered effort label without
        // rebuilding the prompt.
        let effort_state = Arc::new(Mutex::new(EffortState {
            current: Some(Effort::Low),
            supported: vec![Effort::Low, Effort::Medium, Effort::High],
        }));

        let mut prompt = ForgePrompt::new(PathBuf::from("."), AgentId::default());
        prompt.model(ModelId::new("gpt-4"));
        prompt.effort_state(effort_state.clone());

        let rendered = render_prompt(&prompt);
        let before = rendered.styled().to_string();
        assert!(
            before.contains("[L]"),
            "expected initial effort [L], got: {before:?}"
        );

        // Cycle the shared state — no re-render of the prompt object itself.
        effort_state.lock().unwrap().cycle();

        let after = rendered.styled().to_string();
        assert!(
            after.contains("[M]"),
            "expected cycled effort [M], got: {after:?}"
        );
        assert_ne!(before, after, "styled() must reflect the live state");
    }

    #[test]
    fn test_styled_reflects_live_agent_state_change() {
        use std::path::PathBuf;
        use std::sync::{Arc, Mutex};

        use forge_api::{AgentId, ModelId};

        // Ctrl+Q cycles the shared AgentState; the rendered agent name must
        // update on the next `styled()` call without rebuilding the prompt.
        let agent_state = Arc::new(Mutex::new(AgentState::new(AgentId::FORGE)));

        let mut prompt = ForgePrompt::new(PathBuf::from("."), AgentId::default());
        prompt.model(ModelId::new("gpt-4"));
        prompt.agent_state(agent_state.clone());

        let rendered = render_prompt(&prompt);
        let before = rendered.styled().to_string();
        assert!(
            before.contains("FORGE"),
            "expected initial agent FORGE, got: {before:?}"
        );

        agent_state.lock().unwrap().cycle();

        let after = rendered.styled().to_string();
        assert!(
            after.contains("MUSE"),
            "expected cycled agent MUSE, got: {after:?}"
        );
    }
}
