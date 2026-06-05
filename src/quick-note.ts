import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

const noteArea = document.getElementById("note-area") as HTMLTextAreaElement;
const saveBtn = document.getElementById("save-btn") as HTMLButtonElement;
const saveAsBtn = document.getElementById("save-as-btn") as HTMLButtonElement;
const newBtn = document.getElementById("new-btn") as HTMLButtonElement;
const openBtn = document.getElementById("open-btn") as HTMLButtonElement;
const pinBtn = document.getElementById("pin-btn") as HTMLButtonElement;
const filepathEl = document.getElementById("filepath") as HTMLElement;
const dirtyIndicator = document.getElementById("dirty-indicator") as HTMLElement;
const charCount = document.getElementById("char-count") as HTMLElement;
const toastEl = document.getElementById("toast") as HTMLElement;

const appWindow = getCurrentWindow();

let currentPath: string | null = null;
let savedText = "";
let pinned = true; // window starts always-on-top
let toastTimer: number | undefined;

// ---- Toast ----
function toast(msg: string): void {
  toastEl.textContent = msg;
  toastEl.classList.add("show");
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toastEl.classList.remove("show"), 2500);
}

// ---- Dirty state ----
function isDirty(): boolean {
  return noteArea.value !== savedText;
}

function updateStatus(): void {
  const dirty = isDirty();
  dirtyIndicator.textContent = dirty ? "● 未儲存" : "";
  saveBtn.disabled = !dirty || currentPath === null;
  saveBtn.classList.toggle("dirty", dirty);
  charCount.textContent = `${noteArea.value.length} 字元`;
  filepathEl.textContent = currentPath
    ? currentPath.split(/[\\/]/).pop() ?? currentPath
    : "（未儲存）";
  filepathEl.title = currentPath ?? "";
  document.title = `${dirty ? "● " : ""}${currentPath?.split(/[\\/]/).pop() ?? "Quick Note"}`;
}

noteArea.addEventListener("input", updateStatus);

// ---- Save ----
async function save(): Promise<void> {
  if (!currentPath) {
    await saveAs();
    return;
  }
  try {
    await invoke("write_md", { path: currentPath, content: noteArea.value });
    savedText = noteArea.value;
    updateStatus();
    toast("已儲存");
  } catch (e) {
    toast(`儲存失敗: ${String(e)}`);
  }
}

async function saveAs(): Promise<void> {
  const path = await saveDialog({
    filters: [
      { name: "文字檔", extensions: ["txt", "md"] },
      { name: "所有檔案", extensions: ["*"] },
    ],
    defaultPath: currentPath ?? "note.txt",
  });
  if (!path) return;
  currentPath = path;
  try {
    await invoke("write_md", { path: currentPath, content: noteArea.value });
    savedText = noteArea.value;
    updateStatus();
    toast("已另存新檔");
  } catch (e) {
    toast(`儲存失敗: ${String(e)}`);
  }
}

// ---- Open ----
async function openFile(): Promise<void> {
  if (isDirty()) {
    const confirmed = window.confirm("有未儲存的變更，確定要放棄並開啟新檔案嗎？");
    if (!confirmed) return;
  }
  const selected = await openDialog({
    multiple: false,
    filters: [
      { name: "文字檔", extensions: ["txt", "md"] },
      { name: "所有檔案", extensions: ["*"] },
    ],
  });
  if (typeof selected !== "string") return;
  try {
    const text = await invoke<string>("read_md", { path: selected });
    currentPath = selected;
    savedText = text;
    noteArea.value = text;
    updateStatus();
  } catch (e) {
    toast(`開啟失敗: ${String(e)}`);
  }
}

// ---- New ----
function newNote(): void {
  if (isDirty()) {
    const confirmed = window.confirm("有未儲存的變更，確定要放棄並新建筆記嗎？");
    if (!confirmed) return;
  }
  currentPath = null;
  savedText = "";
  noteArea.value = "";
  updateStatus();
  noteArea.focus();
}

// ---- Pin (always on top) ----
async function togglePin(): Promise<void> {
  pinned = !pinned;
  try {
    await invoke("set_always_on_top", { onTop: pinned });
    pinBtn.classList.toggle("pinned", pinned);
    pinBtn.title = pinned ? "取消釘選（解除最上層）" : "釘選視窗（固定最上層）";
    toast(pinned ? "視窗已釘選" : "視窗已取消釘選");
  } catch (e) {
    pinned = !pinned; // revert
    toast(`設定失敗: ${String(e)}`);
  }
}

// ---- Event listeners ----
saveBtn.addEventListener("click", () => void save());
saveAsBtn.addEventListener("click", () => void saveAs());
openBtn.addEventListener("click", () => void openFile());
newBtn.addEventListener("click", newNote);
pinBtn.addEventListener("click", () => void togglePin());

window.addEventListener("keydown", (ev) => {
  if (ev.ctrlKey && !ev.shiftKey && (ev.key === "s" || ev.key === "S")) {
    ev.preventDefault();
    void save();
  } else if (ev.ctrlKey && ev.shiftKey && (ev.key === "s" || ev.key === "S")) {
    ev.preventDefault();
    void saveAs();
  } else if (ev.ctrlKey && !ev.shiftKey && (ev.key === "n" || ev.key === "N")) {
    ev.preventDefault();
    newNote();
  } else if (ev.ctrlKey && !ev.shiftKey && (ev.key === "o" || ev.key === "O")) {
    ev.preventDefault();
    void openFile();
  }
});

// Close confirmation if dirty
appWindow.onCloseRequested((event) => {
  if (isDirty()) {
    const confirmed = window.confirm("有未儲存的變更，確定要關閉嗎？");
    if (!confirmed) {
      event.preventDefault();
    }
  }
});

// ---- Init ----
async function init(): Promise<void> {
  // Apply initial pinned state to window
  try {
    await invoke("set_always_on_top", { onTop: pinned });
    pinBtn.classList.toggle("pinned", pinned);
    pinBtn.title = pinned ? "取消釘選（解除最上層）" : "釘選視窗（固定最上層）";
  } catch {
    // ignore
  }
  updateStatus();
  noteArea.focus();
}

void init();
