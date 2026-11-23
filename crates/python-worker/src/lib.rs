//! The Pyodide worker. Running off the main thread keeps the page responsive,
//! streams output while a program runs, and lets Stop terminate the worker.
//!
//! Receives: `{ type: "run", name, code, stdin? }`
//!           `{ type: "prefetch", code }`, to load the packages `code` imports
//!           and report their type information.
//! Sends:    `{ type: "ready" }`
//!           `{ type: "output", lines: [{ text, isError }] }`
//!           `{ type: "input", prompt }`
//!           `{ type: "done" }`
//!           `{ type: "prefetching", names }`
//!           `{ type: "prefetched", json, failed }`  (`json` is a `Vec<PackageFiles>`)
//!           `{ type: "failed", message }`
//!
//! `stdin` is a SharedArrayBuffer. An `input()` call blocks on its first i32
//! until the page has stored the reply: byte length in the second i32, UTF-8
//! bytes from offset 8.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use js_sys::{
    Array, Atomics, Date, Function, Int32Array, Object, Promise, SharedArrayBuffer, Uint8Array,
};
use pyde_js::{field, flag, message, set, text};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

const PYODIDE_URL: &str = "https://cdn.jsdelivr.net/pyodide/v314.0.7/full/";

#[wasm_bindgen(raw_module = "https://cdn.jsdelivr.net/pyodide/v314.0.7/full/pyodide.mjs")]
extern "C" {
    type Pyodide;
    type PyodideFs;

    #[wasm_bindgen(catch, js_name = loadPyodide)]
    async fn load_pyodide(options: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch, js_name = runPython)]
    fn run_python(this: &Pyodide, code: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, js_name = setStdout)]
    fn set_stdout(this: &Pyodide, options: &JsValue);

    #[wasm_bindgen(method, js_name = setStderr)]
    fn set_stderr(this: &Pyodide, options: &JsValue);

    #[wasm_bindgen(method, js_name = setStdin)]
    fn set_stdin(this: &Pyodide, options: &JsValue);

    #[wasm_bindgen(method, catch, js_name = loadPackagesFromImports)]
    fn load_packages_from_imports(
        this: &Pyodide,
        code: &str,
        options: &JsValue,
    ) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, getter, js_name = FS)]
    fn fs(this: &Pyodide) -> PyodideFs;

    #[wasm_bindgen(method, catch, js_name = writeFile)]
    fn write_file(this: &PyodideFs, path: &str, data: &str) -> Result<(), JsValue>;
}

/// Runs each program with fresh globals, as a script named after the open file.
/// Traceback frames from before the user's file are Pyodide internals, so they
/// are dropped.
const RUNNER: &str = include_str!("../python/runner.py");

/// Finds the installed packages a program imports and gathers their `.pyi` and
/// `.py` files for the type checker. A `.py` next to a `.pyi` of the same name is
/// left out, as the checker would prefer the stub anyway.
const PACKAGES: &str = include_str!("../python/packages.py");

/// Without cross-origin isolation there is no SharedArrayBuffer to block on.
const NO_INPUT: &str = include_str!("../python/no_input.py");

/// A tight print loop can post far faster than the page can handle. Lines go out
/// straight away until a window's budget is spent, then wait for the next line
/// after the window ends. Timers can't help, because they don't fire while Python
/// is running.
const OUTPUT_WINDOW_MS: f64 = 50.0;
/// Lines let through straight away in each window.
const OUTPUT_WINDOW_LINES: usize = 100;

#[derive(Default)]
struct Output {
    held: Vec<(String, bool)>,
    window_start: f64,
    window_lines: usize,
    /// Lets held lines schedule their own delivery, for programs that stop
    /// printing while they wait on something async.
    owner: Weak<RefCell<State>>,
    timer_pending: bool,
}

impl Output {
    fn line(&mut self, text: String, is_error: bool) {
        self.held.push((text, is_error));
        let now = Date::now();
        if now - self.window_start >= OUTPUT_WINDOW_MS {
            self.window_start = now;
            self.window_lines = 0;
        }
        if self.window_lines < OUTPUT_WINDOW_LINES {
            self.send_held();
        } else {
            self.schedule_flush();
        }
    }

    fn schedule_flush(&mut self) {
        if self.timer_pending || self.held.is_empty() {
            return;
        }
        let owner = self.owner.clone();
        let flush = Closure::once_into_js(move || {
            let Some(state) = owner.upgrade() else { return };
            let Ok(mut state) = state.try_borrow_mut() else {
                return;
            };
            let output = &mut state.output;
            output.timer_pending = false;
            output.window_start = Date::now();
            output.window_lines = 0;
            output.send_held();
        });
        let scheduled = scope().set_timeout_with_callback_and_timeout_and_arguments_0(
            flush.unchecked_ref(),
            OUTPUT_WINDOW_MS as i32,
        );
        self.timer_pending = scheduled.is_ok();
    }

    fn send_held(&mut self) {
        if self.held.is_empty() {
            return;
        }
        self.window_lines += self.held.len();
        let lines = Array::new();
        for (text, is_error) in self.held.drain(..) {
            let line = Object::new();
            set(&line, "text", &JsValue::from_str(&text));
            set(&line, "isError", &JsValue::from_bool(is_error));
            lines.push(&line);
        }
        post("output", &[("lines", lines.into())]);
    }
}

/// Splits one output stream into lines. Splitting bytes on `\n` can't cut a
/// multi-byte character in half.
#[derive(Default)]
struct Stream {
    partial: Vec<u8>,
}

impl Stream {
    fn write(&mut self, bytes: &[u8], is_error: bool, output: &mut Output) {
        for part in bytes.split_inclusive(|&byte| byte == b'\n') {
            self.partial.extend_from_slice(part);
            if self.partial.last() == Some(&b'\n') {
                self.partial.pop();
                output.line(self.take(), is_error);
            }
        }
    }

    fn take(&mut self) -> String {
        let text = String::from_utf8_lossy(&self.partial).into_owned();
        self.partial.clear();
        text
    }
}

#[derive(Default)]
struct State {
    output: Output,
    stdout: Stream,
    stderr: Stream,
    stdin: Option<SharedArrayBuffer>,
}

impl State {
    fn finish_run(&mut self) {
        if !self.stdout.partial.is_empty() {
            let text = self.stdout.take();
            self.output.line(text, false);
        }
        if !self.stderr.partial.is_empty() {
            let text = self.stderr.take();
            self.output.line(text, true);
        }
        self.output.send_held();
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    spawn_local(async {
        if let Err(error) = serve().await {
            post("failed", &[("message", describe(&error).into())]);
        }
    });
}

/// Loads Pyodide, then answers messages from the page until the worker is stopped.
async fn serve() -> Result<(), JsValue> {
    let options = Object::new();
    set(&options, "indexURL", &JsValue::from_str(PYODIDE_URL));
    let pyodide: Pyodide = load_pyodide(options.into()).await?.unchecked_into();

    let state = Rc::new(RefCell::new(State::default()));
    state.borrow_mut().output.owner = Rc::downgrade(&state);
    pyodide.set_stdout(&writer(&state, false));
    pyodide.set_stderr(&writer(&state, true));
    pyodide.set_stdin(&reader(&state));
    let run: Function = pyodide.run_python(RUNNER)?.unchecked_into();
    if !flag(&js_sys::global(), "crossOriginIsolated") {
        pyodide.run_python(NO_INPUT)?;
    }

    let packages: Array = pyodide.run_python(PACKAGES)?.unchecked_into();
    let (wanted, collect): (Function, Function) = (
        packages.get(0).unchecked_into(),
        packages.get(1).unchecked_into(),
    );

    let pyodide = Rc::new(pyodide);
    let on_message = Closure::<dyn Fn(MessageEvent)>::new(move |e: MessageEvent| {
        let (pyodide, run, state) = (pyodide.clone(), run.clone(), state.clone());
        let (wanted, collect) = (wanted.clone(), collect.clone());
        spawn_local(async move {
            let data = e.data();
            match text(&data, "type").as_str() {
                "prefetch" => {
                    prefetch(&pyodide, &wanted, &collect, &text(&data, "code")).await;
                }
                "run" => {
                    let (name, code) = (text(&data, "name"), text(&data, "code"));
                    state.borrow_mut().stdin = field(&data, "stdin").dyn_into().ok();
                    if let Err(error) = run_program(&pyodide, &run, &state, &name, &code).await {
                        state.borrow_mut().output.line(describe(&error), true);
                    }
                    state.borrow_mut().finish_run();
                    post("done", &[]);
                }
                _ => {}
            }
        });
    });
    scope().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();

    post("ready", &[]);
    Ok(())
}

/// Writes `code` to a file named `name` and runs it, loading what it imports first.
async fn run_program(
    pyodide: &Pyodide,
    run: &Function,
    state: &Rc<RefCell<State>>,
    name: &str,
    code: &str,
) -> Result<(), JsValue> {
    // The working directory is /home/pyodide, so the program can open its own
    // file by name.
    pyodide.fs().write_file(name, code)?;

    let options = Object::new();
    let ignore = Closure::<dyn Fn(JsValue)>::new(|_| {});
    let state_for_errors = state.clone();
    let on_error = Closure::<dyn Fn(String)>::new(move |text: String| {
        state_for_errors.borrow_mut().output.line(text, true);
    });
    set(&options, "messageCallback", ignore.as_ref());
    set(&options, "errorCallback", on_error.as_ref());
    JsFuture::from(pyodide.load_packages_from_imports(code, &options)?).await?;

    let finished = run.call2(&JsValue::NULL, &code.into(), &name.into())?;
    JsFuture::from(Promise::resolve(&finished)).await?;
    Ok(())
}

/// Options for `setStdout` and `setStderr`.
fn writer(state: &Rc<RefCell<State>>, is_error: bool) -> JsValue {
    let state = state.clone();
    let write = Closure::<dyn Fn(Uint8Array) -> u32>::new(move |bytes: Uint8Array| {
        let state = &mut *state.borrow_mut();
        let stream = if is_error {
            &mut state.stderr
        } else {
            &mut state.stdout
        };
        stream.write(&bytes.to_vec(), is_error, &mut state.output);
        bytes.length()
    });
    let options = Object::new();
    set(&options, "write", write.as_ref());
    write.forget();
    options.into()
}

/// Options for `setStdin`. Python's `input()` writes its prompt to stdout without
/// a newline, so whatever partial line is waiting there is the prompt.
fn reader(state: &Rc<RefCell<State>>) -> JsValue {
    let state = state.clone();
    let read = Closure::<dyn Fn() -> Option<String>>::new(move || {
        let state = &mut *state.borrow_mut();
        let stdin = state.stdin.clone()?;
        state.output.send_held();
        post("input", &[("prompt", state.stdout.take().into())]);
        Some(read_line(&stdin))
    });
    let options = Object::new();
    set(&options, "stdin", read.as_ref());
    read.forget();
    options.into()
}

/// Blocks until the page has put a reply to `input()` in `stdin`.
fn read_line(stdin: &SharedArrayBuffer) -> String {
    let flags = Int32Array::new(stdin);
    let _ = Atomics::wait(&flags, 0, 0);
    let length = flags.get_index(1) as u32;
    let bytes = Uint8Array::new_with_byte_offset_and_length(stdin, 8, length).to_vec();
    let _ = Atomics::store(&flags, 0, 0);
    let mut line = String::from_utf8_lossy(&bytes).into_owned();
    line.push('\n');
    line
}

fn post(kind: &str, fields: &[(&str, JsValue)]) {
    let _ = scope().post_message(&message(kind, fields));
}

fn scope() -> DedicatedWorkerGlobalScope {
    js_sys::global().unchecked_into()
}

fn describe(error: &JsValue) -> String {
    match error.dyn_ref::<js_sys::Error>() {
        Some(error) => String::from(error.to_string()),
        None => format!("{error:?}"),
    }
}

/// Loads the packages `code` imports, then reports their files. Only packages not
/// reported before are touched, so typing around an unchanged import is free.
async fn prefetch(pyodide: &Pyodide, wanted: &Function, collect: &Function, code: &str) {
    let names = wanted
        .call1(&JsValue::NULL, &code.into())
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    let list: Vec<String> = serde_json::from_str(&names).unwrap_or_default();
    if list.is_empty() {
        return;
    }
    post("prefetching", &[("names", list.join(", ").into())]);

    let loaded = async {
        let options = Object::new();
        let quiet = Closure::<dyn Fn(JsValue)>::new(|_| {});
        set(&options, "messageCallback", quiet.as_ref());
        set(&options, "errorCallback", quiet.as_ref());
        JsFuture::from(pyodide.load_packages_from_imports(code, &options)?).await?;
        collect.call1(&JsValue::NULL, &names.as_str().into())
    }
    .await;
    match loaded {
        Ok(json) => post("prefetched", &[("json", json), ("failed", JsValue::FALSE)]),
        Err(_) => post(
            "prefetched",
            &[("json", "[]".into()), ("failed", JsValue::TRUE)],
        ),
    }
}
