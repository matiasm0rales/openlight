// El updater falso: responde como `tauri-plugin-updater` y `tauri-plugin-process`.
//
// Se elige por URL con `?update=<escenario>` (por defecto no hay versión nueva):
// - `available`: hay una versión nueva y todo sale bien.
// - `hang`: la descarga se queda al 40 % hasta llamar a `finishDownload()`.
// - `offline`: no hay red ni para buscar.
// - `offline_download`: la red se corta en medio de la descarga.
// - `bad_signature`: el paquete no pasa la verificación de la firma.
// - `install_fails`: no se puede reemplazar la app (sin permiso de escritura).
// - `restart_fails`: se instala pero no se puede reiniciar.
import type { Channel } from "@tauri-apps/api/core";
import type { DownloadEvent } from "@tauri-apps/plugin-updater";

export const UPDATE_SCENARIOS = ["none", "available", "hang", "offline", "offline_download", "bad_signature", "install_fails", "restart_fails"] as const;
export type UpdateScenario = (typeof UPDATE_SCENARIOS)[number];

export const MOCK_VERSION = "0.1.0";
export const MOCK_NEXT_VERSION = "0.2.0";
export const MOCK_NOTES = "## Novedades\n\n- **Escenas** en el control manual.\n- Menos consumo con la pantalla quieta.\n- Se arregló la reconexión de los focos 3.5.";

const SIZE = 12_000_000;
const CHUNKS = 10;

export class MockUpdater {
  scenario: UpdateScenario;
  /** Lo que la UI pidió, en orden (para probar que los focos se restauran antes de instalar). */
  calls: string[] = [];
  restarted = false;
  private resume: (() => void) | null = null;

  constructor(
    scenario: string | null,
    private wait: (ms: number) => Promise<void>,
  ) {
    this.scenario = UPDATE_SCENARIOS.find((s) => s === scenario) ?? "none";
  }

  /** Termina una descarga detenida por el escenario `hang`. */
  finishDownload(): void {
    this.resume?.();
    this.resume = null;
  }

  async check() {
    this.calls.push("check");
    await this.wait(300);
    if (this.scenario === "offline") throw "error sending request for url (https://github.com/matiasm0rales/openlight/releases/latest/download/latest.json)";
    if (this.scenario === "none") return null;
    return { rid: 1, currentVersion: MOCK_VERSION, version: MOCK_NEXT_VERSION, body: MOCK_NOTES, rawJson: {} };
  }

  async download(onEvent: Channel<DownloadEvent>) {
    this.calls.push("download");
    const send = (e: DownloadEvent) => onEvent.onmessage(e);
    // La pausa existe desde el principio: se la puede soltar antes de que la descarga llegue.
    const gate = new Promise<void>((r) => (this.resume = r));
    send({ event: "Started", data: { contentLength: SIZE } });
    for (let i = 0; i < CHUNKS; i++) {
      await this.wait(120);
      if (i === 4 && this.scenario === "hang") await gate;
      if (i === 4 && this.scenario === "offline_download") throw "error decoding response body";
      send({ event: "Progress", data: { chunkLength: SIZE / CHUNKS } });
    }
    send({ event: "Finished" });
    if (this.scenario === "bad_signature") throw "The signature verification failed";
    return 2;
  }

  async install() {
    this.calls.push("install");
    await this.wait(400);
    if (this.scenario === "install_fails") throw "Permission denied (os error 13)";
    return null;
  }

  async restart() {
    this.calls.push("restart");
    await this.wait(200);
    if (this.scenario === "restart_fails") throw "failed to restart app";
    this.restarted = true;
    return null;
  }
}
