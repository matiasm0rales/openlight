import { describe, expect, it } from "vitest";
import { classifyUpdateError, plainNotes, updateFixes } from "./updates";

describe("errores del updater", () => {
  // Los textos son los que devuelven tauri-plugin-updater, reqwest, minisign y el sistema.
  const cases: [string, "check" | "download" | "install", string][] = [
    ["error sending request for url (https://github.com/x/latest.json)", "check", "network"],
    ["error sending request for url (https://github.com/x/latest.json)", "download", "network"],
    ["error decoding response body", "download", "network"],
    ["operation timed out", "download", "network"],
    // Las URL de descarga de GitHub traen una "signature" en la consulta: no es un error de firma.
    ["error sending request for url (https://release-assets.githubusercontent.com/a?X-Amz-Signature=ab&sig=cd)", "download", "network"],
    ["Could not fetch a valid release JSON from the remote", "check", "unavailable"],
    ["error decoding response body", "check", "unavailable"],
    ["missing field `version` at line 1 column 2", "check", "unavailable"],
    ["`Download request failed with status: 404 Not Found`", "download", "unavailable"],
    ["The signature verification failed", "download", "signature"],
    ["The signature was created with a different key than the one provided", "download", "signature"],
    ["Invalid encoding in minisign data", "download", "signature"],
    ["Unexpected signature algorithm", "download", "signature"],
    ["Invalid symbol 95, offset 9.", "download", "signature"],
    ["Invalid input length: 17", "download", "signature"],
    ["The signature abc could not be decoded, please check if it is a valid base64 string.", "download", "signature"],
    ["The update was signed for version 0.1.0 but the update endpoint announced version 0.2.0.", "download", "signature"],
    ["The update signature does not specify the version it was signed for", "download", "signature"],
    ["the platform `darwin-aarch64` was not found in the response `platforms` object", "check", "unsupported"],
    ['None of the fallback platforms `["linux-x86_64-deb", "linux-x86_64"]` were found in the response `platforms` object', "check", "unsupported"],
    ["Permission denied (os error 13)", "install", "permission"],
    ["Read-only file system (os error 30)", "install", "permission"],
    ["Access is denied. (os error 5)", "install", "permission"],
    ["temp directory is not on the same mount point as the app", "install", "permission"],
    ["Authentication failed or was cancelled", "install", "auth"],
    ["Failed to install package", "install", "install"],
    ["algo que nadie esperaba", "check", "unavailable"],
    ["algo que nadie esperaba", "download", "network"],
    ["algo que nadie esperaba", "install", "install"],
  ];

  for (const [message, step, kind] of cases) {
    it(`${step}: «${message.slice(0, 60)}» → ${kind}`, () => {
      expect(classifyUpdateError(message, step)).toBe(kind);
      expect(classifyUpdateError(new Error(message), step)).toBe(kind);
    });
  }

  it("cada error sugiere al menos una acción", () => {
    for (const kind of ["network", "unavailable", "signature", "unsupported", "permission", "auth", "install", "restart"] as const) {
      expect(updateFixes(kind).length).toBeGreaterThan(0);
    }
  });

  it("sin nada que reintentar se ofrece bajarla de la web; si falló el reinicio, reiniciar", () => {
    expect(updateFixes("unsupported")).toEqual(["web"]);
    expect(updateFixes("restart")).toEqual(["restart"]);
    expect(updateFixes("signature")).toEqual(["retry", "web"]);
    expect(updateFixes("network")).toEqual(["retry"]);
  });
});

describe("notas de la versión", () => {
  it("quita el markdown de lo que se muestra como texto", () => {
    const md = "## Novedades\r\n\r\n- **Escenas** nuevas, ver [la guía](https://example.com).\r\n* `tray`: __más__ rápido\r\n\r\n\r\n\r\nFin";
    expect(plainNotes(md)).toBe("Novedades\n\n• Escenas nuevas, ver la guía.\n• tray: más rápido\n\nFin");
  });

  it("no toca guiones bajos ni asteriscos sueltos", () => {
    expect(plainNotes("Usá OPENLIGHT_DATA_DIR y 2 * 3")).toBe("Usá OPENLIGHT_DATA_DIR y 2 * 3");
  });

  it("sin notas no muestra nada", () => {
    for (const nada of [undefined, null, "", "  \n  ", "<!-- comentario -->"]) expect(plainNotes(nada)).toBe("");
  });
});
