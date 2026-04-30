Executes shell commands. Defaults to `{{env.cwd}}` if no `cwd` specified.

- Do NOT use `cd` in command string. Use `cwd` parameter instead.
- For terminal operations only (git, npm, docker). Use dedicated tools for file operations.
- Quote file paths with spaces. Use `&&` for sequential, multiple calls for parallel.
- Do NOT use `find`/`grep`/`cat`/`head`/`tail`/`sed`/`awk`/`echo` — use dedicated tools instead.
- Output truncated at {{config.stdoutMaxPrefixLength}} prefix/{{config.stdoutMaxSuffixLength}} suffix lines. Full output in temp file.
- Returns stdout, stderr, and exit code.
