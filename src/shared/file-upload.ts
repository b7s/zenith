import { setIcon } from "./icon";

export interface FileUploadFile {
  id: string;
  name: string;
  size: number;
  type: string;
  dataUrl?: string;
  file?: File;
  error?: string | null;
}

export interface FileUploadOptions {
  accept?: string;
  maxSize?: number;
  multiple?: boolean;
  prompt?: string;
  hint?: string;
  initialFiles?: FileUploadFile[];
  onChange?: (files: FileUploadFile[]) => void;
  validate?: (file: FileUploadFile) => string | null;
}

export interface FileUploadHandle {
  element: HTMLElement;
  getFiles(): FileUploadFile[];
  setFiles(files: FileUploadFile[]): void;
  clear(): void;
  destroy(): void;
}

let _idCounter = 0;
function uid(): string {
  return "fu-" + (++_idCounter).toString(36);
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

const FILE_EXT_REGEX = /\.([a-z0-9]+)$/i;

function matchAccept(name: string, type: string, accept: string): boolean {
  const patterns = accept.split(",").map((p) => p.trim().toLowerCase());
  for (const pat of patterns) {
    if (pat === "*" || pat === "*/*") return true;
    if (pat.startsWith(".")) {
      const ext = FILE_EXT_REGEX.exec(name);
      if (ext && "." + ext[1].toLowerCase() === pat) return true;
    } else if (pat.endsWith("/*")) {
      const category = pat.slice(0, -2);
      if (type.startsWith(category + "/")) return true;
    } else if (pat === type.toLowerCase()) {
      return true;
    }
  }
  return false;
}

function previewFile(file: File): Promise<string | undefined> {
  if (file.type.startsWith("image/")) {
    return new Promise((resolve) => {
      const r = new FileReader();
      r.onload = () => resolve(r.result as string);
      r.onerror = () => resolve(undefined);
      r.readAsDataURL(file);
    });
  }
  return Promise.resolve(undefined);
}

export function mountFileUpload(
  parent: HTMLElement,
  opts?: FileUploadOptions,
): FileUploadHandle {
  const {
    accept = "*/*",
    maxSize = 0,
    multiple = false,
    prompt = "Drop files here or click to browse",
    hint = "",
    initialFiles = [],
    onChange,
    validate,
  } = opts ?? {};

  let files: FileUploadFile[] = initialFiles.map((f) => ({ ...f }));

  const root = document.createElement("div");
  root.className = "zen-file-upload";
  if (!multiple) root.classList.add("zen-file-upload--single");

  const zone = document.createElement("div");
  zone.className = "zen-file-upload__zone";
  zone.tabIndex = 0;
  zone.setAttribute("role", "button");
  zone.setAttribute("aria-label", prompt);

  const input = document.createElement("input");
  input.type = "file";
  input.multiple = multiple;
  input.accept = accept !== "*/*" ? accept : "";
  input.style.display = "none";

  const iconEl = document.createElement("span");
  iconEl.className = "zen-file-upload__icon";
  setIcon(iconEl, "upload", { size: 16 });
  zone.append(iconEl);

  const promptEl = document.createElement("span");
  promptEl.className = "zen-file-upload__prompt";
  promptEl.textContent = prompt;
  zone.append(promptEl);

  if (hint) {
    const hintEl = document.createElement("span");
    hintEl.className = "zen-file-upload__hint";
    hintEl.textContent = hint;
    zone.append(hintEl);
  }

  const errorEl = document.createElement("span");
  errorEl.className = "zen-file-upload__error";
  zone.append(errorEl);

  root.append(zone, input);

  const filesEl = document.createElement("div");
  filesEl.className = "zen-file-upload__files";
  root.append(filesEl);

  function showError(msg: string): void {
    errorEl.textContent = msg;
    errorEl.classList.add("is-visible");
    zone.classList.add("is-error");
  }

  function clearError(): void {
    errorEl.textContent = "";
    errorEl.classList.remove("is-visible");
    zone.classList.remove("is-error");
  }

  function renderFileRow(f: FileUploadFile): HTMLElement {
    const row = document.createElement("div");
    row.className = "zen-file-upload__file";

    const ic = document.createElement("span");
    ic.className = "zen-file-upload__file-icon";
    if (f.dataUrl && f.type.startsWith("image/")) {
      const img = document.createElement("img");
      img.src = f.dataUrl;
      img.alt = f.name;
      ic.append(img);
    } else {
      ic.textContent = f.name.charAt(0).toUpperCase();
    }

    const info = document.createElement("div");
    info.className = "zen-file-upload__file-info";

    const nameEl = document.createElement("span");
    nameEl.className = "zen-file-upload__file-name";
    nameEl.textContent = f.name;
    if (f.error) nameEl.style.color = "var(--danger)";

    const sizeEl = document.createElement("span");
    sizeEl.className = "zen-file-upload__file-size";
    sizeEl.textContent = f.error ? f.error : formatSize(f.size);

    info.append(nameEl, sizeEl);

    const rm = document.createElement("button");
    rm.type = "button";
    rm.className = "zen-icon-button zen-file-upload__file-remove";
    rm.setAttribute("aria-label", "Remove " + f.name);
    setIcon(rm, "x", { size: 12 });

    row.append(ic, info, rm);
    return row;
  }

  function addFiles(raw: FileList | File[]): void {
    clearError();
    const list = Array.from(raw);

    if (!multiple && files.length > 0 && list.length > 0) {
      files = [];
    }

    const accepted: FileUploadFile[] = [];
    for (const file of list) {
      if (accept !== "*/*" && !matchAccept(file.name, file.type, accept)) {
        showError(`"${file.name}" — file type not accepted`);
        continue;
      }
      if (maxSize > 0 && file.size > maxSize) {
        showError(`"${file.name}" exceeds ${formatSize(maxSize)} limit`);
        continue;
      }
      const entry: FileUploadFile = {
        id: uid(),
        name: file.name,
        size: file.size,
        type: file.type,
        file,
      };
      if (validate) {
        const err = validate(entry);
        if (err) {
          entry.error = err;
        }
      }
      accepted.push(entry);
    }

    if (accepted.length === 0) return;

    const promises = accepted.map(async (entry) => {
      if (entry.file && !entry.dataUrl) {
        entry.dataUrl = await previewFile(entry.file);
      }
      return entry;
    });

    Promise.all(promises).then((resolved) => {
      files.push(...resolved);
      emitChange();
    });
  }

  function emitChange(): void {
    filesEl.innerHTML = "";
    for (let i = 0; i < files.length; i++) {
      const row = renderFileRow(files[i]);
      const rm = row.querySelector<HTMLButtonElement>(".zen-file-upload__file-remove");
      if (rm) {
        const idx = i;
        rm.addEventListener("click", () => {
          files.splice(idx, 1);
          emitChange();
        });
      }
      filesEl.append(row);
    }
    if (onChange) onChange(files);
  }

  zone.addEventListener("click", () => input.click());

  input.addEventListener("change", () => {
    if (input.files && input.files.length > 0) {
      addFiles(input.files);
      input.value = "";
    }
  });

  zone.addEventListener("dragenter", (e) => {
    e.preventDefault();
    e.stopPropagation();
    zone.classList.add("is-drag-over");
    clearError();
  });
  zone.addEventListener("dragover", (e) => {
    e.preventDefault();
    e.stopPropagation();
    zone.classList.add("is-drag-over");
  });
  zone.addEventListener("dragleave", (e) => {
    e.preventDefault();
    e.stopPropagation();
    zone.classList.remove("is-drag-over");
  });
  zone.addEventListener("drop", (e) => {
    e.preventDefault();
    e.stopPropagation();
    zone.classList.remove("is-drag-over");
    if (e.dataTransfer?.files && e.dataTransfer.files.length > 0) {
      addFiles(e.dataTransfer.files);
    }
  });

  if (initialFiles.length > 0) emitChange();

  parent.append(root);

  return {
    element: root,
    getFiles: () => [...files],
    setFiles: (newFiles) => {
      files = newFiles.map((f) => ({ ...f }));
      emitChange();
    },
    clear: () => {
      files = [];
      emitChange();
    },
    destroy: () => {
      root.remove();
    },
  };
}

export async function readFilesAsDataUrls(
  files: FileUploadFile[],
): Promise<FileUploadFile[]> {
  const promises = files.map(async (f) => {
    if (f.file && !f.dataUrl && f.file.type.startsWith("image/")) {
      f.dataUrl = await previewFile(f.file);
    }
    return f;
  });
  return Promise.all(promises);
}
