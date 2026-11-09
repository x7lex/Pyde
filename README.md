# PyDE :3

> [!WARNING]  
> PyDE is currently in early development, so features like syntax highlighting may be prone to bugs.

PyDE is an independent, lightweight Python IDE/Text Editor, made using Rust (WebAssembly) 
## Building & Running
```
rustup target add wasm32-unknown-unknown
cargo install trunk # if you don't have it installed
trunk serve --release
```
> [!NOTE]  
> `input()` needs the page to be cross-origin isolated. `trunk serve` sends the required headers (see `Trunk.toml`). A production host must also send `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`.

If you want to build it and deploy it somewhere else:
```
trunk build # debug
trunk build --release # stable
```
this will output to `dist/` where you can serve it somewhere else.
