# rustaichat

Terminal-first multi-provider AI chat CLI focused on Gemini for the MVP stage.

## Features (MVP)

- `config` command to add/show/remove provider credentials stored in `~/.config/rustaichat/config.toml` (or platform equivalent).
- Provider abstraction with a working Google Gemini implementation (service account or API key).
- `chat` REPL with history reset (`/reset`), optional system prompt, and JSON history export via `--save`.
- `message` command for one-off prompts without entering the REPL.
- Basic retry/backoff for Google API calls plus placeholder streaming hooks for MVP+.

## Project Layout

```
rustaichat/
├─ Cargo.toml
├─ README.md
├─ examples/
│  └─ config.toml           # sample configuration
└─ src/
   ├─ main.rs               # entrypoint / command executor
   ├─ cli.rs                # clap CLI schema
   ├─ config.rs             # load/store provider configuration
   ├─ provider/
   │  ├─ mod.rs             # provider factory + exports
   │  ├─ trait_provider.rs  # shared provider trait + types
   │  ├─ google.rs          # Gemini implementation (MVP)
   │  ├─ anthropic.rs       # stub for Claude
   │  └─ openai.rs          # stub for OpenAI
   ├─ repl.rs               # REPL loop + history logic
   ├─ streaming.rs          # placeholder streaming helpers
   ├─ logger.rs             # chat history persistence helpers
   └─ utils.rs              # path helpers
```

## Getting Started

1. Install a recent Rust toolchain (`1.75+` recommended).
2. Build the CLI:

```powershell
cargo build
```

3. Configure the Google provider. With a service account JSON file:

```powershell
rustaichat config set google --service-account C:\path\to\sa.json --project-id my-project --default
```

   Or with an API key (uses Gemini REST key):

```powershell
rustaichat config set google --api-key your_api_key_here --default
```

4. Start chatting:

```powershell
rustaichat chat --model gemini-2.0-flash
```

5. Send a single message without the REPL (pass `--provider` unless you have already set a default):

```powershell
rustaichat message --provider google --model gemini-2.0-flash "Hello Gemini"
```

### npm installation (optional)

Once published to npm, the CLI can be installed globally:

```powershell
npm install -g rustaichat
rustaichat chat --model gemini-2.0-flash
```

> The npm package builds the Rust binary during installation, so make sure the Rust toolchain (`cargo`) is available on the target machine.

To test locally before publishing, run:

```powershell
npm install -g .
```

### Publishing to npm

1. Ensure the version in `package.json` matches `Cargo.toml` and bump as needed (`npm version patch`).
2. Run the test build locally (`npm install`) to confirm the installer compiles the binary.
3. Authenticate with npm (`npm login`) and publish: `npm publish --access public`.
4. Consumers can then `npm install -g rustaichat` to get the CLI.

### Configuration Notes


### Gemini Authentication Tips

- Service account mode requests an OAuth token with the `https://www.googleapis.com/auth/generative-language` scope.
- Ensure the service account has the *Generative Language API User* role within your GCP project and the Gemini API is enabled.
- API key mode works with the standard `generativelanguage.googleapis.com` key; keep keys secret and never commit them.

## Next Steps

- Implement true streaming responses (`reqwest::bytes_stream`) and incremental REPL output.
- Flesh out Anthropic and OpenAI providers using the shared trait.
- Add unit tests around config parsing and provider adapters.
- Provide optional encrypted storage for secrets.
