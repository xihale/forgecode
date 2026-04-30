Retrieves content from URLs as markdown or raw text. Handles HTTP/HTTPS, converts HTML to markdown. For large pages, returns first 40K chars and stores full content in temp file.

Text-based content only. For binary downloads, use `shell` with `curl -fLo <output_file> <url>`.
