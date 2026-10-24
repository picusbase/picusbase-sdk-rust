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

## Versioning

Both crates follow semantic versioning. A release is a git tag `vX.Y.Z` that matches the
`version` of both crates; the tag publishes `picusbase-errors` first, then `picusbase-sdk`, to
crates.io. Pre-1.0, a minor bump may contain breaking changes; each release notes them.

## License

MIT. See [LICENSE](LICENSE).
