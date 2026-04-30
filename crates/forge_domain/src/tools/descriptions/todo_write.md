Manage a structured task list for the current coding session.

Each call sends only changed items. Each item requires `content` (unique key) and `status` (`pending`|`in_progress`|`completed`|`cancelled`).

**Rules:**
- New content → added. Existing content → status updated. `cancelled` → removed. Unmentioned → unchanged.
- Only ONE task `in_progress` at a time.
- Mark `in_progress` BEFORE starting. Mark `completed` IMMEDIATELY after finishing.
- Do NOT mark `completed` if tests fail, implementation partial, or errors unresolved.

**When to use:** Complex multi-step tasks (3+ steps), user provides multiple tasks, explicit request for todo list.

**When NOT to use:** Single trivial task, conversational/informational requests.
