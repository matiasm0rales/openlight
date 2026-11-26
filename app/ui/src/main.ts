import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { runtime } from "./lib/mode";

async function boot() {
  // Sin Tauri (navegador común, Playwright): se instala el backend falso.
  if (!("__TAURI_INTERNALS__" in window) || import.meta.env.VITE_MOCK === "1") {
    const { installMocks, isScenario } = await import("./lib/mocks/backend");
    runtime.mock = true;
    const params = new URLSearchParams(location.search);
    const scenario = params.get("scenario");
    const backend = installMocks({
      scenario: isScenario(scenario) ? scenario : null,
      fast: params.has("fast"),
      update: params.get("update"),
    });
    (window as unknown as { __openlightMock: unknown }).__openlightMock = backend;
  }
  mount(App, { target: document.getElementById("app")! });
}

void boot();
