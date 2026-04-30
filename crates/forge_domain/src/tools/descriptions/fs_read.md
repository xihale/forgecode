Reads a file from the local filesystem.

- `file_path` must be absolute. Reads up to {{config.maxReadSize}} lines by default.
- Optional `range` for partial reads. Lines > {{config.maxLineLength}} chars truncated.
- Line numbers start at 1. Read multiple files in parallel for efficiency.
{{#if (contains model.input_modalities "image")}}
- Supports images (PNG, JPG) and PDFs (base64 encoded, max {{config.maxImageSize}} bytes).
{{/if}}
- .ipynb files read as JSON. Cannot read directories — use `{{tool_names.shell}}` with `ls`.
