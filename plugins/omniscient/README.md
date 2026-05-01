# Omniscient — Semantic Filesystem Intelligence

Two-pass semantic filesystem indexer with cluster-aware cross-system search.

## What it does

- **Indexes your filesystem** — text, code, PDFs, images, office docs, binaries
- **Understands content** — not just filenames, but what's *in* the files
- **Cross-system search** — finds files across all your devices (praxisbot, surface, devbox)
- **Security intelligence** — analyzes binaries for capabilities, entropy anomalies, risk scoring
- **Improves over time** — Pass 2 enriches results with LLM summaries and entity extraction

## Architecture

```
Pass 1 (instant): inotify → extract → embed → PluresDB
Pass 2 (async):   BitNet → summarize → entities → classify → re-embed
```

## Commands

| Command | Description |
|---------|-------------|
| `/find <query>` | Semantic search across all indexed files |
| `/index <path>` | Index a directory |
| `/index status` | Show index stats (files, enrichment progress) |
| `/security-scan [path]` | Analyze binaries in path for risk |

## Capabilities Required

- `pluresdb` — stores file nodes and vectors
- `bitnet` — Pass 2 local LLM enrichment
- `rector` — cluster node identity for cross-system awareness

## Install

```
/plugin install omniscient
```
