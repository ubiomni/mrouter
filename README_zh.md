# MRouter - LLM 网关路由器

> **One Endpoint, Every Model. Route, Monitor, and Failover — All From Your Terminal.**

[English](README.md) | [中文](README_zh.md)

一个端点接入所有大模型 — 智能路由、自动容灾、CLI 工具开箱即用。终端原生的 LLM 网关。

## 为什么选择 MRouter？

- **服务器终端配置** - 通过 SSH 在远程服务器上使用完整的 TUI 管理。无需 Web UI — 直接在任何终端会话中配置提供商、监控统计、切换路由
- **单点配置** - 一次性配置所有提供商，应用只需指向 `http://localhost:4444`
- **基于模型的自动路由** - 指定模型名称（如 `gpt-4`、`claude-opus-4`、`gemini-pro`），MRouter 自动路由到正确的提供商
- **集中式 API 密钥管理** - 所有 API 密钥安全地存储在一个地方，无需为每个应用单独配置
- **自动故障转移与熔断器** - 如果一个提供商失败，自动尝试备用提供商，内置容错机制
- **自定义请求头** - 按提供商覆盖 `User-Agent` 等请求头，用于版本控制和兼容性调整
- **CLI 工具一键同步** - 一键同步配置到 Claude Code、Codex、Gemini CLI、OpenCode、OpenClaw

## 核心使用场景

**服务器应用的多提供商路由**

当在服务器上部署需要访问多个 AI 提供商的应用时，MRouter 充当统一网关。您的应用只需指向 `http://localhost:4444` — 无需在每个应用中配置各个提供商的 API 密钥和端点。

```bash
# 所有请求都发送到 localhost:4444 — MRouter 根据模型名称自动路由
curl http://localhost:4444/v1/messages -d '{"model": "gpt-4", ...}'          # → OpenAI
curl http://localhost:4444/v1/messages -d '{"model": "claude-opus-4", ...}'  # → Anthropic
curl http://localhost:4444/v1/messages -d '{"model": "gemini-pro", ...}'     # → Google
```

**通过 SSH 远程服务器管理**

MRouter 的 TUI 支持通过 SSH 使用，非常适合在远程开发服务器或云实例上管理 AI 路由：

```bash
ssh my-server
mrouter              # 在终端中进行完整的 TUI 管理
```

无需浏览器或端口转发 — 直接在终端中添加提供商、切换路由、查看统计、监控日志。

## 功能特性

- 🚀 **提供商管理** - 管理多个 AI 提供商（Anthropic、OpenAI、Google、AWS Bedrock 等）
- 🔄 **智能路由** - 基于模型的智能路由和自动故障转移
- 🛡️ **熔断器** - 内置熔断器实现容错
- 📊 **Token 统计** - 按提供商跟踪 token 使用量和 API 成本
- 💰 **定价配置** - 为每个提供商配置自定义定价，实现精确的成本跟踪
- 🔀 **模型映射** - 将自定义模型名称映射到标准提供商模型
- 📝 **请求日志** - 查看详细的请求日志，支持分页和过滤
- 🔌 **LLM 网关代理** - 为所有 CLI 工具提供统一的代理端点
- ⚙️ **配置同步** - 自动同步配置到 CLI 工具（Claude Code、Codex、Gemini CLI、OpenCode、OpenClaw）
- 🔧 **自定义请求头** - 按提供商自定义请求头覆盖（User-Agent 等）
- 🎨 **精美 TUI** - 直观的终端用户界面，支持通过 SSH 远程使用

## 快速开始

### 安装

```bash
# 克隆仓库
git clone https://github.com/ubiomni/mrouter.git
cd mrouter

# 构建和安装
cargo build --release
cargo install --path .
```

### 基本使用

```bash
# 启动 TUI（推荐）
mrouter

# 或使用 CLI 命令
mrouter list                    # 列出所有提供商
mrouter switch <provider-name>  # 切换活动提供商
mrouter status                  # 显示当前状态
mrouter stats                   # 显示使用统计

# 启动代理服务器
mrouter proxy start
mrouter proxy stop
mrouter proxy status
```

## TUI 快捷键

### 全局
- `1-5` - 切换标签（Providers/Proxy/Logs/Stats/Settings）
- `q` / `Ctrl+C` - 退出
- `?` / `F1` - 切换帮助
- `r` - 刷新

### Providers 标签
- `↑↓` / `j/k` - 导航列表
- `Enter` - 激活提供商
- `a` - 添加新提供商
- `e` - 编辑提供商
- `d` - 删除提供商
- `v` - 查看支持的模型
- `p` - 配置定价
- `o` - 配置请求头（Auth Header + 自定义请求头）
- `m` - 配置模型映射
- `r` - 重置熔断器
- `s` - 管理同步设置

### Proxy 标签
- `s` - 启动代理
- `p` - 停止代理
- `e` - 编辑代理配置

### Logs 标签
- `↑↓` / `j/k` - 导航日志
- `←→` / `h/l` - 上一页/下一页
- `Enter` / `Space` - 查看日志详情
- `Esc` / `q` - 返回列表（从详情页）
- `r` - 刷新日志

### Stats 标签
- `t` - 切换时间范围（今天/本周/本月/全部）
- `r` - 刷新统计

## 配置

配置文件：`~/.config/mrouter/config.toml`

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

## 支持的提供商

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
- 智谱 AI
- 月之暗面 (Moonshot)
- 百川智能
- OpenRouter
- Together AI
- Fireworks AI
- Groq
- 自定义端点

## 代理模式

MRouter 可以作为统一的 LLM 网关代理：

1. **启动代理**：
   ```bash
   mrouter proxy start
   ```

2. **配置 CLI 工具**使用 `http://localhost:4444` 作为 base URL

3. **在 TUI 中切换提供商** - 所有 CLI 工具将自动使用新的提供商

### 作为后台服务运行

MRouter 可以在服务器上作为守护进程运行：

```bash
# 后台启动代理
nohup mrouter proxy start > /dev/null 2>&1 &

# 或使用 systemd（Linux）
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

非常适合远程开发服务器 — 通过 SSH 连接后使用 TUI（`mrouter`）管理提供商、查看日志、监控统计，一切都在终端中完成。

### CLI 工具配置示例

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

> **提示**：在 Providers 标签中使用 `s` 键可以自动同步配置到 CLI 工具，无需手动配置。

### 按提供商自定义请求头

您可以为每个提供商配置自定义请求头。设置自定义请求头后，会覆盖客户端发送的原始请求头（如 `User-Agent`）。如果未设置，请求头将按原样从客户端透传。

在添加/编辑提供商时，以 JSON 格式配置自定义请求头：

```json
{
  "User-Agent": "MyApp/1.0",
  "X-Custom-Header": "custom-value"
}
```

适用场景：
- 向提供商发送与客户端不同的 `User-Agent`
- 添加某些 API 要求的提供商特定请求头
- 覆盖默认请求头以实现兼容性调整

## Token 统计

MRouter 跟踪流式和非流式响应的 token 使用量：

- 按提供商统计
- 基于时间的过滤（今天/本周/本月/全部）
- 输入/输出 token 分解
- 缓存 token 跟踪（缓存创建和缓存读取 token）
- 使用自定义定价配置进行成本跟踪

在提供商设置中启用/禁用每个提供商的统计。

## 请求日志

在 Logs 标签中查看详细的请求日志：

- 分页日志视图，支持导航
- 查看完整的请求/响应详情
- 按提供商过滤
- 时间戳和持续时间跟踪
- 错误消息显示

使用方向键或 `h/l` 导航，按 Enter 查看详情，按 Esc 返回列表。

## 熔断器

使用熔断器模式实现自动容错：

- **关闭（Closed）**：正常运行
- **打开（Open）**：失败后提供商暂时禁用
- **半开（Half-Open）**：测试提供商是否恢复

在 Providers 标签中使用 `r` 键手动重置熔断器。

## 智能路由

MRouter 支持基于模型的智能路由：

1. **模型匹配**：请求中指定的模型会优先路由到支持该模型的提供商
2. **自动故障转移**：如果首选提供商失败，自动尝试其他提供商
3. **优先级排序**：按提供商优先级排序（数字越小优先级越高）

### 配置支持的模型

在添加/编辑提供商时，可以指定该提供商支持的模型列表：

```
Supported Models: gpt-4, gpt-4-turbo, gpt-3.5-turbo
```

或使用 `Ctrl+F` 从 API 自动获取模型列表。

### 配置定价

在 Providers 标签中按 `p` 键配置提供商的自定义定价：

- 输入 token 价格（每百万 token）
- 输出 token 价格（每百万 token）
- 缓存写入价格（每百万 token）
- 缓存读取价格（每百万 token）

这样可以在 Stats 标签中实现精确的成本跟踪。

### 配置模型映射

在 Providers 标签中按 `m` 键配置模型名称映射：

```
自定义模型名:实际提供商模型
my-gpt:gpt-4-turbo
```

这允许您使用自定义模型名称，并将其映射到实际的提供商模型。

## 输入对话框快捷键

在添加/编辑提供商时：

- `Enter` - 提交
- `Esc` - 取消
- `Tab` / `Shift+Tab` - 切换字段
- `Ctrl+U` - 清空当前字段
- `Ctrl+K` - 复制当前字段值
- `Ctrl+V` - 从剪贴板粘贴
- `Ctrl+R` - 切换密码可见性
- `Ctrl+F` - 从 API 获取模型列表
- `Space` - 切换复选框 / 打开模型浏览器

## 配置同步

MRouter 支持自动同步配置到以下 CLI 工具：

- **Claude Code**: `~/.claude/settings.json`
- **Codex**: `~/.codex/config.json` 和 `~/.codex/auth.json`
- **Gemini CLI**: `~/.config/gemini/config.json`
- **OpenCode**: `~/.opencode/config.json`
- **OpenClaw**: `~/.openclaw/.env`

在提供商设置中配置 `sync_to_cli_tools` 字段来启用同步。

## 故障排除

### 熔断器打开

如果提供商的熔断器打开：

1. 检查提供商的 API 密钥和 Base URL 是否正确
2. 检查网络连接
3. 在 Providers 标签中按 `r` 重置熔断器
4. 调整 Settings 中的熔断器阈值

### Token 统计为空

确保：

1. 提供商已启用 Token 统计（在编辑提供商时勾选 "Enable Token Stats"）
2. 已通过代理发送请求
3. 上游 API 在响应中包含 usage 信息

## 许可证

MIT License - 详见 [LICENSE](LICENSE) 文件。

## 贡献

欢迎贡献！请随时提交 Pull Request。

## 相关项目

- [Claude Code](https://github.com/anthropics/claude-code) - Anthropic 官方 CLI
- [OpenClaw](https://github.com/openclaw/openclaw) - 开源 AI CLI 工具
