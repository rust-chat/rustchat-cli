# rustaichat

Terminal-first multi-provider AI chat CLI focused on Gemini for the MVP stage. The package includes npm packaging support that compiles the release binary during installation.

## Overview

`rustaichat` is a terminal-first Rust CLI that orchestrates multi-provider chats by loading credentials from `~/.config/rustaichat/config.toml`, sending messages to the configured provider, and persisting session history. The MVP ships with Google Gemini support and provides a provider trait for adding Anthropic/OpenAI adapters.

## Features

- Config management & secrets store: `config` command stores multiple provider credentials and exposes `set`, `get`, `list`, and `delete` operations. Example:

  ```powershell
  rustaichat config set google --api-key <YOUR_KEY> --default
  ```

- Chat REPL + single-shot messaging: `chat` supports a system prompt, `/reset`, and `--save` for exporting JSON history. `message` sends a one-off request without entering the REPL.

- Provider abstraction: Implementations conform to the trait in `provider/trait_provider.rs`. The repo includes a Google Gemini implementation and stubs for Anthropic/OpenAI.

- Retry/backoff + streaming placeholder: Basic retry/backoff for API calls and a streaming placeholder in `streaming.rs` for future incremental output.

## Project Layout

```
rustaichat/
├─ Cargo.toml              # Rust dependencies + CLI metadata
├─ README.md               # This document
├─ examples/
│  └─ config.toml          # Example config
└─ src/
   ├─ main.rs              # CLI entrypoint and command dispatch
   ├─ cli.rs               # clap schema and subcommands
   ├─ config.rs            # config file load/save and default provider
   ├─ provider/
   │  ├─ mod.rs            # factory and exports
   │  ├─ trait_provider.rs # common provider trait
   │  ├─ google.rs         # Gemini MVP implementation
   │  ├─ anthropic.rs      # Anthropic stub
   │  └─ openai.rs         # OpenAI stub
   ├─ repl.rs              # REPL loop and history logic
   ├─ streaming.rs         # streaming helpers (placeholder)
   ├─ logger.rs            # history persistence helpers
   └─ utils.rs             # path/file helpers
```

## Quick Start

1. Install a recent Rust toolchain (1.75+ recommended).

```powershell
rustup toolchain install stable
```

2. Build the CLI locally:

```powershell
cargo build --release
```

3. Register provider credentials (example for Google Gemini):

```powershell
rustaichat config set google --api-key <YOUR_KEY> --default
```

4. Start the chat REPL:

```powershell
rustaichat chat --model gemini-2.0-flash
```

5. Send a single message without the REPL:

```powershell
rustaichat message --provider google --model gemini-2.0-flash "Hello Gemini"
```

## npm Packaging

### Installation

This package is published on npm and is available now. Install the CLI globally with:

```powershell
npm install -g rustaichat
rustaichat chat --model gemini-2.0-flash
```

The package runs `scripts/postinstall.js`, which will call `cargo build --release` to build the binary if a prebuilt executable for your platform is not bundled. If the postinstall step needs to compile from source, ensure a Rust toolchain (`cargo`) is available on the machine.

To test locally before publishing:

```powershell
npm install -g .
```

### Publishing

1. Ensure `Cargo.toml` and `package.json` `version` fields match.
2. Verify local build: `npm install` or `npm install -g .` to confirm `postinstall` runs `cargo build`.
3. `npm login` and `npm publish --access public` to publish the package.

## Configuration

- Config file location: `~/.config/rustaichat/config.toml` (on Windows use `%APPDATA%\rustaichat\config.toml`).
- Example config: see `examples/config.toml` for fields like `default_project`, `model`, and `service_account`.
- Commands: `rustaichat config list`, `rustaichat config get <provider>`, `rustaichat config delete <provider>`.

## Gemini Authentication Tips

- Service account: request OAuth tokens with the `https://www.googleapis.com/auth/generative-language` scope and grant the service account the *Generative Language API User* role in GCP.
- API key: use a `generativelanguage.googleapis.com` API key and keep it secret.

## Next Steps

- Implement streaming via `reqwest::bytes_stream` and incremental REPL output.
- Complete Anthropic and OpenAI provider implementations.
- Add unit tests for config parsing, provider adapters, and REPL history rotation.
- Add CI packaging tests that run `npm install` and maintain release notes.

---

*English-only documentation for the Rust CLI, provider configuration, and npm packaging workflows.*
