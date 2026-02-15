# Photon Usage Guide

## First-Time Setup

```bash
# 1. Build Photon
cargo build --release

# 2. Download the AI models (~350MB, one-time)
cargo run -- models download
```

After building, the binary is at `./target/release/photon`. All examples below use `cargo run --` which builds and runs in one step — replace with `photon` if you've installed it.

## Processing Images

### Single image

```bash
cargo run -- process photo.jpg
```

Outputs JSON to stdout with metadata, content hash, embedding, and tags.

### Directory of images

```bash
cargo run -- process ./photos/
```

### Save output to a file

```bash
cargo run -- process ./photos/ -o results.jsonl -f jsonl
```

### Pipe to another tool

```bash
cargo run -- process ./photos/ -f jsonl | jq '.tags'
```

## Common Flags

| Flag | What it does |
|------|-------------|
| `-o, --output <path>` | Write output to file instead of stdout |
| `-f, --format <json\|jsonl>` | Output format (default: `json`) |
| `-p, --parallel <n>` | Parallel workers (default: `4`) |
| `--quality <fast\|high>` | `fast` = 224px model, `high` = 384px model |
| `--skip-existing` | Skip images already in the output file |
| `-v, --verbose` | Debug logging |

## Disabling Pipeline Stages

Skip stages you don't need for faster processing:

```bash
cargo run -- process photo.jpg --no-thumbnail
cargo run -- process photo.jpg --no-embedding --no-tagging
cargo run -- process photo.jpg --no-description
```

## Tagging Options

```bash
# Show full hierarchy paths (e.g. "animal > dog > labrador retriever")
cargo run -- process photo.jpg --show-tag-paths

# Keep ancestor tags instead of deduplicating them
cargo run -- process photo.jpg --no-dedup-tags
```

## LLM Descriptions (BYOK)

Add natural-language descriptions via an LLM provider:

```bash
# Local (Ollama)
cargo run -- process photo.jpg --llm ollama --llm-model llama3.2-vision

# Anthropic
cargo run -- process photo.jpg --llm anthropic --llm-model claude-sonnet-4-20250514

# OpenAI
cargo run -- process photo.jpg --llm openai --llm-model gpt-4o-mini

# Hyperbolic
cargo run -- process photo.jpg --llm hyperbolic --llm-model meta-llama/Llama-3.2-11B-Vision-Instruct
```

API keys are read from config or environment variables (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `HYPERBOLIC_API_KEY`).

## Batch Processing

```bash
# Process a large folder with 8 workers, skip already-done images
cargo run -- process ./photos/ -o results.jsonl -f jsonl -p 8 --skip-existing
```

## Model Management

```bash
cargo run -- models download   # Download SigLIP models
cargo run -- models list       # Show installed models
cargo run -- models path       # Show model directory
```

## Configuration

```bash
cargo run -- config show       # Display current config
cargo run -- config path       # Show config file location
cargo run -- config init       # Create default config file
```

Config file location is platform-specific (e.g. `~/.config/photon/config.toml` on Linux). Settings can be overridden by CLI flags.

## Interactive Mode

Run `cargo run` with no arguments on a terminal to get a guided menu for processing, model management, and configuration.
