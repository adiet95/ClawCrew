# ClawCrew CLI Reference

Curated operational reference for common `clawcrew` commands. It is not an exhaustive command tree; use `clawcrew --help` and `clawcrew <command> --help` for the commands and flags in the installed build.

## Table of Contents

1. [Agent](#agent)
2. [Onboarding](#onboarding)
3. [Status & Diagnostics](#status--diagnostics)
4. [Memory](#memory)
5. [Cron](#cron)
6. [Providers & Models](#providers--models)
7. [Gateway & Daemon](#gateway--daemon)
8. [Service Management](#service-management)
9. [Channels](#channels)
10. [Security & Emergency Stop](#security--emergency-stop)
11. [Hardware Peripherals](#hardware-peripherals)
12. [Skills](#skills)
13. [Shell Completions](#shell-completions)

---

## Agent

Interactive chat or single-message mode.

```bash
clawcrew agent -a assistant                                          # Interactive REPL
clawcrew agent -a assistant -m "Summarize today's logs"              # Single message
clawcrew agent -a assistant -p anthropic --model claude-sonnet-4-6   # Override provider/model
clawcrew agent -a assistant -t 0.3                                   # Set temperature
clawcrew agent -a assistant --peripheral nucleo-f401re:/dev/ttyACM0  # Attach hardware
```

`-a <alias>` is required and must match a configured `[agents.<alias>]` entry — there is no default agent.

**Key flags:**
- `-m <message>` — single message mode (no REPL)
- `-p <provider>` — override provider (openrouter, anthropic, openai, ollama)
- `--model <model>` — override model
- `-t <float>` — temperature (0.0–2.0)
- `--peripheral <name>:<port>` — attach hardware peripheral

The agent has access to 30+ tools gated by security policy: shell, file_read, file_write, file_edit, glob_search, content_search, memory_store, memory_recall, memory_forget, browser, http_request, web_fetch, web_search, cron, delegate, git, and more. Max tool iterations defaults to 10.

---

## Quickstart

First-time setup. Picks a model provider, writes a working `~/.clawcrew/config.toml`, and binds one agent to that provider in a single shot.

```bash
clawcrew quickstart                                                           # Interactive prompts
clawcrew quickstart --model-provider ollama --model qwen2.5:7b                # Non-interactive (local)
clawcrew quickstart --model-provider openrouter --model openrouter/auto \
                    --api-key sk-or-... --agent or                            # Non-interactive (hosted)
```

**Flags:**
- `--model-provider <name>` — anthropic, openai, openrouter, ollama, gemini, glm, telnyx, …
- `--model <id>` — model id to write for the new provider entry
- `--api-key <key>` — API key (omit for local providers like ollama)
- `--agent <alias>` — agent alias (defaults to a sanitized provider name)

Creates `~/.clawcrew/config.toml` with `0600` permissions. Quickstart is idempotent: re-running it on a configured install leaves the existing config alone. To change one field afterward, use `clawcrew config set <path> <value>`; to reconfigure channels, use `clawcrew config set channels.<type>.<alias>.<field> <value>`. For secret fields, omit the value to use the masked input prompt, for example `clawcrew config set channels.telegram.default.bot-token`. Per-channel guides live under [Channels → Overview](../../../../docs/book/src/channels/overview.md).

---

## Status & Diagnostics

```bash
clawcrew status                    # System overview
clawcrew doctor                    # Run all diagnostic checks
clawcrew doctor models             # Probe model connectivity
clawcrew doctor traces             # Query execution traces
```

---

## Memory

```bash
clawcrew memory list                              # List all entries
clawcrew memory list --category core --limit 10   # Filtered list
clawcrew memory get "some-key"                    # Get specific entry
clawcrew memory stats                             # Usage statistics
clawcrew memory clear --key "prefix" --yes        # Delete entries (requires --yes)
```

**Key flags:**
- `--category <name>` — filter by category (core, daily, conversation, custom)
- `--limit <n>` — limit results
- `--key <prefix>` — key prefix for clear operations
- `--yes` — skip confirmation (required for clear)

---

## Cron

```bash
clawcrew cron list                                                      # List all jobs
clawcrew cron add '0 9 * * 1-5' 'Good morning' --tz America/New_York   # Recurring (cron expr)
clawcrew cron add-at '2026-03-11T10:00:00Z' 'Remind me about meeting'  # One-time at specific time
clawcrew cron add-every 3600000 'Check server health'                   # Interval in milliseconds
clawcrew cron once 30m 'Follow up on that task'                         # Delay from now
clawcrew cron pause <id>                                                # Pause job
clawcrew cron resume <id>                                               # Resume job
clawcrew cron remove <id>                                               # Delete job
```

**Subcommands:**
- `add <cron-expr> <command>` — standard cron expression (5-field)
- `add-at <iso-datetime> <command>` — fire once at exact time
- `add-every <ms> <command>` — repeating interval
- `once <duration> <command>` — delay from now (e.g., `30m`, `2h`, `1d`)

---

## Providers & Models

```bash
clawcrew providers                                # List all 40+ supported providers
clawcrew models list                              # Show cached model catalog
clawcrew models refresh --all                     # Refresh catalogs from all providers
clawcrew models set anthropic/claude-sonnet-4-6   # Set default model
clawcrew models status                            # Current model info
```

Model routing in config.toml:

```toml
[[model_routes]]
hint = "reasoning"
model_provider = "openrouter"
model = "anthropic/claude-sonnet-4-6"
```

---

## Gateway & Daemon

```bash
clawcrew gateway                                 # Start HTTP gateway (foreground)
clawcrew gateway -p 8080 --host 127.0.0.1        # Custom port/host

clawcrew daemon                                  # Gateway + channels + scheduler + heartbeat
clawcrew daemon -p 8080 --host 0.0.0.0           # Custom bind
```

**Gateway defaults:**
- Port: 42617
- Host: 127.0.0.1
- Pairing required: true
- Public bind allowed: false

---

## Service Management

OS service lifecycle (systemd on Linux, launchd on macOS).

```bash
clawcrew service install     # Install as system service
clawcrew service start       # Start the service
clawcrew service status      # Check service status
clawcrew service stop        # Stop the service
clawcrew service restart     # Restart the service
clawcrew service uninstall   # Remove the service
```

**Logs:**
- macOS: `~/.clawcrew/logs/daemon.stdout.log`
- Linux: `journalctl -u clawcrew`

---

## Channels

Channels use alias-keyed entries under `[channels.<type>.<alias>]`. Availability depends on the installed build's feature set; consult the current channel guide and generated config reference before configuring a channel.

```bash
clawcrew channels list       # List configured channels
clawcrew channel doctor      # Check channel health
```

---

## Security & Emergency Stop

```bash
clawcrew estop --level kill-all                              # Stop everything
clawcrew estop --level network-kill                          # Block all network access
clawcrew estop --level domain-block --domain "*.example.com" # Block specific domains
clawcrew estop --level tool-freeze --tool shell              # Freeze specific tool
clawcrew estop status                                        # Check estop state
clawcrew estop resume --network                              # Resume (may require OTP)
```

**Estop levels:**
- `kill-all` — nuclear option, stops all agent activity
- `network-kill` — blocks all outbound network
- `domain-block` — blocks specific domain patterns
- `tool-freeze` — freezes individual tools

Autonomy lives on a risk profile and a runtime profile, both alias-keyed; the agent points at them via `[agents.<alias>] risk_profile = "..."` and `runtime_profile = "..."`:

```toml
[risk_profiles.assistant]
level = "supervised"                           # readonly | supervised | full
workspace_only = true
allowed_commands = ["git", "cargo", "python"]
forbidden_paths = ["/etc", "/root", "~/.ssh"]

[runtime_profiles.assistant]
max_actions_per_hour = 20
max_cost_per_day_cents = 500
```

---

## Hardware Peripherals

```bash
clawcrew hardware discover                              # Find USB devices
clawcrew hardware introspect /dev/ttyACM0               # Probe device capabilities
clawcrew peripheral list                                # List configured peripherals
clawcrew peripheral add nucleo-f401re /dev/ttyACM0      # Add peripheral
clawcrew peripheral flash-nucleo                        # Flash STM32 firmware
clawcrew peripheral flash --port /dev/cu.usbmodem101    # Flash Arduino firmware
```

**Supported boards:** STM32 Nucleo-F401RE, Arduino Uno R4, Raspberry Pi GPIO, ESP32.

Attach to agent session: `clawcrew agent -a assistant --peripheral nucleo-f401re:/dev/ttyACM0`

---

## Skills

```bash
clawcrew skills list         # List installed skills
clawcrew skills install <path-or-url>  # Install a skill
clawcrew skills audit        # Audit installed skills
clawcrew skills remove <name>  # Remove a skill
```

---

## Shell Completions

```bash
clawcrew completions zsh     # Generate Zsh completions
clawcrew completions bash    # Generate Bash completions
clawcrew completions fish    # Generate Fish completions
```

---

## Config File

Default location: `~/.clawcrew/config.toml`

Config resolution order (first match wins):
1. `CLAWCREW_CONFIG_DIR` environment variable
2. `CLAWCREW_WORKSPACE` environment variable
3. `~/.clawcrew/active_workspace.toml` marker file
4. `~/.clawcrew/config.toml` (default)
