# PyDE

> [!WARNING]
> PyDE is currently in early development, so features like syntax highlighting may be prone to bugs.

PyDE is an independent, lightweight Python IDE/Text Editor, made using Rust (WebAssembly).

## Features

- Python runs in the browser with [Pyodide](https://pyodide.org), off the main thread. Output streams while a program runs, `input()` works, and Stop ends a runaway program.
- Type checking and hover information from [Ty](https://github.com/astral-sh/ty), also in a worker. Packages that Pyodide can load, such as numpy, are type checked too.
- Semantic syntax colouring from Ruff's parser, with diagnostics underlined and unreachable or unused code dimmed.
- Format with Ruff's default style (Shift+Alt+F), smart indentation, and indent guides.
- Open and save local files (Ctrl/Cmd+O and Ctrl/Cmd+S). Click the file name to rename it.

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

This will output to `dist/` where you can serve it somewhere else.

## Layout

| Crate                  | What it is                                                                                                                            |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/wasm`          | The page's entry point.                                                                                                               |
| `crates/app`           | The user interface: components, state, and editing logic.                                                                             |
| `crates/analysis`      | Syntax colouring (`highlight/`), formatting, and the messages the type checker sends. Shared by the page and the type-checker worker. |
| `crates/ty-worker`     | The Ty type checker, running in a Web Worker. `ty_engine/` splits it into the database, checking, hover, signatures and docstrings.   |
| `crates/python-worker` | Pyodide, running in a Web Worker. Its Python helpers are plain files in `python/`.                                                    |
| `crates/js`            | Messaging helpers shared by the page and both workers.                                                                                |
| `web/`                 | `index.html` and the single stylesheet, `main.css`.                                                                                   |

Inside `crates/app/src`: `components/` holds the views, `state/` has one controller per concern (document, type checker, runner, status), `clients/` talks to the two workers, and `editing/` holds indentation, text layout, and textarea edits.

## Development

```
cargo check --workspace --target wasm32-unknown-unknown
cargo clippy --workspace --all-targets
cargo fmt --all
```

Clippy's `all` group is enabled for every crate through `[workspace.lints]`. Rust is formatted with `rustfmt`, and the `view!` blocks with `leptosfmt`.

With Nix, `nix develop` provides all of these tools, and `nix fmt` runs every formatter (Rust, `view!` blocks, CSS, Markdown, TOML, Nix) in the right order.

The stylesheet names classes `block`, `block-part`, and `is-state`, and takes every colour, size, and gap from the tokens at the top of `web/main.css`.
