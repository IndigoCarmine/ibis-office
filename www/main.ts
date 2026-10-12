// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// Boots the wasm editor. `pkg/` is produced by scripts/build-web.sh.
import init, { run } from './pkg/ibis_office_app.js';

/** A file chosen by the user or loaded at startup. */
interface PickedFile {
  name: string;
  bytes: Uint8Array;
}

declare global {
  interface Window {
    /** Called from Rust for ファイル → 開く. */
    ibisOfficePickFile: () => Promise<PickedFile | null>;
  }
}

/** Show the browser's file picker. Resolves to `null` when cancelled. */
function pickFile(): Promise<PickedFile | null> {
  return new Promise((resolve, reject) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.pptx,.xlsx,.docx,application/vnd.openxmlformats-officedocument.presentationml.presentation,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/vnd.openxmlformats-officedocument.wordprocessingml.document';
    input.addEventListener('change', () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      file.arrayBuffer().then((buffer) => resolve({ name: file.name, bytes: new Uint8Array(buffer) }), reject);
    });
    input.addEventListener('cancel', () => resolve(null));
    input.click();
  });
}

/**
 * The file the playground page picked (`?local=1`). It is handed over through
 * the browser's own IndexedDB, never a server, and deleted once read.
 */
function localFile(): Promise<PickedFile | null> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open('ibisoffice-playground', 1);
    open.onupgradeneeded = () => open.result.createObjectStore('files');
    open.onerror = () => reject(open.error);
    open.onsuccess = () => {
      const tx = open.result.transaction('files', 'readwrite');
      const files = tx.objectStore('files');
      const get = files.get('picked');
      get.onsuccess = () => {
        files.delete('picked');
        resolve((get.result as PickedFile | undefined) ?? null);
      };
      get.onerror = () => reject(get.error);
    };
  });
}

/** The document named by `?open=<url>`, fetched from this site, or the one the playground page picked. */
async function initialFile(): Promise<PickedFile | null> {
  if (new URLSearchParams(location.search).has('local')) {
    return localFile();
  }
  const url = new URLSearchParams(location.search).get('open');
  if (!url) {
    return null;
  }
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url}: HTTP ${response.status}`);
  }
  const path = new URL(url, location.href).pathname;
  const name = decodeURIComponent(path.slice(path.lastIndexOf('/') + 1));
  return { name, bytes: new Uint8Array(await response.arrayBuffer()) };
}

const status = document.getElementById('status');
const statusText = document.getElementById('status-text');
const progress = document.getElementById('progress');
const progressFill = progress?.firstElementChild as HTMLElement | null;

/**
 * The wasm's uncompressed size, written next to it by build-web.sh, or 0.
 */
async function wasmSize(): Promise<number> {
  try {
    const response = await fetch(new URL('./pkg/ibis_office_app_bg.wasm.size', import.meta.url));
    return response.ok ? Number((await response.text()).trim()) || 0 : 0;
  } catch {
    return 0;
  }
}

/**
 * Download the app's wasm, filling the progress bar against its size: the
 * size build-web.sh wrote (the bytes the browser hands over are the
 * uncompressed ones, while a server that compresses, as GitHub Pages does,
 * counts the compressed ones in Content-Length), else Content-Length when the
 * transfer is not compressed. The bar stays hidden when neither is known.
 */
async function fetchWasm(): Promise<Response> {
  const url = new URL('./pkg/ibis_office_app_bg.wasm', import.meta.url);
  const [response, size] = await Promise.all([fetch(url), wasmSize()]);
  if (!response.ok) {
    throw new Error(`${url.pathname}: HTTP ${response.status}`);
  }
  const compressed = response.headers.has('Content-Encoding');
  const total = size > 0 ? size : compressed ? 0 : Number(response.headers.get('Content-Length'));
  if (!response.body || !(total > 0)) {
    return response;
  }
  if (progress) {
    progress.style.display = 'block';
  }
  const reader = response.body.getReader();
  const chunks: BlobPart[] = [];
  let loaded = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    chunks.push(value);
    loaded += value.length;
    if (progressFill) {
      progressFill.style.width = `${Math.min(100, (loaded / total) * 100)}%`;
    }
  }
  // Downloaded; compiling and starting the app can take a few more seconds.
  // Fill the bar right away (the download may end short of Content-Length
  // by a few bytes, and the bar's transition would lag behind).
  if (progressFill) {
    progressFill.style.transition = 'none';
    progressFill.style.width = '100%';
  }
  progress?.classList.add('starting');
  if (statusText) {
    statusText.textContent = ja ? 'ibisOffice を起動しています…' : 'Starting ibisOffice…';
  }
  // Let the browser paint the full bar and the new text before the page
  // gets busy compiling and starting the app.
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  return new Response(new Blob(chunks), { headers: { 'Content-Type': 'application/wasm' } });
}

/** Japanese when the viewer chose it on the playground page, or when the browser prefers it. */
function prefersJapanese(): boolean {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem('ibisoffice-lang');
  } catch {
    // Storage may be blocked; fall back to the browser's language.
  }
  return (saved ?? navigator.language).toLowerCase().startsWith('ja');
}

const ja = prefersJapanese();

function showStatus(text: string): void {
  if (statusText) {
    statusText.textContent = text;
  }
  if (progress) {
    progress.style.display = 'none';
  }
}

window.ibisOfficePickFile = pickFile;

if (!('gpu' in navigator)) {
  showStatus(ja
    ? 'このブラウザは WebGPU に対応していません。最新の Chrome、Edge、Safari をお使いください。'
    : 'This browser does not support WebGPU. Please use the latest Chrome, Edge or Safari.');
} else {
  try {
    const [, file] = await Promise.all([init({ module_or_path: fetchWasm() }), initialFile()]);
    // Icons load from <base>/assets/icons/*.svg, next to this page (the site
    // may live under a path, as on GitHub Pages). The base must be absolute:
    // the icon loader fetches with reqwest, which rejects relative URLs.
    const base = new URL('.', location.href).href.replace(/\/$/, '');
    run(base, file?.name, file?.bytes);
    status?.remove();
  } catch (error) {
    console.error(error);
    showStatus(ja ? `起動できませんでした: ${error}` : `Could not start: ${error}`);
  }
}
