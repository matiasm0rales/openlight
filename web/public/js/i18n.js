// Idiomas de la web.
//
// - El español vive en index.html: es la fuente y lo que se ve sin JavaScript.
// - `EN` traduce, clave por clave, cada `data-i18n`, `data-i18n-html` y `data-i18n-attr` del HTML.
// - `TEXTS` son las frases que arma el JavaScript (botones de descarga, tema, demo), en los dos.
//
// Una prueba (tests/unit/i18n.test.js) verifica que no falte ni sobre ninguna clave.

export function pickLanguage({ search = "", saved = null, language = "" } = {}) {
  const asked = new URLSearchParams(search).get("lang");
  for (const candidate of [asked, saved]) {
    if (candidate === "es" || candidate === "en") return candidate;
  }
  // Igual que la app: español, salvo que el sistema esté en inglés.
  return /^en\b/i.test(language) ? "en" : "es";
}

export const TEXTS = {
  es: {
    downloadFor: "Descargar para {os}",
    download: "Descargar OpenLight",
    soonFor: "Próximamente para {os}",
    soon: "Próximamente",
    allDownloads: "Ver todas las descargas",
    loading: "Buscando la última versión…",
    ready: "Versión {version}. Archivo {file} de {size} MB, gratis.",
    soonStatus:
      "Todavía no hay una versión para descargar desde acá. Mientras tanto podés mirar la página de descargas en GitHub.",
    noBuild: "Todavía no hay un instalador para el procesador de esta compu.",
    mobile: "OpenLight es para computadoras: abrí esta página desde tu Mac o tu PC.",
    chooseSystem: "Elegí tu sistema en la lista de abajo.",
    alsoFor: "También para:",
    downloadsFor: "Descargas para:",
    soonFormat: "Próximamente ({file})",
    formats: {
      dmg: "Descargar el instalador .dmg",
      exe: "Descargar el instalador .exe",
      msi: "Descargar el .msi",
      appimage: "Descargar el AppImage",
      deb: "Descargar el .deb",
      rpm: "Descargar el .rpm",
    },
    toDark: "Usar tema oscuro",
    toLight: "Usar tema claro",
    play: "Activar animaciones",
    pause: "Pausar animaciones",
    manualUpdates: {
      title: "Actualizaciones",
      text: "La versión disponible todavía no se actualiza sola. Cuando salga una nueva, bajala de esta página, cerrá OpenLight e instalala encima.",
    },
  },
  en: {
    downloadFor: "Download for {os}",
    download: "Download OpenLight",
    soonFor: "Coming soon for {os}",
    soon: "Coming soon",
    allDownloads: "See all downloads",
    loading: "Looking for the latest version…",
    ready: "Version {version}. A {size} MB {file} file, free.",
    soonStatus: "There is no version to download from here yet. Meanwhile you can check the downloads page on GitHub.",
    noBuild: "There is no installer for this computer’s processor yet.",
    mobile: "OpenLight runs on computers: open this page on your Mac or PC.",
    chooseSystem: "Pick your system from the list below.",
    alsoFor: "Also for:",
    downloadsFor: "Downloads for:",
    soonFormat: "Coming soon ({file})",
    formats: {
      dmg: "Download the .dmg installer",
      exe: "Download the .exe installer",
      msi: "Download the .msi",
      appimage: "Download the AppImage",
      deb: "Download the .deb",
      rpm: "Download the .rpm",
    },
    toDark: "Use dark theme",
    toLight: "Use light theme",
    play: "Turn animations on",
    pause: "Pause animations",
    manualUpdates: {
      title: "Updates",
      text: "The available version doesn’t update itself yet. When a new one is out, download it from this page, quit OpenLight and install it over the old one.",
    },
  },
};

export const EN = {
  "meta.title": "OpenLight: your bulbs follow the colors on your screen",
  "meta.description":
    "Free app for macOS, Windows and Linux: your Tuya or Smart Life bulbs follow the colors on your screen in real time, over your local network.",
  "nav.skip": "Skip to content",
  "nav.label": "Sections",
  "nav.install": "How to install",
  "nav.faq": "Questions",
  "hero.title": "Your bulbs, in the color of your screen",
  "hero.lead":
    "OpenLight makes your Tuya or Smart Life bulbs follow, in real time, the colors of whatever you are watching. Everything travels over your home network.",
  "hero.howto": "See how to install it, step by step",
  beta: "<strong>Windows and Linux are in beta:</strong> they build and pass the automated tests, but have not been tried with real bulbs yet.",
  "demo.alt": "Illustration: a screen and two bulbs. Each bulb takes the color of the side of the screen next to it.",
  "demo.caption": "Each bulb follows a part of the screen. Try changing what’s on it:",
  "demo.scenes": "Screen scene",
  "demo.sunset": "Sunset",
  "demo.ocean": "Ocean",
  "demo.forest": "Forest",
  "demo.neon": "Neon",
  "needs.title": "What you need",
  "needs.bulbs.title": "Tuya or Smart Life color bulbs",
  "needs.bulbs.text": "Already added in the Smart Life (or Tuya Smart) phone app. White-only bulbs won’t work for this.",
  "needs.wifi.title": "Your computer on the same Wi-Fi",
  "needs.wifi.text": "The computer and the bulbs must be on the same network. Watch out for guest networks.",
  "needs.computer.title": "A supported computer",
  "needs.computer.text":
    "macOS 12.3 or newer, Windows 10 (version 2004 or newer) or 11, or 64-bit Linux on an Intel or AMD processor (Ubuntu 22.04 or equivalent).",
  "needs.time.title": "About 15 minutes",
  "needs.time.text":
    "Just once, with your phone at hand. The setup assistant walks you through creating the free Tuya developer account.",
  "install.title": "How to install it",
  "install.lead":
    "OpenLight is not digitally signed with Apple or Microsoft yet, so the first time your computer will ask for an extra confirmation. Here is every step.",
  "install.tabs": "Your system",
  "il.applications": "Applications",
  "il.done": "Done",
  "il.trash": "Move to Trash",
  "il.privacy": "Privacy & Security",
  "il.security": "Security",
  "il.blocked": "“OpenLight” was blocked.",
  "il.openAnyway": "Open Anyway",
  "il.smartscreen": "Windows protected your PC",
  "il.moreInfo": "More info",
  "il.runAnyway": "Run anyway",
  "il.firewall": "Windows Firewall",
  "il.private": "Private networks",
  "il.public": "Public networks",
  "il.allow": "Allow access",
  "il.newVersion": "A new version is ready",
  "il.update": "Update",
  "mac.1.title": "Copy OpenLight to Applications",
  "mac.1.text": "Open the .dmg file you downloaded and drag the OpenLight icon onto the Applications folder.",
  "mac.1.alt": "Illustration: dragging OpenLight to the Applications folder",
  "mac.2.title": "Open it once and close the warning",
  "mac.2.text":
    "Open OpenLight from Applications. macOS will say it cannot check it and won’t open it. Click “Done”. Don’t move it to the Trash.",
  "mac.2.alt": "Illustration: macOS warning with the “Move to Trash” and “Done” buttons. Click “Done”.",
  "mac.3.title": "Click “Open Anyway”",
  "mac.3.text":
    "Open System Settings and go to Privacy & Security. Scroll down to the Security section and click “Open Anyway”.",
  "mac.3.alt": "Illustration: in Privacy & Security, the “Open Anyway” button next to OpenLight’s name",
  "mac.4.title": "Confirm, and you’re in",
  "mac.4.text": "Click “Open” and confirm with your password or Touch ID. From now on it opens like any other app.",
  "mac.5.title": "Grant the permissions it asks for",
  "mac.5.text":
    "The assistant asks for Screen Recording (to see the colors) and Local Network (to talk to the bulbs). After turning on screen recording, reopen the app.",
  "mac.why.title": "Why does macOS stop it the first time?",
  "mac.why.text":
    "Apple opens without asking only the apps registered in its paid developer program, and OpenLight isn’t in it yet. The warning doesn’t mean a virus was found: it means Apple hasn’t reviewed the app. The code is public, and you should only download it from this page or from GitHub.",
  "mac.why.old": "On macOS 14 or earlier you can simply right-click OpenLight, choose “Open” and confirm.",
  "mac.damaged.q": "Does it say the app “is damaged”?",
  "mac.damaged.a":
    "It’s another form of the same block: macOS flagged the file because it was downloaded from the internet. Only do this if you downloaded it from this page or from GitHub. Open the Terminal app, paste this line, press Enter and open OpenLight again.",
  "win.1.title": "Open the installer",
  "win.1.text":
    "Double-click the file ending in -setup.exe. If your browser says it isn’t commonly downloaded, open the download’s menu and choose to keep it.",
  "win.2.title": "If you see “Windows protected your PC”",
  "win.2.text":
    "Click “More info” and then “Run anyway”. Windows shows that warning for new programs without a digital signature.",
  "win.2.alt": "Illustration: the “Windows protected your PC” warning. First click “More info”, then “Run anyway”.",
  "win.3.title": "Follow the installer",
  "win.3.text": "It does not ask for administrator rights. When it’s done, open OpenLight from the Start menu.",
  "win.4.title": "Allow it on your network",
  "win.4.text":
    "The first time it looks for bulbs, Windows Firewall will ask. Check “Private networks” and allow access.",
  "win.4.alt": "Illustration: Windows Firewall prompt with “Private networks” checked and the “Allow access” button",
  "win.msi": "Prefer an .msi installer (for example, to manage several computers)?",
  "linux.lead":
    "Pick just one format. If unsure, use the .deb on Ubuntu, Debian or Mint, the .rpm on Fedora and the AppImage anywhere else.",
  "linux.appimage.for": "Runs on almost any distribution, with nothing to install.",
  "linux.appimage.1": "Right-click the file, open Properties and turn on “Executable as Program”.",
  "linux.appimage.2": "Double-click to open it.",
  "linux.appimage.terminal": "From a terminal:",
  "linux.appimage.fuse":
    "If it won’t open, install FUSE 2: <code>sudo apt install libfuse2</code> (on Ubuntu 24.04 or newer, <code>libfuse2t64</code>).",
  "linux.deb.for": "Ubuntu, Debian, Linux Mint and derivatives.",
  "linux.deb.text": "Double-click it to install from your software center, or use a terminal in the downloads folder:",
  "linux.rpm.for": "Fedora and derivatives.",
  "linux.rpm.text": "Double-click it to install from your software center, or use a terminal in the downloads folder:",
  "linux.screen.title": "When it asks what to share",
  "linux.screen.text":
    "On Wayland desktops, the system asks which screen you want to share. Pick the monitor: OpenLight remembers your choice.",
  "updates.title": "It updates itself",
  "updates.text":
    "When a new version is out, the app tells you and updates with one click. No need to come back to this page or reinstall.",
  "updates.alt": "Illustration: a notice inside the app saying “A new version is ready”, with an “Update” button",
  "faq.title": "Frequently asked questions",
  "faq.free.q": "Is it free?",
  "faq.free.a": "Yes. OpenLight is free and open source, under the MIT or Apache 2.0 license.",
  "faq.account.q": "Why do I need a Tuya developer account?",
  "faq.account.a":
    "Every bulb has a key that can only be read from the developer portal of Tuya, the company behind Smart Life. The account is free and the assistant guides you. After that, colors go straight from your computer to the bulbs.",
  "faq.offline.q": "Does it work without internet?",
  "faq.offline.a":
    "Once set up, colors only travel over your local network. You need internet to add bulbs, renew their keys and get updates.",
  "faq.privacy.q": "Is my screen uploaded anywhere?",
  "faq.privacy.a":
    "No. The picture is analyzed on your computer and bulbs only receive a color. Keys are stored in your system’s keychain.",
  "faq.bulbs.q": "Which bulbs work? Can I use several?",
  "faq.bulbs.a":
    "Tuya or Smart Life color bulbs (protocols 3.1, 3.3, 3.4 and 3.5). You can use several, each following a different area of the screen.",
  "faq.noresponse.q": "A bulb isn’t responding. What do I do?",
  "faq.noresponse.a":
    "Check it’s on the same Wi-Fi as the computer and close the Smart Life phone app, which sometimes holds the connection. If you paired it again, use “Renew key” inside OpenLight.",
  "faq.permissions.q": "After updating, my Mac asks for permissions again",
  "faq.permissions.a":
    "Because the app isn’t signed yet, after an update macOS may ask again for Screen Recording permission and keychain access. Allow them again and, if you turned screen recording on, reopen OpenLight.",
  "faq.phone.q": "Is there a phone version?",
  "faq.phone.a":
    "No. OpenLight is for computers: it follows the colors on your Mac or PC screen. The phone is only used during setup.",
  "source.title": "The code is out in the open",
  "source.text": "Anyone can check how OpenLight works or suggest improvements on GitHub, the site where the project is published.",
  "source.code": "See the code on GitHub",
  "source.issues": "Report a problem",
  "footer.disclaimer":
    "OpenLight is not affiliated with Philips (Ambilight is a Philips trademark) or with Tuya. Smart Life is a Tuya app.",
};
