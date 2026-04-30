Multiple edits to a single file in one operation. Prefer over `{{tool_names.patch}}` when making several changes.

- Edits applied sequentially. Atomic — all succeed or none applied.
- `old_string` must match file exactly. `replace_all` for renaming across file.
- Must read file first. Use absolute paths.
- For new files: first edit with empty `old_string`.
