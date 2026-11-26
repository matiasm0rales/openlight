# openlight-ui

La interfaz de OpenLight: Svelte 5 (runes) + TypeScript + Vite. Habla con el backend solo a través
de `src/lib/bindings.ts` (generado por tauri-specta desde `app/src-tauri/src/ipc.rs`; no se edita).

## Correrla sin backend

```bash
pnpm --filter openlight-ui dev
```

Abrí <http://localhost:1420>. Fuera de Tauri se instala solo un backend falso (`src/lib/mocks`) que
responde los mismos comandos y emite los mismos eventos, con datos realistas: dos focos (W509Z2 en
3.5 y A60 BAW en 3.3), un foco sin color, la clave rechazada (914), la región equivocada, cortes de
conexión y `color://live` a 8 Hz por foco.

- `?scenario=fresh`: el asistente desde cero. `?scenario=configured`: dos focos, uno con 914.
  `?scenario=four`: cuatro focos sanos, para el orden y las distribuciones de zonas.
- `?fast=1`: demoras cortas (lo usan las pruebas).
- En **Menú del tray** hay un panel para forzar fallas (914, cortes, sin monitor).
- El paso 6 tiene pistas en `src/lib/mocks/backend.ts` (secretos de largo equivocado, cuentas vencidas…).

## Estructura

- `src/screens`: pantallas A–G de la sección 3.9 del plan, más el mock del tray.
- `src/lib/components`: piezas compartidas (`ErrorCard`, `SettingControl`, `ZoneEditor`, `Eclipse`…).
- `src/lib/app.svelte.ts`: estado compartido alimentado por comandos y eventos.
- `src/lib/i18n/{es,en}.json`: todos los textos (español con voseo e inglés).
- `src/content`: guía del portal de Tuya e ilustraciones, editables sin tocar código.

Los ajustes se generan desde el catálogo de `settings_get`: la pantalla no conoce ningún ajuste.
Cada `UserFacingError` se muestra con su título, su explicación y sus acciones.

## Pruebas

```bash
pnpm test        # Vitest + Playwright (levanta `pnpm dev` solo)
pnpm check       # svelte-check
```

Playwright necesita Chromium una vez: `pnpm exec playwright install chromium`. Las pruebas de punta
a punta corren contra el backend falso e incluyen una auditoría de axe (sin violaciones serias ni
críticas) en cada pantalla, en tema claro y oscuro.

Capturas de las pantallas: `CAPTURAS=<carpeta> pnpm exec playwright test e2e/capturas.spec.ts`.
