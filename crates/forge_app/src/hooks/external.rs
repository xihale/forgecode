use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use async_trait::async_trait;
use forge_domain::{
    Conversation, EventData, EventHandle, ToolcallStartPayload,
};
use serde::{Deserialize, Serialize};

/// Hook handler that executes external scripts for lifecycle events.
///
/// It looks for scripts in `~/.forge/hooks/` and executes them if they exist.
/// Currently only supports `ToolcallStart` event for command rewriting.
#[derive(Clone, Default)]
pub struct ExternalHookHandler;

#[derive(Serialize, Deserialize)]
struct ExternalHookInput {
    tool_name: String,
    tool_input: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct ExternalHookOutput {
    decision: String,
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: Option<HookSpecificOutput>,
}

#[derive(Serialize, Deserialize)]
struct HookSpecificOutput {
    tool_input: serde_json::Value,
}

impl ExternalHookHandler {
    pub fn new() -> Self {
        Self
    }

    fn get_hook_path(&self, event_name: &str) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let hook_path = home.join(".forge").join("hooks").join(format!("rtk-{}.sh", event_name));
        if hook_path.exists() {
            Some(hook_path)
        } else {
            None
        }
    }
}

#[async_trait]
impl EventHandle<EventData<ToolcallStartPayload>> for ExternalHookHandler {
    async fn handle(
        &self,
        event: &mut EventData<ToolcallStartPayload>,
        _conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let Some(hook_path) = self.get_hook_path("toolcall-start") else {
            return Ok(());
        };

        let tool_call = &event.payload.tool_call;
        let input = ExternalHookInput {
            tool_name: tool_call.name.as_str().to_string(),
            tool_input: serde_json::to_value(&tool_call.arguments)?,
        };

        let mut child = Command::new(hook_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let mut stdin = child.stdin.take().unwrap();
        let input_json = serde_json::to_string(&input)?;
        stdin.write_all(input_json.as_bytes()).await?;
        drop(stdin);

        let output = child.wait_with_output().await?;
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(external_output) = serde_json::from_str::<ExternalHookOutput>(&output_str) {
                if external_output.decision == "allow" {
                    if let Some(specific) = external_output.hook_specific_output {
                        let updated_args = serde_json::from_value(specific.tool_input)?;
                        event.payload.tool_call.arguments = updated_args;
                    }
                }
            }
        }

        Ok(())
    }
}
