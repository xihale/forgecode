use std::fmt::Display;
use std::path::PathBuf;

use anyhow::{Context, Result};
use forge_api::{API, ForgeAPI};
use forge_config::ForgeConfig;
use forge_select::ForgeWidget;

use crate::cli::{SelectCommand, SelectCommandGroup};

/// Runs a `forge select` subcommand.
///
/// Each variant fetches data through the Forge API, presents an interactive
/// fuzzy picker, and prints the selected value to stdout for the shell plugin
/// to consume.
pub async fn run(group: SelectCommandGroup) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config = ForgeConfig::read()
        .context("Failed to read Forge configuration from .forge.toml")?;
    let api = ForgeAPI::init(cwd, config);

    match group.command {
        SelectCommand::Model { query } => select_model(&api, query.as_deref()).await,
        SelectCommand::Agent { query } => select_agent(&api, query.as_deref()).await,
        SelectCommand::Provider { query, configured } => {
            select_provider(&api, query.as_deref(), configured).await
        }
        SelectCommand::ReasoningEffort { query } => {
            select_reasoning_effort(query.as_deref()).await
        }
        SelectCommand::Command { query } => select_command(query.as_deref()).await,
        SelectCommand::Conversation { query } => select_conversation(&api, query.as_deref()).await,
        SelectCommand::File { query } => select_file(query.as_deref()).await,
    }
}

/// A display row for model selection: `model_id  provider_id`.
#[derive(Clone)]
struct ModelRow {
    model_id: forge_domain::ModelId,
    provider_id: forge_domain::ProviderId,
}

impl Display for ModelRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\t{}", self.model_id, self.provider_id)
    }
}

async fn select_model(api: &impl API, query: Option<&str>) -> Result<()> {
    let all_provider_models = api.get_all_provider_models().await?;

    let mut rows: Vec<ModelRow> = Vec::new();
    for pm in &all_provider_models {
        for model in &pm.models {
            rows.push(ModelRow {
                model_id: model.id.clone(),
                provider_id: pm.provider_id.clone(),
            });
        }
    }

    if rows.is_empty() {
        return Ok(());
    }

    let mut builder = ForgeWidget::select("Model", rows);
    if let Some(q) = query {
        builder = builder.with_initial_text(q);
    }

    let selected = tokio::task::spawn_blocking(move || builder.prompt()).await??;

    if let Some(row) = selected {
        println!("{}", row.model_id);
        println!("{}", row.provider_id);
    }

    Ok(())
}

/// A display row for agent selection.
#[derive(Clone)]
struct AgentRow {
    id: String,
    name: String,
}

impl Display for AgentRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\t{}", self.id, self.name)
    }
}

async fn select_agent(api: &impl API, query: Option<&str>) -> Result<()> {
    let agents = api.get_agents().await?;

    let rows: Vec<AgentRow> = agents
        .into_iter()
        .map(|a| AgentRow {
            id: a.id.to_string(),
            name: a.title.unwrap_or_else(|| a.id.to_string()),
        })
        .collect();

    if rows.is_empty() {
        return Ok(());
    }

    let mut builder = ForgeWidget::select("Agent", rows);
    if let Some(q) = query {
        builder = builder.with_initial_text(q);
    }

    let selected = tokio::task::spawn_blocking(move || builder.prompt()).await??;

    if let Some(row) = selected {
        println!("{}", row.id);
    }

    Ok(())
}

/// A display row for provider selection.
#[derive(Clone)]
struct ProviderRow {
    id: String,
    display: String,
}

impl Display for ProviderRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display)
    }
}

async fn select_provider(
    api: &impl API,
    query: Option<&str>,
    configured_only: bool,
) -> Result<()> {
    let providers = api.get_providers().await?;

    let mut rows: Vec<ProviderRow> = providers
        .into_iter()
        .filter(|p| !configured_only || p.is_configured())
        .map(|p| {
            let id = p.id().to_string();
            let display = format!(
                "{}  {}",
                id,
                p.url()
                    .and_then(|u| u.domain().map(|d| d.to_string()))
                    .unwrap_or_default()
            );
            ProviderRow { id, display }
        })
        .collect();

    if rows.is_empty() {
        return Ok(());
    }

    let mut builder = ForgeWidget::select("Provider", rows);
    if let Some(q) = query {
        builder = builder.with_initial_text(q);
    }

    let selected = tokio::task::spawn_blocking(move || builder.prompt()).await??;

    if let Some(row) = selected {
        println!("{}", row.id);
    }

    Ok(())
}

async fn select_reasoning_effort(query: Option<&str>) -> Result<()> {
    let efforts = vec![
        "none", "minimal", "low", "medium", "high", "xhigh", "max",
    ];

    let mut builder = ForgeWidget::select("Reasoning Effort", efforts);
    if let Some(q) = query {
        builder = builder.with_initial_text(q);
    }

    let selected = tokio::task::spawn_blocking(move || builder.prompt()).await??;

    if let Some(effort) = selected {
        println!("{effort}");
    }

    Ok(())
}

async fn select_command(query: Option<&str>) -> Result<()> {
    let commands = vec![
        "info",
        "banner",
        "config",
        "hook",
        "provider",
        "conversation",
        "suggest",
        "cmd",
        "workspace",
        "commit",
        "data",
        "vscode",
        "update",
        "setup",
        "doctor",
        "clipboard",
        "logs",
        "select",
    ];

    let mut builder = ForgeWidget::select("Command", commands);
    if let Some(q) = query {
        builder = builder.with_initial_text(q);
    }

    let selected = tokio::task::spawn_blocking(move || builder.prompt()).await??;

    if let Some(cmd) = selected {
        println!("{cmd}");
    }

    Ok(())
}

/// A display row for conversation selection.
#[derive(Clone)]
struct ConversationRow {
    id: String,
    title: String,
}

impl Display for ConversationRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\t{}", self.id, self.title)
    }
}

async fn select_conversation(api: &impl API, query: Option<&str>) -> Result<()> {
    let conversations = api
        .get_conversations(None)
        .await
        .context("Failed to fetch conversations")?;

    let rows: Vec<ConversationRow> = conversations
        .into_iter()
        .map(|c| ConversationRow {
            id: c.id.into_string(),
            title: c.title.unwrap_or_default(),
        })
        .collect();

    if rows.is_empty() {
        return Ok(());
    }

    let mut builder = ForgeWidget::select("Conversation", rows);
    if let Some(q) = query {
        builder = builder.with_initial_text(q);
    }

    let selected = tokio::task::spawn_blocking(move || builder.prompt()).await??;

    if let Some(row) = selected {
        println!("{}", row.id);
    }

    Ok(())
}

async fn select_file(query: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let entries = walkdir_files(&cwd)?;
    if entries.is_empty() {
        return Ok(());
    }

    let mut builder = ForgeWidget::select("File", entries);
    if let Some(q) = query {
        builder = builder.with_initial_text(q);
    }

    let selected = tokio::task::spawn_blocking(move || builder.prompt()).await??;

    if let Some(file) = selected {
        println!("{file}");
    }

    Ok(())
}

/// Walk the current directory and collect relative file paths.
fn walkdir_files(root: &PathBuf) -> Result<Vec<String>> {
    let mut files = Vec::new();

    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            if let Some(name) = entry.file_name().to_str() {
                files.push(name.to_string());
            }
        }
    }

    files.sort();
    Ok(files)
}
