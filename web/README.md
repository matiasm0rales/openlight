# Web de descarga de OpenLight

Una sola página en español e inglés para que cualquiera baje la app e instale solo. Es HTML, CSS y
JavaScript sin framework ni paso de build: lo que está en `public/` es lo que se publica.

## Correrla en tu máquina

Hace falta Node 20 o más nuevo. Desde `web/`:

```bash
npm run dev
```

Queda en <http://localhost:4180>. No hay que instalar nada para verla. El servidor
(`scripts/serve.mjs`) manda las mismas cabeceras de seguridad que `vercel.json`.

## Pruebas

```bash
npm ci
npx playwright install chromium
npm test
```

- `npm run test:unit` (sin dependencias): qué instalador se elige según el sistema y la
  arquitectura, con un JSON de ejemplo de la API (`tests/fixtures/release-latest.json`), y que no
  falte ni sobre ninguna traducción.
- `npm run test:e2e` (Playwright): descargas, «Próximamente», idiomas, pestañas con teclado, tema,
  animaciones, la página sin JavaScript, auditoría de accesibilidad con axe y desbordes en el
  celular. La API de GitHub se responde dentro de las pruebas.

En CI corren con `.github/workflows/web.yml`, solo cuando cambia `web/`.

axe no mide el contraste de texto sobre degradados. Si cambiás los colores del fondo animado
(`.aurora`) o `--muted`, revisá a mano que el texto siga por encima de 4,5:1.

## Publicarla en Vercel

1. *Add New → Project* e importá `matiasm0rales/openlight`.
2. En **Root Directory** elegí `web`.
3. No toques el resto: `vercel.json` ya define *Framework Preset* «Other», sin instalación ni build,
   y **Output Directory** `public`.
4. *Deploy*. Queda en un subdominio `.vercel.app`; un dominio propio se agrega después en
   *Settings → Domains*, sin cambiar nada del código.

Con el repo conectado, cada push a `main` publica la versión de producción y cada PR genera una vista
previa.

## Cómo funcionan las descargas

Al abrir la página se consulta
`https://api.github.com/repos/matiasm0rales/openlight/releases/latest` desde el navegador (esa API
ignora borradores y pre-releases, así que una versión aparece recién cuando publicás la Release).

| Sistema | Botón principal | También ofrece |
|---|---|---|
| macOS | `.dmg` universal | |
| Windows | `-setup.exe` | `.msi` |
| Linux | `.AppImage` (`.deb` o `.rpm` si el navegador dice qué distribución es) | los tres |

Si no hay releases, la API falla o se agota el límite de consultas (60 por hora y por IP), los
botones dicen **«Próximamente»** y llevan a la página de releases. Sin JavaScript también llevan ahí.

La elección está en `public/js/downloads.js`. Si cambian los nombres de los instaladores en
`release.yml`, actualizá ese archivo y el JSON de ejemplo.

## Cambiar textos

- Español: directamente en `public/index.html`.
- Inglés: en `public/js/i18n.js`, objeto `EN`, con la clave del `data-i18n` correspondiente.
- Frases que arma el JavaScript (botones de descarga, tema, animaciones): `TEXTS`, en los dos
  idiomas.

El idioma inicial es español salvo que el navegador esté en inglés; `?lang=en` o `?lang=es` lo
fuerzan en un enlace.

## Reemplazar las ilustraciones por capturas

Cada lugar para una captura es un `<figure class="shot" data-shot="…">` con un dibujo SVG adentro:

| `data-shot` | Qué muestra |
|---|---|
| `mac-arrastrar` | El `.dmg` abierto, arrastrando a Aplicaciones |
| `mac-aviso` | El aviso de macOS al abrir por primera vez |
| `mac-abrir-igual` | Privacidad y seguridad → «Abrir igualmente» |
| `windows-smartscreen` | «Windows protegió su PC» |
| `windows-firewall` | El aviso del firewall |
| `actualizacion` | El aviso de versión nueva dentro de la app |

Para poner una captura, guardala en `public/img/capturas/` y cambiá el `<svg>` por:

```html
<img src="img/capturas/mac-aviso.es.png" width="640" height="400" alt="Aviso de macOS…"
     data-i18n-attr="src:mac.2.shot;alt:mac.2.alt">
```

y agregá `"mac.2.shot": "img/capturas/mac-aviso.en.png"` en `EN` (si la captura es la misma en los
dos idiomas, dejá solo `alt:` en `data-i18n-attr`). Al sacar un dibujo, borrá de `EN` las claves
`il.*` que ya no se usen: la prueba de traducciones avisa cuáles.

> **Nunca subas capturas con datos reales:** nada de claves, Access ID, IP, nombres de redes, mails
> ni nombres de cuentas. Usá una cuenta de prueba o tapá los datos antes de guardar la imagen.

## Archivos

| Ruta | Qué hay |
|---|---|
| `public/index.html` | La página (en español) |
| `public/styles.css` | Estilos, temas y movimiento |
| `public/js/downloads.js` | Detectar el sistema y elegir el instalador |
| `public/js/view.js` | Qué dice cada botón |
| `public/js/i18n.js` | Inglés y frases dinámicas |
| `public/js/main.js` | Conecta todo con la página |
| `public/js/boot.js` | Aplica el tema guardado antes de pintar |
| `public/img/` | Logos e ícono, copiados de `app/` |
| `public/fonts/` | Plus Jakarta Sans (licencia OFL incluida) |
