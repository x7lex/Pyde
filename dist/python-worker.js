// Runs Pyodide off the main thread, so the page stays responsive, output
// streams while a program runs, and Stop can simply terminate this worker.
//
// Receives: { name, code }
// Sends:    { type: "ready" }
//           { type: "output", lines: [{ text, isError }] }
//           { type: "done" }
//           { type: "failed", message }

const PYODIDE_URL = "https://cdn.jsdelivr.net/pyodide/v314.0.7/full/";

// Runs each program with fresh globals, as a script named after the open file.
// Traceback frames from before the user's file are Pyodide internals, so they
// are dropped.
const RUNNER = `
import builtins, sys, traceback
from pyodide.code import eval_code_async

def _no_input(prompt=""):
    raise RuntimeError("input() is not supported in Pyde yet")

builtins.input = _no_input

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
`;

// A tight print loop can post far faster than the page can handle. Lines go out
// straight away until a window's budget is spent, then wait for the next line
// after the window ends. Timers can't help, because they don't fire while Python
// is running.
const OUTPUT_WINDOW_MS = 50;
const OUTPUT_WINDOW_LINES = 100;
let heldLines = [];
let windowStart = 0;
let windowLines = 0;

function sendHeldLines() {
  windowLines += heldLines.length;
  if (heldLines.length) postMessage({ type: "output", lines: heldLines });
  heldLines = [];
}

function output(text, isError) {
  heldLines.push({ text, isError });
  const now = performance.now();
  if (now - windowStart >= OUTPUT_WINDOW_MS) {
    windowStart = now;
    windowLines = 0;
  }
  if (windowLines < OUTPUT_WINDOW_LINES) sendHeldLines();
}

// Sends complete lines as they are written. `flush` sends any partial line left
// at the end of a run, which Pyodide's built-in line batching would hold back.
function lineWriter(isError) {
  const decoder = new TextDecoder();
  let partial = "";
  return {
    write(bytes) {
      const lines = (partial + decoder.decode(bytes, { stream: true })).split("\n");
      partial = lines.pop();
      for (const line of lines) output(line, isError);
      return bytes.length;
    },
    flush() {
      if (partial) output(partial, isError);
      partial = "";
    },
  };
}

const stdout = lineWriter(false);
const stderr = lineWriter(true);

const ready = (async () => {
  const { loadPyodide } = await import(`${PYODIDE_URL}pyodide.mjs`);
  const pyodide = await loadPyodide({ indexURL: PYODIDE_URL });
  pyodide.setStdout({ write: stdout.write });
  pyodide.setStderr({ write: stderr.write });
  return { pyodide, run: pyodide.runPython(RUNNER) };
})();

ready.then(
  () => postMessage({ type: "ready" }),
  (error) => postMessage({ type: "failed", message: String(error) }),
);

onmessage = async ({ data: { name, code } }) => {
  const python = await ready.catch(() => null);
  if (!python) return; // Already reported as "failed".
  const { pyodide, run } = python;
  try {
    // The working directory is /home/pyodide, so the program can open its own
    // file by name.
    pyodide.FS.writeFile(name, code);
    await pyodide.loadPackagesFromImports(code, {
      messageCallback: () => {},
      errorCallback: (text) => output(text, true),
    });
    await run(code, name);
  } catch (error) {
    output(String(error), true);
  }
  stdout.flush();
  stderr.flush();
  sendHeldLines();
  postMessage({ type: "done" });
};
