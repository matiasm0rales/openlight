// Qué muestran los botones de descarga según la release, el sistema y el idioma.
// Funciones puras: main.js solo copia el resultado al DOM.
//
// `release` tiene tres estados: undefined (buscando), null (no hay o falló la API) u objeto.

import { FORMATS, RELEASES_URL, pickDownloads, primaryDownload } from "./downloads.js";

export const OS_NAMES = { macos: "macOS", windows: "Windows", linux: "Linux" };
const FILE_LABELS = { dmg: ".dmg", exe: ".exe", msi: ".msi", appimage: ".AppImage", deb: ".deb", rpm: ".rpm" };

const fill = (template, values) => template.replace(/\{(\w+)\}/g, (_, key) => values[key]);

/** Interpreta la respuesta de la API. Sin ningún instalador, es como si no hubiera release. */
export function releaseFromApi(json, arch = "x64") {
  const downloads = pickDownloads(json?.assets, arch);
  if (!Object.values(downloads).some(Boolean)) return null;
  return { version: String(json.tag_name ?? "").replace(/^v/, ""), downloads };
}

/** El botón grande de la portada. */
export function mainButton(release, platform, texts) {
  const os = OS_NAMES[platform.os];
  const mobile = platform.os === "mobile";
  if (release === null) {
    return {
      label: os ? fill(texts.soonFor, { os }) : texts.soon,
      href: RELEASES_URL,
      status: mobile ? texts.mobile : texts.soonStatus,
    };
  }
  if (!os) {
    return { label: texts.allDownloads, href: RELEASES_URL, status: mobile ? texts.mobile : texts.chooseSystem };
  }
  const label = fill(texts.downloadFor, { os });
  if (release === undefined) return { label, href: RELEASES_URL, status: texts.loading };
  const primary = primaryDownload(release.downloads, platform);
  if (!primary) return { label: texts.allDownloads, href: RELEASES_URL, status: texts.noBuild };
  const size = Math.max(1, Math.round((primary.asset.size ?? 0) / 1024 / 1024));
  return {
    label,
    href: primary.asset.browser_download_url,
    status: fill(texts.ready, { version: release.version, file: FILE_LABELS[primary.format], size }),
  };
}

/** Los botones de cada formato en las instrucciones de instalación. */
export function formatButton(format, release, texts) {
  if (release === null) {
    return { label: fill(texts.soonFormat, { file: FILE_LABELS[format] }), href: RELEASES_URL };
  }
  return { label: texts.formats[format], href: release?.downloads[format]?.browser_download_url ?? RELEASES_URL };
}

/** Los links chicos a los otros sistemas, debajo del botón grande. */
export function otherLinks(release, platform, texts) {
  const detected = Boolean(OS_NAMES[platform.os]);
  return {
    label: detected ? texts.alsoFor : texts.downloadsFor,
    items: Object.keys(FORMATS).map((os) => ({
      os,
      visible: os !== platform.os,
      // El sistema elegido acá no es el detectado: la distribución de Linux no se conoce.
      href: (release && primaryDownload(release.downloads, { os })?.asset.browser_download_url) || RELEASES_URL,
    })),
  };
}
