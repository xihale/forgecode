Launch a specialized agent to handle complex tasks autonomously.

Available agents:
{{#each agents}}
- **{{id}}**{{#if description}}: {{description}}{{/if}}{{#if tools}}
  - Tools: {{#each tools}}{{this}}{{#unless @last}}, {{/unless}}{{/each}}{{/if}}
{{/each}}

- Specify `agent_id` to select agent type. Provide detailed task descriptions.
- Launch multiple agents concurrently with multiple tool calls.
- Agents can be resumed via `session_id` for follow-up work.
- Do NOT use for simple lookups — use `{{tool_names.read}}` or `{{tool_names.fs_search}}` directly.
