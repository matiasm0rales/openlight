# Contenido editable del asistente

Los textos y dibujos de la guía del portal de Tuya viven acá y **no en el código**, porque el
portal cambia sus pantallas seguido (riesgo de la sección 4.4 del plan). Para actualizarlos no
hace falta tocar Svelte:

- `guide/es.json` y `guide/en.json`: misma estructura en los dos idiomas (una prueba lo verifica).
  - `topics`: guías de ayuda que abren los errores (`show_help`) y los enlaces "¿Cómo?".
    Las claves son los `HelpTopic` del contrato (`tuya_account`, `cloud_project`, ...).
  - `wizard`: los pasos 3, 4 y 5 del asistente (cuenta, proyecto Cloud, vincular Smart Life).
  - `data_centers`: el selector "¿dónde creaste tu cuenta de Smart Life?" → Data Center (`Region`).
- `img/*.svg`: ilustraciones. Se referencian por nombre de archivo (sin `.svg`) desde el JSON.
  Q2 las reemplaza por capturas reales del portal (PNG o SVG con el mismo nombre).

Los textos van en español rioplatense con voseo, igual que el resto de la app.
