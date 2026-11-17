# picusbase-function-sdk

The Rust side of a native-v1 [PicusBase](https://github.com/picusbase) compute function: the
`HttpHandler` trait, the request and response envelopes, `host::{query_read_only, execute_write,
http_request, log_*}` (database and outbound HTTP through the platform's mediator) and the
`native::run_native_worker` loop.

It runs inside the function's own process, which handles one request at a time over stdin and
stdout. It is deliberately synchronous and depends only on `serde` and `serde_json`; the client
for calling PicusBase from outside is the separate `picusbase-sdk` crate.

```toml
[dependencies]
picusbase-function-sdk = "0.1"
```

```rust
// src/lib.rs
use picusbase_function_sdk::{Error, ExecutionContext, HttpHandler, HttpRequest, HttpResponse};
use std::collections::HashMap;

pub struct Echo;

impl HttpHandler for Echo {
    fn handle(req: HttpRequest, _ctx: ExecutionContext) -> Result<HttpResponse, Error> {
        Ok(HttpResponse { status: 200, headers: HashMap::new(), body: req.body.unwrap_or_default() })
    }
}

// src/bin/native_worker.rs
// fn main() { picusbase_function_sdk::native::run_native_worker::<my_crate::Echo>() }
```

See [CHANGELOG.md](CHANGELOG.md) for breaking changes.
