# MRouter - LLM Gateway Router

> **One Endpoint, Every Model. Route, Monitor, and Failover — All From Your Terminal.**

[English](README.md) | [中文](README_zh.md)

One endpoint for every AI provider — smart routing, auto failover, zero-config CLI sync. Built as a terminal-native LLM gateway.

## Why MRouter?

- **Server Terminal Configuration** - Full TUI management via SSH on remote servers. No web UI needed — configure providers, monitor stats, and switch routing directly from any terminal session
- **Single Configuration Point** - Configure all providers once. Applications only need `http://localhost:4444`
- **Model-Based Auto-Routing** - Specify a model name (e.g., `gpt-4`, `claude-opus-4`, `gemini-pro`) and MRouter routes to the correct provider automatically
- **Centralized API Key Management** - All API keys stored securely in one place, no per-application configuration needed
- **Automatic Failover & Circuit Breaker** - If one provider fails, automatically tries alternatives with built-in fault tolerance
- **Custom Headers Per Provider** - Override headers like `User-Agent` per provider for version control and compatibility
- **CLI Tools Auto-Sync** - One-click sync configuration to Claude Code, Codex, Gemini CLI, OpenCode, OpenClaw

## Key Use Cases

**Multi-Provider Routing for Server Applications**

When deploying applications on servers that need to access multiple AI providers, MRouter acts as a unified gateway. Your applications only need to point to `http://localhost:4444` — no need to configure API keys and endpoints for each provider in every application.

```bash
# All requests go to localhost:4444 — MRouter routes by model name
curl http://localhost:4444/v1/messages -d '{"model": "gpt-4", ...}'          # → OpenAI
curl http://localhost:4444/v1/messages -d '{"model": "claude-opus-4", ...}'  # → Anthropic
curl http://localhost:4444/v1/messages -d '{"model": "gemini-pro", ...}'     # → Google
```

**Remote Server Management via SSH**

MRouter's TUI works over SSH, making it ideal for managing AI routing on remote development servers or cloud instances:

```bash
ssh my-server
mrouter              # Full TUI management in your terminal
```

No web browser or port forwarding needed — add providers, switch routes, view stats, and monitor logs all from the terminal.

### Core Features

- 🚀 **Provider Management** - Manage multiple AI providers (Anthropic, OpenAI, Google, AWS Bedrock, etc.)
- 🔄 **Smart Routing** - Model-based intelligent routing and automatic failover
- 🛡️ **Circuit Breaker** - Built-in circuit breaker for fault tolerance
- 📊 **Token Statistics** - Track token usage and API costs per provider with input/output breakdown
- 💰 **Pricing Configuration** - Configure custom pricing per provider for accurate cost tracking
- 🔀 **Model Mappings** - Map custom model names to standard provider models
- 📝 **Request Logs** - View detailed request logs with pagination and filtering
- 🔌 **LLM Gateway Proxy** - Unified proxy endpoint for all CLI tools
- ⚙️ **Config Sync** - Automatic configuration sync to CLI tools (Claude Code, Codex, Gemini CLI, OpenCode, OpenClaw)
- 🔧 **Custom Headers** - Per-provider custom header overrides (User-Agent, etc.)
- 🔄 **Protocol Conversion** - Automatic Anthropic ↔ OpenAI protocol conversion, use any client with any provider
- 📦 **Provider Import/Export** - Export and import provider configurations for easy migration
- 🎨 **Beautiful TUI** - Intuitive terminal user interface with keyboard shortcuts, works over SSH

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/ubiomni/mrouter.git
cd mrouter

# Build and install
cargo build --release
cargo install --path .
```

### Basic Usage

```bash
# Start TUI (recommended)
mrouter

# Or use CLI commands
mrouter list                    # List all providers
mrouter switch <provider-name>  # Switch active provider
mrouter status                  # Show current status
mrouter stats                   # Show usage statistics

# Start proxy server
mrouter proxy start
mrouter proxy stop
mrouter proxy status
```

## TUI Keyboard Shortcuts

### Global
- `1-5` - Switch tabs (Providers/Proxy/Logs/Stats/Settings)
- `q` / `Ctrl+C` - Quit
- `?` / `F1` - Toggle help
- `r` - Refresh

### Providers Tab
- `↑↓` / `j/k` - Navigate list
- `Enter` - Activate provider
- `a` - Add new provider
- `e` - Edit provider
- `d` - Delete provider
- `v` - View supported models
- `p` - Configure pricing
- `o` - Configure headers (Auth Header + Custom Headers)
- `m` - Configure model mappings
- `r` - Reset circuit breaker
- `s` - Manage sync settings
- `I` - Import providers from JSON
- `E` - Export providers to JSON

### Proxy Tab
- `s` - Start proxy
- `p` - Stop proxy
- `e` - Edit proxy config

### Logs Tab
- `↑↓` / `j/k` - Navigate logs
- `←→` / `h/l` - Previous/Next page
- `Enter` / `Space` - View log detail
- `Esc` / `q` - Back to list (from detail)
- `r` - Refresh logs

### Stats Tab
- `t` - Toggle time range (Today/Week/Month/All)
- `r` - Refresh statistics

## Configuration

Configuration file: `~/.config/mrouter/config.toml`

```toml
[proxy]
bind = "127.0.0.1"
port = 4444
takeover_mode = false

[circuit_breaker]
failure_threshold = 5
success_threshold = 2
timeout_secs = 60
half_open_timeout_secs = 30

[log]
level = "info"
file = "~/mrouter.log"
```

## Supported Providers

- Anthropic (Claude)
- OpenAI (GPT-4, GPT-3.5)
- Google AI Studio (Gemini)
- AWS Bedrock
- Azure OpenAI
- Google Vertex AI
- Mistral AI
- Cohere
- DeepSeek
- xAI (Grok)
- Meta (Llama)
- MiniMax
- Zhipu AI
- Moonshot AI
- Baichuan
- OpenRouter
- Together AI
- Fireworks AI
- Groq
- Custom endpoints

## Proxy Mode

MRouter can act as a unified LLM Gateway proxy:

1. **Start the proxy**:
   ```bash
   mrouter proxy start
   ```

2. **Configure your CLI tools** to use `http://localhost:4444` as the base URL

3. **Switch providers** in the TUI - all CLI tools will automatically use the new provider

### Running as a Background Service

MRouter can run as a daemon on servers:

```bash
# Start proxy in background
nohup mrouter proxy start > /dev/null 2>&1 &

# Or use systemd (Linux)
sudo tee /etc/systemd/system/mrouter.service > /dev/null <<'EOF'
[Unit]
Description=MRouter LLM Gateway
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/mrouter proxy start
Restart=on-failure
User=your-username

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl enable --now mrouter
```

Perfect for remote development servers where you SSH in and want centralized provider management. Use the TUI (`mrouter`) to manage providers, view logs, and monitor stats — all from your terminal.

### CLI Tool Configuration Examples

**Claude Code**
```json
// ~/.claude/settings.json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://localhost:4444"
  }
}
```

**Codex**
```json
// ~/.codex/config.json
{
  "provider": "openai",
  "model": "gpt-4"
}
// ~/.codex/auth.json
{
  "openai_api_key": "any-placeholder-key"
}
```

**Gemini CLI**
```json
// ~/.config/gemini/config.json
{
  "base_url": "http://localhost:4444"
}
```

**OpenClaw**
```env
# ~/.openclaw/.env
BASE_URL=http://localhost:4444
```

> **Tip**: Use `s` key in the Providers tab to auto-sync configuration to your CLI tools, so you don't have to configure them manually.

### Custom Headers Per Provider

You can configure custom headers for each provider. When a custom header is set, it overrides the client's original header (e.g., `User-Agent`). If not set, headers are passed through from the client as-is.

Configure custom headers when adding/editing a provider in JSON format:

```json
{
  "User-Agent": "MyApp/1.0",
  "X-Custom-Header": "custom-value"
}
```

This is useful for:
- Sending a different `User-Agent` to providers than what the client sends
- Adding provider-specific headers required by certain APIs
- Overriding default headers for compatibility

## Protocol Conversion

MRouter supports automatic bidirectional protocol conversion between Anthropic and OpenAI formats. This allows you to use any client (e.g., Claude Code sending Anthropic format) with any provider (e.g., DeepSeek expecting OpenAI format) — the proxy handles the translation transparently.

### How It Works

- **Client format** is auto-detected from the request path: `/v1/messages` → Anthropic, `/v1/chat/completions` → OpenAI
- **Provider format** is determined by the provider's `API Format` setting
- When the client format differs from the provider format, MRouter automatically converts requests, responses, and SSE streaming events

### Configuration

Set the `API Format` field when adding/editing a provider:

| API Format | Behavior |
|---|---|
| **Auto** (default) | Passthrough — no conversion, requests forwarded as-is |
| **Anthropic** | Provider expects Anthropic protocol. If client sends OpenAI format, auto-convert |
| **OpenAI** | Provider expects OpenAI protocol. If client sends Anthropic format, auto-convert |

### Example: Claude Code → DeepSeek

Claude Code sends Anthropic format (`/v1/messages`), but DeepSeek uses OpenAI format:

1. Add DeepSeek provider with `API Format = OpenAI`
2. Claude Code sends request to `http://localhost:4444/v1/messages`
3. MRouter detects mismatch (client=Anthropic, provider=OpenAI)
4. Request is converted: Anthropic → OpenAI format, path → `/v1/chat/completions`
5. Response/SSE stream is converted back: OpenAI → Anthropic format

### What Gets Converted

- **Request**: system message, message content format, max_tokens, stop sequences, path
- **Response**: content blocks, finish reason, usage stats
- **SSE Streaming**: Anthropic events (`message_start`, `content_block_delta`, `message_delta`, `message_stop`) ↔ OpenAI chunks (`chat.completion.chunk`, `[DONE]`)

## Provider Import/Export

Export and import provider configurations for easy migration between machines or backup.

- **Export** (`Shift+E`): Saves all providers to `~/.mrouter/providers.json`
- **Import** (`Shift+I`): Loads providers from `~/.mrouter/providers.json`, skipping duplicates by name

## Token Statistics

MRouter tracks token usage for both streaming and non-streaming responses:

- Per-provider statistics
- Time-based filtering (Today/Week/Month/All)
- Input/Output token breakdown
- Cache token tracking (cache creation and cache read tokens)
- Cost tracking with custom pricing configuration

Enable/disable statistics per provider in the provider settings.

## Request Logs

View detailed request logs in the Logs tab:

- Paginated log view with navigation
- View full request/response details
- Filter by provider
- Timestamp and duration tracking
- Error message display

Navigate with arrow keys or `h/l`, press Enter to view details, and Esc to return to the list.

## Circuit Breaker

Automatic fault tolerance with circuit breaker pattern:

- **Closed**: Normal operation
- **Open**: Provider temporarily disabled after failures
- **Half-Open**: Testing if provider recovered

Reset circuit breaker manually with `r` key in Providers tab.

## Smart Routing

MRouter supports model-based intelligent routing:

1. **Model Matching**: Requests are routed to providers that support the specified model
2. **Automatic Failover**: If the preferred provider fails, automatically tries other providers
3. **Priority Sorting**: Providers are sorted by priority (lower number = higher priority)

### Configuring Supported Models

When adding/editing a provider, you can specify the list of models it supports:

```
Supported Models: gpt-4, gpt-4-turbo, gpt-3.5-turbo
```

Or use `Ctrl+F` to automatically fetch the model list from the API.

### Configuring Pricing

Press `p` on a provider in the Providers tab to configure custom pricing:

- Input price per million tokens
- Output price per million tokens
- Cache write price per million tokens
- Cache read price per million tokens

This enables accurate cost tracking in the Stats tab.

### Configuring Model Mappings

Press `m` on a provider in the Providers tab to configure model name mappings:

```
custom-model-name:actual-provider-model
my-gpt:gpt-4-turbo
```

This allows you to use custom model names that map to actual provider models.

## Input Dialog Shortcuts

When adding/editing providers:

- `Enter` - Submit
- `Esc` - Cancel
- `Tab` / `Shift+Tab` - Switch fields
- `Ctrl+U` - Clear current field
- `Ctrl+K` - Copy current field value
- `Ctrl+V` - Paste from clipboard
- `Ctrl+R` - Toggle password visibility
- `Ctrl+F` - Fetch model list from API
- `Space` - Toggle checkbox / Open model browser

## Config Sync

MRouter supports automatic configuration sync to the following CLI tools:

- **Claude Code**: `~/.claude/settings.json`
- **Codex**: `~/.codex/config.json` and `~/.codex/auth.json`
- **Gemini CLI**: `~/.config/gemini/config.json`
- **OpenCode**: `~/.opencode/config.json`
- **OpenClaw**: `~/.openclaw/.env`

Configure the `sync_to_cli_tools` field in provider settings to enable sync.

## Troubleshooting

### Circuit Breaker Open

If a provider's circuit breaker is open:

1. Check that the provider's API key and Base URL are correct
2. Check network connectivity
3. Press `r` in the Providers tab to reset the circuit breaker
4. Adjust circuit breaker thresholds in Settings

### Empty Token Statistics

Make sure:

1. Token statistics are enabled for the provider (check "Enable Token Stats" when editing provider)
2. Requests have been sent through the proxy
3. The upstream API includes usage information in responses

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Related Projects

- [Claude Code](https://github.com/anthropics/claude-code) - Anthropic's official CLI
- [OpenClaw](https://github.com/openclaw/openclaw) - Open-source AI CLI tool
