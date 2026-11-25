// Funciones puras: qué sistema usa quien visita la web y qué archivo de la release le sirve.
// No tocan el DOM ni la red, así se prueban con `node --test`.

export const REPO_URL = "https://github.com/matiasm0rales/openlight";
export const RELEASES_URL = `${REPO_URL}/releases`;
export const LATEST_RELEASE_API = "https://api.github.com/repos/matiasm0rales/openlight/releases/latest";

/** Formatos de instalador de cada sistema, del más recomendable al menos. */
export const FORMATS = {
  macos: ["dmg"],
  windows: ["exe", "msi"],
  linux: ["appimage", "deb", "rpm"],
};

// Solo instaladores: las firmas (.sig), latest.json y .app.tar.gz son del actualizador.
const PATTERNS = {
  dmg: /\.dmg$/i,
  exe: /-setup\.exe$/i,
  msi: /\.msi$/i,
  appimage: /\.appimage$/i,
  deb: /\.deb$/i,
  rpm: /\.rpm$/i,
};

const ARCHES = {
  universal: /[_.-]universal[_.-]/i,
  arm64: /[_.-](aarch64|arm64)[_.-]/i,
  x64: /[_.-](x64|x86_64|amd64)[_.-]/i,
};

/**
 * @param {{userAgent?: string, maxTouchPoints?: number, architecture?: string}} nav
 *   `architecture` es la client hint de Chromium ("x86" o "arm"), si se pudo pedir.
 * @returns {{os: "macos"|"windows"|"linux"|"mobile"|"unknown", arch: "x64"|"arm64", distro: "debian"|"fedora"|null}}
 */
export function detectPlatform({ userAgent = "", maxTouchPoints = 0, architecture = "" } = {}) {
  const ua = userAgent;
  let os = "unknown";
  if (/Android|iPhone|iPad|iPod|Mobile/i.test(ua)) os = "mobile";
  // Safari en iPad se presenta como una Mac, pero una Mac no tiene pantalla táctil.
  else if (/Macintosh/i.test(ua)) os = maxTouchPoints > 1 ? "mobile" : "macos";
  else if (/Windows/i.test(ua)) os = "windows";
  else if (/Linux/i.test(ua) && !/CrOS/i.test(ua)) os = "linux";

  const arm = /^arm/i.test(architecture) || /aarch64|arm64|armv\d/i.test(ua);
  let distro = null;
  if (os === "linux") {
    if (/Ubuntu|Debian|Mint/i.test(ua)) distro = "debian";
    else if (/Fedora|Red Hat|SUSE/i.test(ua)) distro = "fedora";
  }
  return { os, arch: arm ? "arm64" : "x64", distro };
}

function archOf(name) {
  return Object.keys(ARCHES).find((arch) => ARCHES[arch].test(name)) ?? null;
}

// Menor es mejor; null = ese archivo no corre en esta arquitectura.
function rank(name, format, arch) {
  const own = archOf(name);
  if (own === "universal") return 0;
  if (own === null) return 2;
  if (own === arch) return 1;
  // Windows en ARM emula los programas x64.
  if (own === "x64" && (format === "exe" || format === "msi")) return 3;
  // En Safari no se puede saber si la Mac es Intel o Apple Silicon.
  if (format === "dmg") return 3;
  return null;
}

/**
 * @param {Array<{name: string, browser_download_url: string}>} assets de la API de GitHub
 * @param {"x64"|"arm64"} arch
 * @returns {Record<"dmg"|"exe"|"msi"|"appimage"|"deb"|"rpm", {name: string, browser_download_url: string}|null>}
 */
export function pickDownloads(assets, arch = "x64") {
  const usable = (Array.isArray(assets) ? assets : []).filter(
    (a) => typeof a?.name === "string" && String(a.browser_download_url).startsWith("https://"),
  );
  return Object.fromEntries(
    Object.entries(PATTERNS).map(([format, pattern]) => {
      const candidates = usable
        .filter((a) => pattern.test(a.name))
        .map((a) => ({ asset: a, rank: rank(a.name, format, arch), spanish: /es-ES|es-MX|es-AR/i.test(a.name) }))
        .filter((c) => c.rank !== null)
        .sort((a, b) => a.rank - b.rank || b.spanish - a.spanish);
      return [format, candidates[0]?.asset ?? null];
    }),
  );
}

/** El instalador que va en el botón grande, o null si no hay ninguno para ese sistema. */
export function primaryDownload(downloads, { os, distro = null }) {
  let formats = FORMATS[os] ?? [];
  if (os === "linux" && distro) {
    const own = distro === "debian" ? "deb" : "rpm";
    formats = [own, ...formats.filter((f) => f !== own)];
  }
  const format = formats.find((f) => downloads[f]);
  return format ? { format, asset: downloads[format] } : null;
}
