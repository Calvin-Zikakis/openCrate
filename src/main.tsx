import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { readLibraryStatus, tauriArtwork, TauriLibraryApi, tauriInvoke } from "./library/tauri-api";
import type { LibraryApi } from "./library/types";
import { WorkerLibraryApi } from "./library/worker-api";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("The application root is missing.");
const renderer = createRoot(root);
/** The Rust library when the desktop app has a database (OPENCRATE_DEMO), else the in-memory demo. */
async function createApi(): Promise<LibraryApi> {
  const invoke = tauriInvoke();
  const status = invoke ? await readLibraryStatus(invoke) : null;
  if (invoke && status?.mode) return new TauriLibraryApi(invoke, status.mode, tauriArtwork());
  const demo = new WorkerLibraryApi();
  if (status?.error) demo.startupError = status.error;
  return demo;
}

try {
  const api = await createApi();
  renderer.render(<StrictMode><App api={api} /></StrictMode>);
  if (import.meta.hot) import.meta.hot.dispose(() => api.dispose());
} catch (error) {
  renderer.render(<div className="feature-empty" role="alert"><h1>Could not start the demo library</h1><p>{error instanceof Error ? error.message : "The browser could not create the library worker."}</p><p>Reload the app to retry. No music files have been accessed.</p></div>);
}
