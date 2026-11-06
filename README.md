# pyde

A mini Python IDE that runs in the browser, powered by Rust + WASM.

## Running

```
trunk serve
```

`input()` needs the page to be cross-origin isolated. `trunk serve` sends the required headers (see `Trunk.toml`). A production host must also send `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`.
