Executes shell commands. Defaults to `{{env.cwd}}` if no `cwd` specified.

CRITICAL: This tool is for terminal operations only (git, npm, docker, cargo, make, etc.). DO NOT use it for file operations — use the dedicated tools instead. The dedicated tools have been optimized for correct permissions, access, and performance.

- Do NOT use `cd` in command string. Use `cwd` parameter instead.
- Quote file paths with spaces. Use `&&` for sequential, multiple calls for parallel.

NEVER use these commands — always use the dedicated tool instead:

- `find` → Use `{{tool_names.fs_search}}` with `glob` or `output_mode: "files_with_matches"` to find files. The search tool respects .gitignore and has optimized file access.
- `grep` / `rg` → Use `{{tool_names.fs_search}}` with `pattern` parameter to search file contents. Supports full regex, context lines, file type filtering.
- `cat` / `head` / `tail` → Use `{{tool_names.read}}` to read file contents. Supports line ranges via `start_line`/`end_line`.
- `sed` / `awk` → Use `{{tool_names.patch}}` or `{{tool_names.multi_patch}}` to edit files. Supports string replacement and multi-edit operations.
- `echo` redirection (`>` / `>>`) → Use `{{tool_names.write}}` to create or overwrite files.

Output truncated at {{config.stdoutMaxPrefixLength}} prefix/{{config.stdoutMaxSuffixLength}} suffix lines. Full output in temp file. Returns stdout, stderr, and exit code.
