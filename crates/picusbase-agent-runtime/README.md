# picusbase-agent-runtime

Orchestration core of the AI assistant for apps on [PicusBase](https://github.com/picusbase): the
tool-call loop, tool schemas generated from a function's `openapi.yaml`, record links and confirm
actions. It names no tables; each app's function reads and writes its own and calls `turn::run` in
the middle. All network calls go through `picusbase-function-sdk`'s `host::http_request`, and it
never sees an LLM key (the platform's mediator holds the keys).

```toml
[dependencies]
picusbase-agent-runtime = "0.1"
```

Versioned and released together with `picusbase-function-sdk`.
