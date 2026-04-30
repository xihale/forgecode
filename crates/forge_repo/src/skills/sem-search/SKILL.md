---
name: sem-search
description: Use when calling sem_search tool. Provides query crafting tips, use_case guidelines, and example patterns for effective semantic code search. Auto-triggers on first sem_search call.
---

# Semantic Search Usage Guide

## When to Use sem_search

- Finding implementations of features or algorithms
- Understanding how a system works across multiple files
- Discovering architectural patterns and design approaches
- Locating test examples or fixtures
- Finding where specific technologies/libraries are used
- Exploring unfamiliar codebases
- Finding documentation files (README, guides, API docs)

## When NOT to Use (use fs_search instead)

- Searching for exact strings, TODOs, or specific function names
- Finding all occurrences of a variable or identifier
- Searching in specific file paths or with regex patterns
- When you know the exact text to search for

## Query Field (WHAT the code does)

This is converted to a vector embedding. Use specific technical terms.

**Good:**
- "exponential backoff retry mechanism with configurable delays"
- "streaming LLM responses with SSE chunked transfer encoding"
- "OAuth2 token refresh with automatic retry and expiry check"
- "Diesel database migration runner with transaction support"

**Bad:**
- "retry" (too generic)
- "authentication" (overly broad - specify what aspect)
- "how system works" (meta-question, not searchable)

## Use Case Field (WHY you need this code, INTENT)

This reranks results by relevance. **MANDATORY**: include construct keywords (struct, trait, impl, function, fn, class, definition) when searching for code.

**Good (includes construct keywords):**
- "I need the struct definition and trait implementation for Diesel migrations"
- "Show me the function implementation for semantic search reranker"
- "Find the type declarations and interface definitions for the tool registry"
- "I need documentation explaining how to configure semantic search, not the implementation code"

**Bad (missing construct keywords = get docs instead of code):**
- "I need code that handles authentication"
- "Show me the database logic"
- "Find the reranker code"

## Tips

- Use 2-3 varied queries per search
- Balance specificity with generality
- Match intent: seeking docs → use doc keywords; seeking code → use implementation keywords
- Keep queries targeted - too many broad queries cause timeouts
