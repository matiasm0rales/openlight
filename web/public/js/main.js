import { LATEST_RELEASE_API, detectPlatform } from "./downloads.js";
import { EN, TEXTS, pickLanguage } from "./i18n.js";
import { formatButton, mainButton, manualUpdates, otherLinks, releaseFromApi } from "./view.js";

const root = document.documentElement;
const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];

const store = {
  get(key) {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Es solo una comodidad: si no se puede guardar, se vuelve a elegir la próxima vez.
    }
  },
};

const state = {
  lang: pickLanguage({ search: location.search, saved: store.get("openlight-lang"), language: navigator.language }),
  platform: detectPlatform(navigator),
  json: undefined, // undefined: buscando; null: la API falló
  // Animaciones: lo que se haya elegido acá; si no, lo que pida el sistema.
  motion: store.get("openlight-motion")
    ? store.get("openlight-motion") === "on"
    : !matchMedia("(prefers-reduced-motion: reduce)").matches,
};
const texts = () => TEXTS[state.lang];

/* ---------- Idioma ---------- */

// El español está en el HTML: se guarda la primera vez para poder volver a él.
const originals = new WeakMap();

function translate(element, slot, key, read, write) {
  let saved = originals.get(element);
  if (!saved) originals.set(element, (saved = {}));
  saved[slot] ??= read();
  write(state.lang === "en" ? (EN[key] ?? saved[slot]) : saved[slot]);
}

function applyLanguage() {
  root.lang = state.lang;
  for (const el of $$("[data-i18n]")) {
    translate(el, "text", el.dataset.i18n, () => el.textContent, (v) => (el.textContent = v));
  }
  // Solo textos propios (i18n.js), nunca datos de afuera.
  for (const el of $$("[data-i18n-html]")) {
    translate(el, "html", el.dataset.i18nHtml, () => el.innerHTML, (v) => (el.innerHTML = v));
  }
  for (const el of $$("[data-i18n-attr]")) {
    for (const pair of el.dataset.i18nAttr.split(";")) {
      const [attr, key] = pair.split(":");
      translate(el, attr, key, () => el.getAttribute(attr), (v) => el.setAttribute(attr, v));
    }
  }
  for (const button of $$("[data-lang]")) button.setAttribute("aria-pressed", String(button.dataset.lang === state.lang));
  renderDownloads();
  renderTheme();
  renderPlay();
}

for (const button of $$("[data-lang]")) {
  button.addEventListener("click", () => {
    state.lang = button.dataset.lang;
    store.set("openlight-lang", state.lang);
    // Un ?lang= en la dirección pisaría lo recién elegido al recargar.
    const url = new URL(location.href);
    url.searchParams.delete("lang");
    history.replaceState(null, "", url);
    applyLanguage();
  });
}

/* ---------- Descargas ---------- */

function renderDownloads() {
  const { platform, json } = state;
  const release = json === undefined ? undefined : releaseFromApi(json, platform.arch);
  const t = texts();

  const main = mainButton(release, platform, t);
  $("#download-main").href = main.href;
  $("#download-label").textContent = main.label;
  // role="status": solo se escribe si cambió, para no repetir el anuncio.
  if ($("#download-status").textContent !== main.status) $("#download-status").textContent = main.status;

  const others = otherLinks(release, platform, t);
  $("#download-others-label").textContent = others.label;
  for (const item of others.items) {
    const link = $(`#download-others a[data-os="${item.os}"]`);
    link.href = item.href;
    link.hidden = !item.visible;
  }

  // El texto de siempre sobre actualizaciones ya lo puso applyLanguage; acá solo se pisa si la
  // versión publicada no trae actualizador.
  const manual = manualUpdates(release, t);
  if (manual) {
    $("#actualiza-titulo").textContent = manual.title;
    $(".updates-text .section-lead").textContent = manual.text;
  }
  $(".updates .shot").hidden = Boolean(manual);

  for (const link of $$("a[data-download]")) {
    const button = formatButton(link.dataset.download, release, t);
    link.href = button.href;
    link.textContent = button.label;
  }
}

async function loadRelease() {
  try {
    const response = await fetch(LATEST_RELEASE_API, {
      headers: { Accept: "application/vnd.github+json" },
      signal: AbortSignal.timeout(8000),
    });
    // 404 = todavía no hay releases; 403 = límite de consultas. En los dos casos, «Próximamente».
    state.json = response.ok ? await response.json() : null;
  } catch {
    state.json = null;
  }
  renderDownloads();
}

async function refinePlatform() {
  // Solo Chromium sabe decir la arquitectura; en el resto queda lo que diga el user agent.
  const hints = await navigator.userAgentData?.getHighEntropyValues(["architecture"]).catch(() => null);
  if (!hints?.architecture) return;
  state.platform = detectPlatform({
    userAgent: navigator.userAgent,
    maxTouchPoints: navigator.maxTouchPoints,
    architecture: hints.architecture,
  });
  renderDownloads();
}

/* ---------- Pestañas de instalación ---------- */

const tabs = $$('[role="tab"]');

function selectTab(tab, { focus = false } = {}) {
  for (const other of tabs) {
    const selected = other === tab;
    other.setAttribute("aria-selected", String(selected));
    other.tabIndex = selected ? 0 : -1;
    $(`#${other.getAttribute("aria-controls")}`).hidden = !selected;
  }
  if (focus) tab.focus();
}

tabs.forEach((tab, index) => {
  tab.addEventListener("click", () => selectTab(tab));
  tab.addEventListener("keydown", (event) => {
    const target = {
      ArrowRight: tabs[(index + 1) % tabs.length],
      ArrowLeft: tabs[(index - 1 + tabs.length) % tabs.length],
      Home: tabs[0],
      End: tabs.at(-1),
    }[event.key];
    if (!target) return;
    event.preventDefault();
    selectTab(target, { focus: true });
  });
});

// Quien baja el instalador de otro sistema quiere ver los pasos de ese sistema.
for (const link of $$("#download-others a[data-os]")) {
  link.addEventListener("click", () => selectTab($(`#tab-${link.dataset.os}`)));
}

selectTab($(`#tab-${state.platform.os}`) ?? tabs[0]);

/* ---------- Tema ---------- */

const darkQuery = matchMedia("(prefers-color-scheme: dark)");
const isDark = () => (root.dataset.theme ? root.dataset.theme === "dark" : darkQuery.matches);

function renderTheme() {
  $("#theme").setAttribute("aria-label", isDark() ? texts().toLight : texts().toDark);
  root.classList.toggle("is-dark", isDark());
}

$("#theme").addEventListener("click", () => {
  root.dataset.theme = isDark() ? "light" : "dark";
  store.set("openlight-theme", root.dataset.theme);
  renderTheme();
});
darkQuery.addEventListener("change", renderTheme);

/* ---------- Demo y animaciones ---------- */

const sceneButtons = $$("button[data-scene]");
let timer = null;
// La demo pasa sola de escena hasta que alguien elige una.
let autoScene = true;

function showScene(scene) {
  root.dataset.scene = scene;
  for (const button of sceneButtons) button.setAttribute("aria-pressed", String(button.dataset.scene === scene));
}

function renderPlay() {
  $("#demo-play").textContent = state.motion ? texts().pause : texts().play;
}

function syncMotion() {
  // La hoja de estilos mueve el fondo y las ilustraciones solo con data-motion="on".
  root.dataset.motion = state.motion ? "on" : "off";
  clearInterval(timer);
  if (state.motion && autoScene) {
    timer = setInterval(() => {
      const current = sceneButtons.findIndex((b) => b.dataset.scene === root.dataset.scene);
      showScene(sceneButtons[(current + 1) % sceneButtons.length].dataset.scene);
    }, 4500);
  }
  renderPlay();
}

for (const button of sceneButtons) {
  button.addEventListener("click", () => {
    autoScene = false;
    showScene(button.dataset.scene);
    syncMotion();
  });
}
$("#demo-play").addEventListener("click", () => {
  state.motion = !state.motion;
  autoScene = state.motion;
  store.set("openlight-motion", state.motion ? "on" : "off");
  syncMotion();
});

/* ---------- Arranque ---------- */

applyLanguage();
syncMotion();
loadRelease();
refinePlatform();
