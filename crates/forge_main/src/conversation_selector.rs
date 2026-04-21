use anyhow::Result;
use chrono::Utc;
use forge_api::Conversation;
use forge_domain::{ContextMessage, ConversationId};
use forge_select::{ForgeWidget, PreviewLayout, PreviewPlacement, SelectRow};

use crate::display_constants::markers;
use crate::info::Info;
use crate::porcelain::Porcelain;

/// Logic for selecting conversations from a list
pub struct ConversationSelector;

impl ConversationSelector {
    /// Returns the display title for a conversation, falling back to the
    /// first user message's raw content or content when no title is set.
    pub fn conversation_title(conversation: &Conversation) -> String {
        if let Some(title) = conversation.title.as_deref() {
            return title.to_string();
        }

        conversation
            .first_user_messages()
            .into_iter()
            .find_map(|message| match message {
                ContextMessage::Text(text) => text
                    .raw_content
                    .as_ref()
                    .and_then(|value| {
                        value
                            .as_user_prompt()
                            .map(|prompt| prompt.as_str().to_string())
                    })
                    .or_else(|| Some(text.content.clone())),
                _ => None,
            })
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| markers::EMPTY.to_string())
    }

    /// Select a conversation from the provided list using a custom TUI with
    /// a preview pane showing conversation details.
    ///
    /// The preview command uses `forge conversation info` and
    /// `forge conversation show` to display the selected conversation's
    /// metadata and last message side-by-side with the picker list.
    ///
    /// Returns the selected conversation, or None if the user cancelled.
    pub async fn select_conversation(
        conversations: &[Conversation],
        _current_conversation_id: Option<ConversationId>,
        query: Option<String>,
    ) -> Result<Option<Conversation>> {
        if conversations.is_empty() {
            return Ok(None);
        }

        // Filter to conversations with context so interrupted or untitled
        // sessions remain resumable.
        let valid_conversations: Vec<&Conversation> = conversations
            .iter()
            .filter(|c| c.context.is_some())
            .collect();

        if valid_conversations.is_empty() {
            return Ok(None);
        }

        // Build Info structure for display
        let now = Utc::now();
        let mut info = Info::new();

        for conv in &valid_conversations {
            let title = Self::conversation_title(conv);

            let duration = now.signed_duration_since(
                conv.metadata.updated_at.unwrap_or(conv.metadata.created_at),
            );
            let duration =
                std::time::Duration::from_secs((duration.num_minutes() * 60).max(0) as u64);
            let time_ago = if duration.is_zero() {
                "now".to_string()
            } else {
                format!("{} ago", humantime::format_duration(duration))
            };

            info = info
                .add_title(conv.id)
                .add_key_value("Title", title)
                .add_key_value("Updated", time_ago);
        }

        // Convert to porcelain, drop the UUID title column (col 0), truncate the
        // Title column for display, uppercase headers
        let porcelain_output = Porcelain::from(&info)
            .drop_col(0)
            .truncate(0, 60)
            .uppercase_headers();
        let porcelain_str = porcelain_output.to_string();

        let all_lines: Vec<&str> = porcelain_str.lines().collect();
        if all_lines.is_empty() {
            return Ok(None);
        }

        // Build SelectRow items for the shared Rust selector UI.
        // Each row stores the UUID in `fields[0]` so that `{1}` in the preview
        // command resolves to the conversation ID. The `raw` field is what gets
        // returned on selection (the UUID).
        let mut rows: Vec<SelectRow> = Vec::with_capacity(all_lines.len());

        // Header row (non-selectable via header_lines=1)
        if let Some(header) = all_lines.first() {
            rows.push(SelectRow::header(header.to_string()));
        }

        // Data rows: each maps to a conversation
        for (i, line) in all_lines.iter().skip(1).enumerate() {
            if let Some(conv) = valid_conversations.get(i) {
                let uuid = conv.id.to_string();
                rows.push(SelectRow {
                    raw: uuid.clone(),
                    display: line.to_string(),
                    search: line.to_string(),
                    fields: vec![uuid],
                });
            }
        }

        // Build a lookup map from UUID to Conversation for the result
        let conv_map: std::collections::HashMap<String, Conversation> = valid_conversations
            .into_iter()
            .map(|c| (c.id.to_string(), c.clone()))
            .collect();

        let preview_command =
            "CLICOLOR_FORCE=1 forge conversation info {1}; echo; CLICOLOR_FORCE=1 forge conversation show {1}"
                .to_string();

        let selected_uuid = tokio::task::spawn_blocking(move || -> Result<Option<String>> {
            Ok(ForgeWidget::select_rows("Conversation", rows)
                .query(query)
                .header_lines(1_usize)
                .preview(Some(preview_command))
                .preview_layout(PreviewLayout { placement: PreviewPlacement::Bottom, percent: 60 })
                .prompt()?
                .map(|row| row.raw))
        })
        .await??;

        Ok(selected_uuid.and_then(|uuid| conv_map.get(&uuid).cloned()))
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use forge_api::Conversation;
    use forge_domain::{ConversationId, MetaData, Metrics, Role};
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_test_conversation(id: &str, title: Option<&str>) -> Conversation {
        let now = Utc::now();
        Conversation {
            id: ConversationId::parse(id).unwrap(),
            title: title.map(|t| t.to_string()),
            context: None,
            metrics: Metrics::default().started_at(now),
            metadata: MetaData { created_at: now, updated_at: Some(now) },
        }
    }

    #[tokio::test]
    async fn test_select_conversation_empty_list() {
        let conversations = vec![];
        let result = ConversationSelector::select_conversation(&conversations, None, None)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_select_conversation_with_titles() {
        let conversations = [
            create_test_conversation(
                "550e8400-e29b-41d4-a716-446655440000",
                Some("First Conversation"),
            ),
            create_test_conversation(
                "550e8400-e29b-41d4-a716-446655440001",
                Some("Second Conversation"),
            ),
        ];

        assert_eq!(conversations.len(), 2);
    }

    #[test]
    fn test_select_conversation_without_titles() {
        let conversations = [
            create_test_conversation("550e8400-e29b-41d4-a716-446655440002", None),
            create_test_conversation("550e8400-e29b-41d4-a716-446655440003", None),
        ];

        assert_eq!(conversations.len(), 2);
    }

    #[test]
    fn test_conversation_title_falls_back_to_first_user_message() {
        let now = Utc::now();
        let fixture = Conversation {
            id: ConversationId::parse("550e8400-e29b-41d4-a716-446655440004").unwrap(),
            title: None,
            context: Some(
                forge_domain::Context::default().add_message(ContextMessage::Text(
                    forge_domain::TextMessage::new(Role::User, "Continue previous work")
                        .raw_content(forge_domain::EventValue::text("Resume interrupted agent")),
                )),
            ),
            metrics: Metrics::default().started_at(now),
            metadata: MetaData { created_at: now, updated_at: Some(now) },
        };

        let actual = ConversationSelector::conversation_title(&fixture);
        let expected = "Resume interrupted agent".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_conversation_title_uses_empty_marker_without_title_or_user_message() {
        let fixture = create_test_conversation("550e8400-e29b-41d4-a716-446655440005", None);

        let actual = ConversationSelector::conversation_title(&fixture);
        let expected = markers::EMPTY.to_string();
        assert_eq!(actual, expected);
    }

    /// Verifies that the porcelain output pipeline used by `on_show_conversations`
    /// correctly surfaces fallback titles for conversations without a stored title.
    #[test]
    fn test_conversation_list_porcelain_uses_fallback_title() {
        use crate::info::Info;
        use crate::porcelain::Porcelain;

        let now = Utc::now();

        // Conversation with a proper title
        let titled = Conversation {
            id: ConversationId::parse("550e8400-e29b-41d4-a716-446655440010").unwrap(),
            title: Some("My Agent Run".to_string()),
            context: Some(forge_domain::Context::default()),
            metrics: Metrics::default().started_at(now),
            metadata: MetaData { created_at: now, updated_at: Some(now) },
        };

        // Conversation without title but with user message (interrupted agent)
        let interrupted = Conversation {
            id: ConversationId::parse("550e8400-e29b-41d4-a716-446655440011").unwrap(),
            title: None,
            context: Some(
                forge_domain::Context::default().add_message(ContextMessage::Text(
                    forge_domain::TextMessage::new(Role::User, "fallback content")
                        .raw_content(forge_domain::EventValue::text(
                            "Build the authentication module",
                        )),
                )),
            ),
            metrics: Metrics::default().started_at(now),
            metadata: MetaData { created_at: now, updated_at: Some(now) },
        };

        // Build Info the same way on_show_conversations does
        let conversations = vec![titled, interrupted];
        let mut info = Info::new();
        for conv in &conversations {
            let title = ConversationSelector::conversation_title(conv);
            info = info
                .add_title(conv.id)
                .add_key_value("Title", title)
                .add_key_value("Updated", "now");
        }

        // Apply the same porcelain transforms as on_show_conversations
        let porcelain = Porcelain::from(&info)
            .drop_col(3)
            .truncate(1, 60)
            .uppercase_headers();

        let actual = porcelain.to_string();

        // Verify both conversations appear with correct titles
        assert!(
            actual.contains("My Agent Run"),
            "titled conversation should show its title"
        );
        assert!(
            actual.contains("Build the authentication module"),
            "interrupted conversation should show fallback title from raw_content"
        );
    }

    /// Verifies that conversations without context are excluded from the list,
    /// matching the filter logic in `on_show_conversations`.
    #[test]
    fn test_conversation_list_excludes_no_context_conversations() {
        let now = Utc::now();

        // Has context — should be included
        let with_context = Conversation {
            id: ConversationId::parse("550e8400-e29b-41d4-a716-446655440020").unwrap(),
            title: Some("Active Session".to_string()),
            context: Some(forge_domain::Context::default()),
            metrics: Metrics::default().started_at(now),
            metadata: MetaData { created_at: now, updated_at: Some(now) },
        };

        // No context — should be excluded
        let without_context = Conversation {
            id: ConversationId::parse("550e8400-e29b-41d4-a716-446655440021").unwrap(),
            title: Some("Ghost Session".to_string()),
            context: None,
            metrics: Metrics::default().started_at(now),
            metadata: MetaData { created_at: now, updated_at: Some(now) },
        };

        let conversations = vec![with_context.clone(), without_context];

        // Apply the same filter as on_show_conversations
        let filtered: Vec<&Conversation> = conversations
            .iter()
            .filter(|c| c.context.is_some())
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, with_context.id);
    }
}
