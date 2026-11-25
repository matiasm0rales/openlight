# Release de OpenLight

Cómo se arman y publican los instaladores. El workflow es
[`.github/workflows/release.yml`](../.github/workflows/release.yml).

## Qué produce

| SO | Instaladores |
|---|---|
| macOS | `.dmg` universal (arm64 + x64) y `.app.tar.gz` (para el updater) |
| Windows | `.msi` (WiX) y `-setup.exe` (NSIS) |
| Linux | `.AppImage`, `.deb` y `.rpm` |

Con la clave del updater cargada, además salen los `.sig` y el `latest.json`. Sin ella salen
los mismos instaladores, pero la app instalada **no puede actualizarse sola** (el workflow avisa).

## Cómo se dispara

- **Probar sin publicar:** pestaña *Actions → Release → Run workflow*, o
  `gh workflow run release.yml --ref <rama>`. Los instaladores quedan como *artifacts* del run
  (`instalador-macos`, `instalador-windows`, `instalador-linux`). No se crea ninguna Release.
  También corre solo en los PR que tocan el workflow, `tauri.conf.json`, `Info.plist` o los íconos.
- **Publicar:** empujar un tag `v*`. Ver [Publicar una versión](#publicar-una-versión).

## Publicar una versión

Antes de la primera, hacé una sola vez [la clave del updater](#la-clave-del-updater-una-sola-vez).

1. Subí la versión en `app/src-tauri/tauri.conf.json` (`version`, es la que compara el updater) y
   en el `Cargo.toml` raíz (`[workspace.package] version`). Corré `cargo test -p openlight` para que
   se actualice `Cargo.lock`, commiteá y mergeá a `main`. La primera release (v0.1.0) ya tiene la
   versión puesta.
2. Con `main` actualizado, creá y empujá el tag:

   ```bash
   git tag v0.1.0
   ```

   ```bash
   git push origin v0.1.0
   ```

   El workflow corta enseguida si el tag no coincide con la versión de `tauri.conf.json`.
3. Esperá a que termine. Quedan los instaladores en un **borrador** de Release, y el job
   *Verificar latest.json* comprueba que estén las firmas de todas las plataformas. Si falla porque
   falta alguna, corré *Re-run all jobs*: reusa el mismo borrador.
4. En el borrador, escribí las novedades en la descripción: es lo que muestra la app en
   **Novedades** antes de actualizar (se ve como texto simple; usá listas cortas).
5. **Publicá** la Release. El workflow *Notas del updater* copia la descripción al `latest.json`
   (también si la editás después). Desde ese momento la gente recibe la versión.

Los tags con guion (`v0.2.0-1`) salen como pre-release y **no** llegan a nadie. El MSI de WiX no
acepta pre-releases alfanuméricos (`0.2.0-rc1`) en el campo de versión: para un candidato usá un
número.

## macOS: firma y notarización (opcional)

El workflow firma y notariza **solo si existen los secrets**; si no, arma el `.dmg` sin firmar y
avisa con un warning. Hace falta una cuenta de Apple Developer (USD 99 por año).

1. Con tu cuenta, en <https://developer.apple.com/account/resources/certificates> creá un
   certificado **Developer ID Application** y descargalo, o exportalo desde *Acceso a Llaveros*
   como `.p12` (con contraseña).
2. Pasalo a base64: `base64 -i certificado.p12 | pbcopy`.
3. En *Settings → Secrets and variables → Actions* del repo cargá:

| Secret | Qué es |
|---|---|
| `APPLE_CERTIFICATE` | el `.p12` en base64 |
| `APPLE_CERTIFICATE_PASSWORD` | la contraseña del `.p12` |
| `APPLE_SIGNING_IDENTITY` | por ejemplo `Developer ID Application: Tu Nombre (TEAMID)`; para verla: `security find-identity -v -p codesigning` |
| `APPLE_ID` | el Apple ID con el que notarizás |
| `APPLE_PASSWORD` | una **contraseña de aplicación** de <https://account.apple.com> (no la del Apple ID) |
| `APPLE_TEAM_ID` | el Team ID de 10 caracteres |

Con los tres primeros se firma; con los seis se firma y se notariza. Firmar siempre con la misma
identidad es lo que hace que los permisos de Grabación de pantalla y Red local sobrevivan a las
actualizaciones.

Sin firma, macOS muestra "no se puede abrir porque no se puede verificar al desarrollador":
se abre con clic derecho → *Abrir*, o `xattr -dr com.apple.quarantine /Applications/OpenLight.app`.

`Info.plist` ya trae `NSLocalNetworkUsageDescription` y `NSScreenCaptureUsageDescription`
(`app/src-tauri/Info.plist`, lo fusiona Tauri).

## Windows

Sin firmar en la v1: SmartScreen va a mostrar "Windows protegió su PC"; hay que elegir *Más
información → Ejecutar de todas formas*. Para firmar más adelante hace falta un certificado de
firma de código (o Azure Trusted Signing) y configurar `bundle.windows.signCommand`.

**Firewall:** el instalador NSIS es por usuario (`installMode: currentUser`, sin pedir admin),
y crear una regla de Windows Firewall requiere admin, así que **no se agrega ninguna regla**.
Al primer descubrimiento de focos por UDP, Windows muestra el aviso "Permitir que OpenLight se
comunique en redes privadas": hay que aceptarlo. Si quedó bloqueado, desde una terminal de admin:

```powershell
netsh advfirewall firewall add rule name="OpenLight" dir=in action=allow program="<carpeta de instalación>\openlight.exe" protocol=UDP profile=private
```

## Linux

El `.deb` depende de `libpipewire-0.3-0` y el `.rpm` de `pipewire-libs` (captura de pantalla por
PipeWire). El AppImage necesita FUSE 2 en la máquina. Flatpak queda para más adelante.

## Actualizaciones automáticas (updater)

### Qué ve la gente

La app busca una versión nueva al abrirse y cada 6 horas (se apaga en *Ajustes → Actualizaciones →
Buscar actualizaciones automáticamente*; ahí también está la versión actual y el botón para buscar a
mano). Si hay una, el panel muestra la versión, las novedades y **Actualizar y reiniciar**. Nada se
instala sin que se toque ese botón.

Al tocarlo: descarga el paquete y verifica la firma, devuelve los focos a como estaban (el mismo
camino que *Salir*), instala y vuelve a abrir la app. Si algo falla, sigue andando la versión de
antes, el efecto se retoma si estaba prendido, y se sugiere qué hacer.

| Sistema | Cómo se actualiza |
|---|---|
| macOS | reemplaza la app. Tiene que estar en *Aplicaciones* (desde el `.dmg` no se puede escribir) |
| Windows | corre el instalador NSIS (o el MSI, según cómo se instaló) con una barra de progreso |
| Linux AppImage | reemplaza el archivo `.AppImage` |
| Linux `.deb` / `.rpm` | instala el paquete nuevo; el sistema pide la contraseña |

Sin firma de Apple, cada versión tiene una firma de código distinta para macOS: después de
actualizar puede volver a pedir el permiso de Grabación de pantalla (si queda activado pero no
anda, quitá OpenLight de la lista y agregalo de nuevo).

### La clave del updater (una sola vez)

Los paquetes se firman con una clave propia, distinta de la de Apple. **No la generes dentro del
repo ni la commitees.** Hacelo en tu máquina, desde la carpeta del repo:

1. Generá el par de claves (poné una contraseña y guardala):

   ```bash
   pnpm tauri signer generate -w ~/.tauri/openlight.key
   ```

   Crea `~/.tauri/openlight.key` (privada) y `~/.tauri/openlight.key.pub` (pública). Guardá la
   privada y la contraseña en tu gestor de contraseñas: **si las perdés, las apps instaladas no se
   pueden actualizar más** y hay que pedirle a la gente que reinstale a mano.
2. Cargá la privada como secret del repo:

   ```bash
   gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/openlight.key
   ```

3. Cargá la contraseña (el comando la pide y no queda en el historial):

   ```bash
   gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD
   ```

4. Copiá la clave **pública**:

   ```bash
   pbcopy < ~/.tauri/openlight.key.pub
   ```

   Pegala en `app/src-tauri/tauri.conf.json`, en `plugins.updater.pubkey`, reemplazando todo el
   texto `REEMPLAZAR_POR_LA_CLAVE_PUBLICA_DEL_UPDATER (ver docs/release.md)`. Es una sola línea y es
   pública: commiteala y mergeala **antes** de taguear.

La clave pública queda grabada en cada app: una versión compilada sin ella no puede actualizarse,
así que la v0.1.0 tiene que salir con los cuatro pasos hechos.

### Cómo se comporta el workflow

| Estado | Resultado |
|---|---|
| sin `TAURI_SIGNING_PRIVATE_KEY` | instaladores sin `.sig` ni `latest.json`, con un warning |
| con la clave y el placeholder en `pubkey` | **falla a propósito**, para no publicar una versión que no pueda actualizarse |
| con la clave y la pública | agrega `tauri.updater.conf.json` (`createUpdaterArtifacts`) y `tauri-action` sube los `.sig` y el `latest.json` |

La app consulta
`https://github.com/matiasm0rales/openlight/releases/latest/download/latest.json`. Ese enlace
ignora borradores y pre-releases. El `latest.json` lleva `darwin-aarch64` y `darwin-x86_64` (los dos
apuntan al paquete universal), `windows-x86_64` y `linux-x86_64`, más una entrada por tipo de
paquete (`-nsis`, `-msi`, `-appimage`, `-deb`, `-rpm`). La firma incluye la versión
(`requireSignedVersion`), así que no se puede hacer pasar un paquete viejo por uno nuevo.

### Probar sin publicar una release

**Solo la interfaz**, en el navegador y sin Tauri: `pnpm -C app/ui dev` y abrí
<http://localhost:1420/?scenario=configured&update=available>. Otros escenarios: `hang` (descarga
detenida al 40 %), `offline`, `offline_download`, `bad_signature`, `install_fails`,
`restart_fails`.

**Con la app de verdad:** en builds de desarrollo, `OPENLIGHT_UPDATER_ENDPOINT` cambia dónde se
busca el `latest.json` (en los builds de release se ignora).

1. En una carpeta cualquiera fuera del repo, creá un archivo `paquete` con cualquier contenido y
   este `latest.json` (la clave de `platforms` es la de tu máquina, por ejemplo `darwin-aarch64`,
   `darwin-x86_64`, `windows-x86_64` o `linux-x86_64`):

   ```json
   {
     "version": "9.9.9",
     "notes": "## Novedades\n\n- Versión de prueba.",
     "platforms": {
       "darwin-aarch64": { "signature": "firma-de-prueba", "url": "http://localhost:8123/paquete" }
     }
   }
   ```

2. Servila:

   ```bash
   python3 -m http.server 8123 --bind 127.0.0.1
   ```

3. En otra terminal, desde el repo:

   ```bash
   OPENLIGHT_UPDATER_ENDPOINT=http://localhost:8123/latest.json pnpm tauri dev
   ```

El panel muestra la versión 9.9.9. Con esa firma de prueba, **Actualizar y reiniciar** descarga y
termina en el error de verificación de firma; apagando el servidor se ve el de falta de conexión.
No uses un paquete firmado de verdad en `tauri dev`: al instalarlo reemplazaría la carpeta del
binario de desarrollo. La actualización completa se prueba instalando una versión publicada y
publicando la siguiente.
