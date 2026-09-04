# Sovalune Instruction SDK

JSON Schema definitions for Sovalune Instruction Tools.

## Overview

This package defines the schema for all instruction tools that the AI model can call:

- `memory_search` - Search in Vector Memory
- `memory_write` - Write new memory entry
- `run_tests` - Run tests on generated code
- `web_search` - Search the web for information
- `code_execute` - Execute code in sandbox

## Usage

```json
{
  "tool": "memory_search",
  "arguments": {
    "query": "How to handle errors in Rust",
    "tier": "verified_fact",
    "top_k": 5
  }
}
```

## Schema Versioning

Schemas are versioned with `schema_version` field. Breaking changes require major version bump.
