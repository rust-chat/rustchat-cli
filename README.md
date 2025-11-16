# rustaichat

Terminal-first, multi-provider AI chat CLI with first-class streaming. Gemini is fully exercised in the MVP, while Claude (Anthropic) and OpenAI providers are wired up but still waiting on manual validation.

## Overview

`rustaichat` loads credentials from `~/.config/rustaichat/config.toml`, selects the requested provider, and streams chat responses inside either a REPL (`chat`) or a one-off `message` command. All providers conform to a single trait (`provider/trait_provider.rs`), so new backends only require implementing that interface.

> **Status:** Google Gemini paths are verified daily. Claude/OpenAI adapters share the same CLI plumbing but have **not been tested against the live APIs yet**—please treat them as experimental until you confirm them with your own credentials.

## Feature Highlights

- **Unified config + secrets:** `rustaichat config set <name> --kind <google|anthropic|openai>` stores multiple credentials, marks defaults, and keeps provider-specific hints.
- **Streaming chat + single-shot messaging:** `chat` exposes `/reset`, `--system`, `--stream`, and `--save`. `message` sends one prompt without entering the REPL.
- **Multiple providers out of the box:** Gemini (service account or API key), Claude (Anthropic Messages API), and OpenAI Chat Completions share the same CLI switches.
- **Smarter streaming:** Gemini streaming now yields only fresh deltas, preventing duplicate or half-baked tokens. Anthropic/OpenAI streams use the same SSE event parser for consistent output.
- **npm packaging with prebuilts:** `scripts/postinstall.js` downloads release binaries for Windows/macOS/Linux and falls back to `cargo build --release` when an artifact is missing.

## Project Layout

```
rustaichat/
├─ Cargo.toml              # Rust dependencies + metadata
├─ README.md               # This document
├─ examples/config.toml    # Sample multi-provider config
├─ scripts/                # npm postinstall + runner helpers
└─ src/
   ├─ cli.rs               # clap schema
   ├─ config.rs            # load/save config + validation
   ├─ provider/
   │  ├─ mod.rs            # provider factory
   │  ├─ trait_provider.rs # shared trait + message types
   │  ├─ google.rs         # Gemini implementation
   │  ├─ anthropic.rs      # Claude (API key)
   │  └─ openai.rs         # OpenAI Chat Completions
   ├─ repl.rs              # REPL/session handling
   ├─ streaming.rs         # shared stream helpers
   ├─ logger.rs            # history persistence
   └─ utils.rs             # misc helpers
```

## Installation

### Via Cargo (local dev)

```powershell
rustup toolchain install stable
cargo build --release
# Run the freshly built binary
target\release\rustaichat.exe chat --model gemini-2.0-flash
```

### Via npm (prebuilt binaries)

```powershell
npm install -g rustaichat
rustaichat chat --model gemini-2.0-flash
```

`scripts/postinstall.js` pulls the appropriate asset (for example `rustaichat-windows-x86_64.exe`) from GitHub Releases. If it cannot find one, it transparently runs `cargo build --release`, so keep a Rust toolchain installed as a fallback.

## Configuration

Configs live at `~/.config/rustaichat/config.toml` (`%APPDATA%\rustaichat\config.toml` on Windows). Use the CLI to manage entries:

```powershell
rustaichat config set google --service-account C:\keys\sa.json --default
rustaichat config set claude --kind anthropic --api-key <ANTHROPIC_KEY>
rustaichat config set openai --kind openai --api-key <OPENAI_KEY> --shared-default-model gpt-4o-mini
rustaichat config show
```

Minimal TOML example:

```toml
default_provider = "google"

[providers.google]
type = "google"
service_account_file = "C:/keys/sa.json"
default_model = "gemini-2.0-flash"

[providers.claude]
type = "anthropic"
api_key = "ANTHROPIC_API_KEY"
default_model = "claude-3-sonnet-20240229"

[providers.openai]
type = "openai"
api_key = "OPENAI_API_KEY"
default_model = "gpt-4o-mini"
```

## Usage

```powershell
# Interactive REPL using the default provider
rustaichat chat

# Force a specific provider/model + streaming
rustaichat chat --provider claude --model claude-3-haiku-20240307 --stream

# One-off prompt without the REPL
rustaichat message --provider openai --model gpt-4o-mini "Summarize the agenda"

# Persist chat history to JSON
rustaichat chat --save session.json
```

### Provider-specific notes

- **Google Gemini:** supports service accounts (OAuth scope `https://www.googleapis.com/auth/generative-language`) or API keys. Streaming now emits only fresh deltas, so the REPL no longer prints duplicated prefixes.
- **Anthropic Claude:** calls `/v1/messages` with `x-api-key` and `anthropic-version: 2023-06-01`, parsing SSE `content_block_delta` events. *Still untested in a real environment—please report any issues.*
- **OpenAI:** targets `/v1/chat/completions` with standard streaming chunks. *Also untested so far; confirm with your workspace before relying on it in production.*

## Streaming Behavior

- Gemini responses pass through a JSON-frame detector that peels complete payloads from arbitrary chunking, then emits only the newly added suffix.
- Anthropic and OpenAI share a lightweight SSE accumulator that waits for blank-line delimiters, parses the JSON payload, and yields real text deltas only.
- The REPL flushes stdout per delta, so responses stay snappy while respecting provider pacing.

## npm Publishing Checklist

1. Ensure `Cargo.toml` and `package.json` versions match.
2. Build/upload release binaries (Windows/macOS/Linux) so `postinstall` can download instead of compiling.
3. Run `npm install -g .` locally to confirm `postinstall.js` finds/installs the binary.
4. `npm publish --access public`.

## Known Limitations

- Claude/OpenAI paths compile and stream locally but **have not been verified against live APIs yet**; treat them as beta quality.
- Automatic history persistence is limited to the optional `--save` flag; richer logging/export flows are planned.
- There is no CI or automated testing pipeline—the short-term focus has been feature velocity.

## Roadmap

- Add integration tests with mocked SSE streams to lock in parser behavior.
- Ship CI workflows that build/upload release assets automatically for npm consumers.
- Extend logging/export (Markdown transcripts, Discord/webhook sinks).
- Explore a TUI once the CLI stabilizes.

---

*English-only documentation. Please report issues—especially for the newly added Claude/OpenAI adapters so we can remove the "untested" label quickly.*
