//! Runs Pyodide off the main thread, so the page stays responsive, output
//! streams while a program runs, and Stop can simply terminate this worker.
//!
//! Receives: `{ name, code, stdin? }`, once per run.
//! Sends:    `{ type: "ready" }`
//!           `{ type: "output", lines: [{ text, isError }] }`
//!           `{ type: "input", prompt }`
//!           `{ type: "done" }`
//!           `{ type: "failed", message }`
//!
//! `stdin` is a SharedArrayBuffer. An `input()` call blocks on its first i32
//! until the page has stored the reply: byte length in the second i32, UTF-8
//! bytes from offset 8.

use std::{cell::RefCell, rc::Rc};

use js_sys::{
    Array, Atomics, Date, Function, Int32Array, Object, Promise, Reflect, SharedArrayBuffer,
    Uint8Array,
};
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
const RUNNER: &str = r#"
import sys, traceback
from pyodide.code import eval_code_async

async def run(code, path):
    try:
        await eval_code_async(code, {"__name__": "__main__", "__file__": path}, filename=path)
    except SystemExit as e:
        if e.code not in (None, 0):
            print(e.code, file=sys.stderr)
    except BaseException as e:
        tb = e.__traceback__
        while tb and tb.tb_frame.f_code.co_filename != path:
            tb = tb.tb_next
        traceback.print_exception(type(e), e, tb)
    finally:
        sys.stdout.flush()
        sys.stderr.flush()

run
"#;

/// Without cross-origin isolation there is no SharedArrayBuffer to block on.
const NO_INPUT: &str = r#"
import builtins

def _no_input(prompt=""):
    raise RuntimeError("input() needs the page to be served with COOP and COEP headers")

builtins.input = _no_input
"#;

/// A tight print loop can post far faster than the page can handle. Lines go out
/// straight away until a window's budget is spent, then wait for the next line
/// after the window ends. Timers can't help, because they don't fire while Python
/// is running.
const OUTPUT_WINDOW_MS: f64 = 50.0;
const OUTPUT_WINDOW_LINES: usize = 100;

#[derive(Default)]
struct Output {
    held: Vec<(String, bool)>,
    window_start: f64,
    window_lines: usize,
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
        }
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

async fn serve() -> Result<(), JsValue> {
    let options = Object::new();
    set(&options, "indexURL", &JsValue::from_str(PYODIDE_URL));
    let pyodide: Pyodide = load_pyodide(options.into()).await?.unchecked_into();

    let state = Rc::new(RefCell::new(State::default()));
    pyodide.set_stdout(&writer(&state, false));
    pyodide.set_stderr(&writer(&state, true));
    pyodide.set_stdin(&reader(&state));
    let run: Function = pyodide.run_python(RUNNER)?.unchecked_into();
    if !Reflect::get(&scope(), &"crossOriginIsolated".into())?.is_truthy() {
        pyodide.run_python(NO_INPUT)?;
    }

    let pyodide = Rc::new(pyodide);
    let on_message = Closure::<dyn Fn(MessageEvent)>::new(move |e: MessageEvent| {
        let (pyodide, run, state) = (pyodide.clone(), run.clone(), state.clone());
        spawn_local(async move {
            let data = e.data();
            let text = |key: &str| field(&data, key).as_string().unwrap_or_default();
            let (name, code) = (text("name"), text("code"));
            state.borrow_mut().stdin = field(&data, "stdin").dyn_into().ok();
            if let Err(error) = run_program(&pyodide, &run, &state, &name, &code).await {
                state.borrow_mut().output.line(describe(&error), true);
            }
            state.borrow_mut().finish_run();
            post("done", &[]);
        });
    });
    scope().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();

    post("ready", &[]);
    Ok(())
}

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
    let message = Object::new();
    set(&message, "type", &JsValue::from_str(kind));
    for (key, value) in fields {
        set(&message, key, value);
    }
    let _ = scope().post_message(&message);
}

fn scope() -> DedicatedWorkerGlobalScope {
    js_sys::global().unchecked_into()
}

fn field(object: &JsValue, key: &str) -> JsValue {
    Reflect::get(object, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

fn set(object: &Object, key: &str, value: &JsValue) {
    let _ = Reflect::set(object, &JsValue::from_str(key), value);
}

fn describe(error: &JsValue) -> String {
    match error.dyn_ref::<js_sys::Error>() {
        Some(error) => String::from(error.to_string()),
        None => format!("{error:?}"),
    }
}
