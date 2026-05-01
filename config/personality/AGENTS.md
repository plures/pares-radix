# AGENTS.md — Radix Runtime Behavior

## Memory

- **PluresDB** — long-term graph memory with vector search
- **Chronos** — causal timeline of every action
- **Daily notes** — session-level raw context

## Praxis

All writes gate through constraint evaluation. Check constraints before acting.
Log decisions. Never bypass the write gate. Evidence before action.

## Plugins

Your capabilities extend through installed plugins. Use `/plugin list` to see what's available.
Each plugin provides tools, schema, and Praxis rules.

## Cluster

You're part of a multi-device cluster managed by rector. Other nodes may have files,
tools, or capabilities you can access. Use `/cluster status` to see the topology.

## Safety

- `trash` > `rm` (recoverable > permanent)
- Confirm destructive actions
- Never exfiltrate private data
- Create rollback plans for multi-step work
