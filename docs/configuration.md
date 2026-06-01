# Configuring Pares Radix

Pares Radix uses `.px` files for configuration. This gives you the full expressiveness of the Praxis Intent Language — not just key-value pairs, but constraints, conditional logic, and reactive behavior — all in one place.

## Quick Start

Copy the starter config and edit it:

```bash
cp config/radix.px ~/.pares-radix/radix.px
# Edit with your settings
pares-radix serve-spine --config ~/.pares-radix/radix.px
```

Or use environment variables and CLI flags — they always override config file values.

## Config File Location

Radix looks for configuration in this order:
1. `--config <path>` CLI flag (explicit)
2. `$PARES_CONFIG` environment variable
3. `./radix.px` (current directory)
4. `~/.pares-radix/radix.px` (user home)

## Config Blocks

A `.px` config file uses `config` blocks. Each block groups related settings:

```px
config radix:
  channel: "telegram"
  model: "claude-sonnet-4.5"
  use_copilot: true
```

### Available Config Blocks

| Block | Purpose |
|-------|---------|
| `radix` | Core runtime settings (channel, model, auth) |
| `telegram` | Telegram bot token and chat settings |
| `model` | Model provider URL, fallbacks, parameters |
| `personality` | System prompt, name, behavior |
| `heartbeat` | Proactive check-in behavior |
| `tools` | Which tools are available to the agent |
| `memory` | Memory storage path and embedding config |
| `logging` | Log level and Chronos timeline settings |

## Environment Variable References

Use `"env:VAR_NAME"` to reference environment variables without hardcoding secrets:

```px
config telegram:
  token: "env:PARES_TELEGRAM_TOKEN"
```

This is the recommended pattern for tokens and API keys.

## Overriding with CLI Flags

Every config value can be overridden via CLI flag or environment variable:

| Config Path | CLI Flag | Env Var |
|-------------|----------|---------|
| `radix.channel` | `--channel` | `PARES_CHANNEL` |
| `radix.model` | `--model` | `PARES_MODEL` |
| `radix.use_copilot` | `--use-copilot` | `PARES_USE_COPILOT` |
| `model.url` | `--model-url` | `PARES_MODEL_URL` |
| `telegram.token` | `--telegram-token` | `PARES_TELEGRAM_TOKEN` |

Priority: CLI flag > env var > config file > default.

## Advanced: Constraints in Config

Because `.px` is a full language, you can add constraints that validate your configuration:

```px
constraint valid_model:
  when: config.radix.model != ""
  require: config.model.url != "" or config.radix.use_copilot == true
  severity: error
  message: "Either model.url or use_copilot must be set"
```

## Advanced: Conditional Configuration

Use the full `.px` grammar for environment-aware configs:

```px
config radix:
  channel: "telegram"
  model: "claude-sonnet-4.5"
  use_copilot: true

# Development overrides
config dev:
  channel: "stdio"
  model: "gpt-4o"
  use_copilot: false
```

Select which config to use via `--profile dev` or `PARES_PROFILE=dev`.

## Examples

### Minimal (stdio, local model)

```px
config radix:
  channel: "stdio"
  model: "llama3"

config model:
  url: "http://localhost:11434/v1"
```

### Production (Telegram, Copilot auth, full tools)

```px
config radix:
  channel: "telegram"
  model: "claude-sonnet-4.5"
  use_copilot: true

config telegram:
  token: "env:PARES_TELEGRAM_TOKEN"

config heartbeat:
  enabled: true
  interval_minutes: 90

config personality:
  name: "Radix"
  system_prompt: "You are a precise, proactive engineering assistant."
```

### Research mode (high temp, no heartbeat)

```px
config radix:
  channel: "stdio"
  model: "gpt-4o"
  use_copilot: true

config model:
  temperature: 1.0
  max_tokens: 8192

config heartbeat:
  enabled: false
```

## Next Steps

- [.px Language Guide](./px-language-guide.md) — learn the full Praxis Intent Language
- [Personality Configuration](./personality.md) — customize your agent's behavior
- [Tool Configuration](./tools.md) — enable/disable specific capabilities
