---
title: AI
description: AI in Forge - model providers (Anthropic, OpenAI, OpenRouter, Ollama, LM Studio, llama.cpp, any OpenAI-compatible server), AI commit messages, and the AI Chat with Claude Code and Gemini CLI including permission modes.
order: 210
tags: [ai, llm, claude, claude code, gemini, ollama, openai, chat]
---

# AI

AI is part of Forge's core and works with **your own** providers and tools; Forge ships no
credentials and no subscription. There are two parts:

- **AI providers**: language model APIs Forge calls itself, used for AI commit messages and
  available to plugins.
- **AI Chat**: Claude Code or Gemini CLI, installed on your machine with your own login, run
  headless and rendered as a native chat.

## Providers

Settings | Tools | **AI Providers**:

![AI Providers](../../screenshots/ai-providers-settings.png)

**Add Provider** offers presets:

| Kind | Default base URL | API key | Default model |
| --- | --- | --- | --- |
| Anthropic (Claude, Messages API) | `https://api.anthropic.com/v1` | required (`ANTHROPIC_API_KEY` as fallback) | Forge's current Claude default |
| OpenAI | `https://api.openai.com/v1` | required (`OPENAI_API_KEY`) | `gpt-4.1` |
| OpenRouter | `https://openrouter.ai/api/v1` | required (`OPENROUTER_API_KEY`) | `anthropic/claude-sonnet-5.5` |
| Ollama | `http://localhost:11434/v1` | not required | pick one |
| LM Studio | `http://localhost:1234/v1` | not required | pick one |
| llama.cpp (`llama-server`) | `http://localhost:8080/v1` | not required | pick one |
| OpenAI-compatible (custom) | `http://localhost:8000/v1` | optional | pick one |

![Add a preset](../../screenshots/ai-providers-add-preset.png)

For each provider set **Name**, **Base URL**, **API key** and **Default model**. **Fetch
Models** lists the models the server offers (for local servers: "Fetch models from the running
server"). **Test Connection** reports latency and the number of models, or a hint such as "is
Ollama running? Start it with `ollama serve`".

![Test connection](../../screenshots/ai-providers-test-connection.png)

- API keys are stored in the OS keychain (service `forge-ai`), never in files ("Will be stored
  in the system keychain on Apply"). Without a stored key, the environment variables above are
  used.
- The provider list lives in `ai.toml` in Forge's config folder.
- **Set as Default Provider** chooses the provider used by features without their own choice.
- **Models per feature**: pick the provider/model for **Chat**, **Code completion** ("prefer a
  fast local model") and **Commit messages** separately.

> **Note:** among the features listed under "Models per feature", only AI commit messages use
> a provider on this version; inline code completion and a provider-based chat are not
> implemented yet. The AI Chat uses the agent CLIs below. The "Google Gemini" provider kind
> declared in the AI plugin manifest has no implementation yet; use Gemini through Gemini CLI
> or an OpenAI-compatible endpoint.

## AI commit messages

In the Commit tool window, **Generate Commit Message with AI** writes a message for the ticked
changes. See [Git](git.md#ai-commit-messages).

## AI Chat

The **AI Chat** tool window (right stripe, or the ✨ button in the main toolbar) runs an
agentic coding CLI in your project and renders its output as a chat: text, thinking, tool calls
as cards, file edits as diffs, permission questions as buttons. No terminal is involved.

![AI Chat](../../screenshots/ai-chat-streaming.png)

### Agents

| Agent | Needs | Install |
| --- | --- | --- |
| **Claude Code** | the `claude` CLI, logged in (subscription or API key) | `npm install -g @anthropic-ai/claude-code` |
| **Gemini CLI** | the `gemini` CLI, logged in | `npm install -g @google/gemini-cli` |

Pick the agent in the chat header. If the CLI is not found, the chat says "Not installed" with
the install command; set its path in Settings | Tools | **AI Agents** (`agents.<id>.path`;
empty = find it on `PATH`).

![Agent picker](../../screenshots/ai-chat-agent-picker.png)

![Agents settings](../../screenshots/settings-ai-agents.png)

### Writing messages

- `Enter` sends, `Shift+Enter` inserts a line break, `Esc` stops the running turn.
- **Attach context** (`@`) attaches the current file, the current selection
  (`file:12-20`), or another open file.
- Slash commands: `/clear` starts a new chat; for Claude Code also `/compact`, `/review`,
  `/init` and `/cost`. `Tab` completes them.
- Suggested prompts appear in an empty chat.

![Attach context](../../screenshots/ai-chat-attach.png)

### Models and modes

The header has a **Model** and a **Permission mode** selector; defaults come from Settings |
Tools | AI Agents (`agents.<id>.model`, `agents.<id>.permissionMode`).

| Claude Code mode | Behaviour |
| --- | --- |
| Ask before edits (default) | confirm every edit and command |
| Auto-accept edits | apply file edits without asking; commands still ask |
| Plan mode | explore and plan, no changes |
| Bypass permissions | never ask — trusted projects only |

Gemini CLI cannot ask inside the IDE, so its mode decides which tools it gets:

| Gemini CLI mode | Behaviour |
| --- | --- |
| Read-only (default) | read and search, no edits or commands |
| Auto-accept edits | edit files without asking, no commands |
| YOLO | edits and commands, never asks |

![Gemini modes](../../screenshots/gemini-modes.png)

### Permission requests

With Claude Code in "Ask before edits", each edit or command shows a card: **Allow once**,
**Allow for this session** ("Don't ask again for this exact command until the chat ends") or
**Deny**. Edit cards show the diff; **Open diff** opens it in a dialog.

![Permission for a command](../../screenshots/chat-permission-command.png)

![Edit diff](../../screenshots/ai-chat-diff-dialog.png)

Files changed by the agent are reloaded in open editor tabs automatically.

### History

**Chat history** lists the previous chats of this Forge session (the list is not kept after a
restart); selecting one resumes the CLI's conversation. **New chat** starts over. The end of a turn shows duration, tokens and cost when
the CLI reports them.

![Finished turn](../../screenshots/ai-chat-finished.png)

> **Note:** the AI Chat runs the CLI on this machine; it is not available in
> [remote projects](remote-development.md) yet.

## Privacy

Forge sends data only to the providers you configure and the CLIs you run: commit diffs to the
commit message provider, your chat messages and attached context to the agent CLI (which talks
to its own service). Nothing is sent anywhere by default.
