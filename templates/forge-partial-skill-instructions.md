## Skill Instructions:

Check `<available_skills>` before tasks. Invoke via `skill` tool with `{"name": "<skill_name>"}`. Returns full instructions. Only invoke listed skills. Don't re-invoke active skills.

<available_skills>
{{#each skills}}
<skill>
<name>{{this.name}}</name>
<description>
{{this.description}}
</description>
</skill>
{{/each}}
</available_skills>