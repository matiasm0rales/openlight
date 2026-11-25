import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { detectPlatform, pickDownloads, primaryDownload } from "../../public/js/downloads.js";

const release = JSON.parse(readFileSync(new URL("../fixtures/release-latest.json", import.meta.url), "utf8"));
const asset = (name) => ({ name, browser_download_url: `https://example.test/${name}` });
const names = (downloads) => Object.fromEntries(Object.entries(downloads).map(([format, a]) => [format, a?.name ?? null]));

const UA = {
  macSafari: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
  winChrome: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
  linuxChrome: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
  ubuntuFirefox: "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0",
  fedoraFirefox: "Mozilla/5.0 (X11; Fedora; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0",
  linuxArm: "Mozilla/5.0 (X11; Linux aarch64; rv:143.0) Gecko/20100101 Firefox/143.0",
  android: "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
  iphone: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
  chromeOs: "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
};

test("detecta macOS, Windows y Linux a partir del user agent", () => {
  assert.equal(detectPlatform({ userAgent: UA.macSafari }).os, "macos");
  assert.equal(detectPlatform({ userAgent: UA.winChrome }).os, "windows");
  assert.equal(detectPlatform({ userAgent: UA.linuxChrome }).os, "linux");
});

test("un celular no es un sistema de escritorio", () => {
  assert.equal(detectPlatform({ userAgent: UA.android }).os, "mobile");
  assert.equal(detectPlatform({ userAgent: UA.iphone }).os, "mobile");
});

test("un iPad que se presenta como Mac se reconoce por la pantalla táctil", () => {
  assert.equal(detectPlatform({ userAgent: UA.macSafari, maxTouchPoints: 5 }).os, "mobile");
  assert.equal(detectPlatform({ userAgent: UA.macSafari, maxTouchPoints: 0 }).os, "macos");
});

test("ChromeOS y los agentes desconocidos quedan sin sistema", () => {
  assert.equal(detectPlatform({ userAgent: UA.chromeOs }).os, "unknown");
  assert.equal(detectPlatform({ userAgent: "curl/8.7.1" }).os, "unknown");
  assert.equal(detectPlatform({}).os, "unknown");
});

test("la arquitectura sale del user agent o de las client hints", () => {
  assert.equal(detectPlatform({ userAgent: UA.winChrome }).arch, "x64");
  assert.equal(detectPlatform({ userAgent: UA.linuxArm }).arch, "arm64");
  assert.equal(detectPlatform({ userAgent: UA.winChrome, architecture: "arm" }).arch, "arm64");
});

test("Firefox en Ubuntu o Fedora indica la distribución", () => {
  assert.equal(detectPlatform({ userAgent: UA.ubuntuFirefox }).distro, "debian");
  assert.equal(detectPlatform({ userAgent: UA.fedoraFirefox }).distro, "fedora");
  assert.equal(detectPlatform({ userAgent: UA.linuxChrome }).distro, null);
});

test("de una release completa elige un instalador por formato y descarta firmas y paquetes del updater", () => {
  assert.deepEqual(names(pickDownloads(release.assets)), {
    dmg: "OpenLight_0.1.0_universal.dmg",
    exe: "OpenLight_0.1.0_x64-setup.exe",
    msi: "OpenLight_0.1.0_x64_en-US.msi",
    appimage: "OpenLight_0.1.0_amd64.AppImage",
    deb: "OpenLight_0.1.0_amd64.deb",
    rpm: "OpenLight-0.1.0-1.x86_64.rpm",
  });
});

test("en macOS prefiere el .dmg universal aunque haya uno por arquitectura", () => {
  const assets = [asset("OpenLight_0.1.0_aarch64.dmg"), asset("OpenLight_0.1.0_universal.dmg")];
  assert.equal(pickDownloads(assets, "arm64").dmg.name, "OpenLight_0.1.0_universal.dmg");
});

test("sin .dmg universal elige el de la arquitectura de la compu", () => {
  const assets = [asset("OpenLight_0.1.0_aarch64.dmg"), asset("OpenLight_0.1.0_x64.dmg")];
  assert.equal(pickDownloads(assets, "arm64").dmg.name, "OpenLight_0.1.0_aarch64.dmg");
  assert.equal(pickDownloads(assets, "x64").dmg.name, "OpenLight_0.1.0_x64.dmg");
});

test("en Linux no ofrece un paquete de otra arquitectura", () => {
  const downloads = pickDownloads(release.assets, "arm64");
  assert.equal(downloads.appimage, null);
  assert.equal(downloads.deb, null);
  assert.equal(downloads.rpm, null);
});

test("en Linux ARM usa el paquete ARM si existe", () => {
  const assets = [asset("OpenLight_0.1.0_amd64.deb"), asset("OpenLight_0.1.0_arm64.deb")];
  assert.equal(pickDownloads(assets, "arm64").deb.name, "OpenLight_0.1.0_arm64.deb");
  assert.equal(pickDownloads(assets, "x64").deb.name, "OpenLight_0.1.0_amd64.deb");
});

test("Windows en ARM recibe el instalador x64, que corre emulado", () => {
  const downloads = pickDownloads(release.assets, "arm64");
  assert.equal(downloads.exe.name, "OpenLight_0.1.0_x64-setup.exe");
  assert.equal(downloads.msi.name, "OpenLight_0.1.0_x64_en-US.msi");
});

test("entre varios .msi elige el que está en español", () => {
  const assets = [asset("OpenLight_0.1.0_x64_en-US.msi"), asset("OpenLight_0.1.0_x64_es-ES.msi")];
  assert.equal(pickDownloads(assets).msi.name, "OpenLight_0.1.0_x64_es-ES.msi");
});

test("ignora enlaces que no son https", () => {
  const assets = [{ name: "OpenLight_0.1.0_universal.dmg", browser_download_url: "javascript:alert(1)" }];
  assert.equal(pickDownloads(assets).dmg, null);
});

test("una lista de assets ausente o rota no rompe nada", () => {
  for (const assets of [undefined, null, [], [{}], [null], "x"]) {
    assert.equal(pickDownloads(assets).dmg, null);
  }
});

test("el instalador principal depende del sistema", () => {
  const downloads = pickDownloads(release.assets);
  assert.equal(primaryDownload(downloads, { os: "macos" }).format, "dmg");
  assert.equal(primaryDownload(downloads, { os: "windows" }).format, "exe");
  assert.equal(primaryDownload(downloads, { os: "linux" }).format, "appimage");
  assert.equal(primaryDownload(downloads, { os: "linux", distro: "debian" }).format, "deb");
  assert.equal(primaryDownload(downloads, { os: "linux", distro: "fedora" }).format, "rpm");
});

test("si falta el formato preferido usa el siguiente del mismo sistema", () => {
  const downloads = pickDownloads([asset("OpenLight_0.1.0_x64_en-US.msi")]);
  assert.equal(primaryDownload(downloads, { os: "windows" }).format, "msi");
});

test("sin instalador para el sistema no hay descarga principal", () => {
  const downloads = pickDownloads([asset("OpenLight_0.1.0_universal.dmg")]);
  assert.equal(primaryDownload(downloads, { os: "windows" }), null);
  assert.equal(primaryDownload(downloads, { os: "mobile" }), null);
  assert.equal(primaryDownload(downloads, { os: "unknown" }), null);
});
