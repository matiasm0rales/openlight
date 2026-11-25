import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { RELEASES_URL } from "../../public/js/downloads.js";
import { formatButton, mainButton, otherLinks, releaseFromApi } from "../../public/js/view.js";
import { TEXTS } from "../../public/js/i18n.js";

const json = JSON.parse(readFileSync(new URL("../fixtures/release-latest.json", import.meta.url), "utf8"));
const es = TEXTS.es;
const platform = (os, extra = {}) => ({ os, arch: "x64", distro: null, ...extra });

test("de la respuesta de la API salen la versión y los instaladores", () => {
  const release = releaseFromApi(json, "x64");
  assert.equal(release.version, "0.1.0");
  assert.equal(release.downloads.dmg.name, "OpenLight_0.1.0_universal.dmg");
});

test("una respuesta sin instaladores o con error no es una release", () => {
  assert.equal(releaseFromApi({ message: "Not Found" }, "x64"), null);
  assert.equal(releaseFromApi({ tag_name: "v0.1.0", assets: [] }, "x64"), null);
  assert.equal(releaseFromApi(null, "x64"), null);
});

test("con release, el botón grande baja el instalador del sistema detectado", () => {
  const button = mainButton(releaseFromApi(json), platform("macos"), es);
  assert.equal(button.label, "Descargar para macOS");
  assert.ok(button.href.endsWith("/OpenLight_0.1.0_universal.dmg"));
  assert.equal(button.status, "Versión 0.1.0. Archivo .dmg de 22 MB, gratis.");
});

test("sin release muestra «Próximamente» y manda a la página de releases", () => {
  for (const os of ["macos", "windows", "linux"]) {
    const button = mainButton(null, platform(os), es);
    assert.match(button.label, /^Próximamente para /);
    assert.equal(button.href, RELEASES_URL);
    assert.equal(button.status, es.soonStatus);
  }
});

test("mientras busca la release el botón ya sirve: va a la página de releases", () => {
  const button = mainButton(undefined, platform("windows"), es);
  assert.equal(button.label, "Descargar para Windows");
  assert.equal(button.href, RELEASES_URL);
  assert.equal(button.status, es.loading);
});

test("en el celular no ofrece un instalador", () => {
  const button = mainButton(releaseFromApi(json), platform("mobile"), es);
  assert.equal(button.label, es.allDownloads);
  assert.equal(button.href, RELEASES_URL);
  assert.equal(button.status, es.mobile);
  assert.equal(mainButton(null, platform("mobile"), es).label, es.soon);
});

test("si no hay instalador para el procesador lo dice y manda a todas las descargas", () => {
  const button = mainButton(releaseFromApi(json, "arm64"), platform("linux", { arch: "arm64" }), es);
  assert.equal(button.label, es.allDownloads);
  assert.equal(button.href, RELEASES_URL);
  assert.equal(button.status, es.noBuild);
});

test("el botón grande habla el idioma elegido", () => {
  assert.equal(mainButton(releaseFromApi(json), platform("windows"), TEXTS.en).label, "Download for Windows");
});

test("cada botón de formato baja su archivo, o avisa que viene próximamente", () => {
  const release = releaseFromApi(json);
  assert.ok(formatButton("msi", release, es).href.endsWith(".msi"));
  assert.equal(formatButton("msi", release, es).label, es.formats.msi);
  assert.deepEqual(formatButton("deb", null, es), { label: "Próximamente (.deb)", href: RELEASES_URL });
  assert.equal(formatButton("deb", undefined, es).label, es.formats.deb);
});

test("los links chicos son los otros sistemas, o los tres si no se detectó ninguno", () => {
  const release = releaseFromApi(json);
  const links = otherLinks(release, platform("macos"), es);
  assert.deepEqual(links.items.map((i) => [i.os, i.visible]), [["macos", false], ["windows", true], ["linux", true]]);
  assert.equal(links.label, es.alsoFor);
  assert.ok(links.items[1].href.endsWith("-setup.exe"));
  const all = otherLinks(null, platform("mobile"), es);
  assert.ok(all.items.every((i) => i.visible && i.href === RELEASES_URL));
  assert.equal(all.label, es.downloadsFor);
});
