// Single-file open and save. Uses the File System Access API where the browser
// has it (Chromium), so Save can write back to the opened file. Other browsers
// fall back to a file input and a download.

const PICKER_TYPES = [
  { description: "Python files", accept: { "text/x-python": [".py"] } },
];

// A cancelled picker resolves to null rather than rejecting.
async function cancellable(picker) {
  try {
    return await picker();
  } catch (error) {
    if (error.name === "AbortError") return null;
    throw error;
  }
}

function pickWithInput() {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.onchange = () => resolve(input.files[0] ?? null);
    input.oncancel = () => resolve(null);
    input.click();
  });
}

function download(name, text) {
  const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/// Resolves to `{ name, text, handle }`, or null if cancelled. `handle` is null
/// in browsers without the File System Access API.
export async function openFile() {
  let handle = null;
  let file;
  if (window.showOpenFilePicker) {
    const picked = await cancellable(() =>
      window.showOpenFilePicker({ types: PICKER_TYPES }),
    );
    if (!picked) return null;
    handle = picked[0];
    file = await handle.getFile();
  } else {
    file = await pickWithInput();
    if (!file) return null;
  }
  // Decode strictly: lossy decoding would corrupt a binary file on save.
  const bytes = await file.arrayBuffer();
  let text;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error(`${file.name} is not a UTF-8 text file`);
  }
  return { name: file.name, text, handle };
}

/// Writes `text` to `handle`, asking where to save if there is no handle yet.
/// Resolves to `{ name, handle }`, or null if cancelled.
export async function saveFile(handle, name, text) {
  if (!handle && !window.showSaveFilePicker) {
    download(name, text);
    return { name, handle: null };
  }
  const target =
    handle ??
    (await cancellable(() =>
      window.showSaveFilePicker({ suggestedName: name, types: PICKER_TYPES }),
    ));
  if (!target) return null;
  const writable = await target.createWritable();
  await writable.write(text);
  await writable.close();
  return { name: target.name, handle: target };
}
