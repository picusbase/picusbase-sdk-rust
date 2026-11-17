# picusbase-sdk (Rust)

Rust client for the [PicusBase](https://github.com/picusbase) platform: sign-in, platform
resources, storage, functions and realtime changes. Works on native targets and on
`wasm32-unknown-unknown` (browser).

```toml
[dependencies]
picusbase-sdk = "0.1"
```

```rust
use picusbase_sdk::PicusClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Base URL of the PicusBase Console, optional project reference.
    let client = PicusClient::init("https://console.example.com", "", Some("p_xxxxxxxx"));
    let auth = client.auth.as_ref().expect("auth client");

    let session = auth
        .sign_in_with_password("user@example.com", "password", "oauth-client-id")
        .await?;

    // Attach the access token (and project schema) for the other surfaces.
    let authed = client
        .with_token(session.access_token)
        .with_schema("p_xxxxxxxx");
    let _storage = authed.storage();
    Ok(())
}
```

## Crates

| Crate | Purpose |
|---|---|
| `picusbase-sdk` | The client: `auth`, `platform`, `storage`, `functions`, `compute`, `realtime`, `system`, `meta`, `pools`. |
| `picusbase-errors` | The `PicusError` type returned by every call. |
| `picusbase-function-sdk` | The SDK *inside* a native-v1 compute function (`HttpHandler`, `host::*`, the worker loop). Synchronous, depends only on `serde`/`serde_json`; kept apart from the client so a function does not pull in its async dependencies. |
| `picusbase-agent-runtime` | Orchestration core of the AI assistant for apps; depends on `picusbase-function-sdk`. |

## Versioning

All crates follow semantic versioning. There are two release lines, each a git tag that matches the
`version` of its crates:

- `vX.Y.Z` publishes `picusbase-errors`, then `picusbase-sdk`.
- `function-sdk-vX.Y.Z` publishes `picusbase-function-sdk`, then `picusbase-agent-runtime`; it needs an entry
  for that version in `crates/picusbase-function-sdk/CHANGELOG.md`.

Pre-1.0, a minor bump may contain breaking changes; each release notes them. A change in what the platform
injects into a function (environment variables, the stdin/stdout envelope) is a release of the function SDK
with a note, never a silent change.

## License

MIT. See [LICENSE](LICENSE).
