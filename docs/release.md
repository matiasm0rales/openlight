# Release de OpenLight

Cómo se arman y publican los instaladores. El workflow es
[`.github/workflows/release.yml`](../.github/workflows/release.yml).

## Qué produce

| SO | Instaladores |
|---|---|
| macOS | `.dmg` universal (arm64 + x64) y `.app.tar.gz` (para el updater) |
| Windows | `.msi` (WiX) y `-setup.exe` (NSIS) |
| Linux | `.AppImage`, `.deb` y `.rpm` |

Con la clave del updater cargada, además salen los `.sig` y el `latest.json`.

## Cómo se dispara

- **Probar sin publicar:** pestaña *Actions → Release → Run workflow*, o
  `gh workflow run release.yml --ref <rama>`. Los instaladores quedan como *artifacts* del run
  (`instalador-macos`, `instalador-windows`, `instalador-linux`). No se crea ninguna Release.
  También corre solo en los PR que tocan el workflow, `tauri.conf.json`, `Info.plist` o los íconos.
- **Publicar:** empujar un tag `v*` (por ejemplo `v0.1.0`). El workflow sube los instaladores a un
  **borrador** de Release; revisalo y publicalo a mano. Los tags con guion (`v0.1.0-rc1`) salen
  marcados como pre-release.

Antes de taguear, la versión de `app/src-tauri/tauri.conf.json` y la del workspace (`Cargo.toml`
raíz) tienen que coincidir con el tag. El MSI de WiX no acepta pre-releases alfanuméricos
(`0.1.0-rc1`) en el campo de versión, así que para un candidato conviene usar un número
(`0.1.0-1`) o aceptar que el MSI no se genere en esa corrida.

## macOS: firma y notarización (opcional)

El workflow firma con **Developer ID** y notariza **solo si existen los secrets**. Sin ellos avisa
con un warning y la `.app` sale con **firma ad-hoc** (`bundle.macOS.signingIdentity: "-"` en
`tauri.conf.json`); lo mismo pasa con `pnpm tauri build` en tu máquina. La variable
`APPLE_SIGNING_IDENTITY`, cuando está definida, reemplaza a ese `-`.

- La firma ad-hoc sella los recursos de la app. Sin ella queda solo la firma del linker,
  `codesign --verify --deep --strict` falla y es probable que una descarga en Apple Silicon diga que
  la app «está dañada». No identifica a nadie ni reemplaza a la notarización.
- El `.dmg` queda **sin firmar a propósito**: Tauri no le pone firma ad-hoc porque un `.dmg`
  autofirmado puede no abrirse ([tauri#12288](https://github.com/tauri-apps/tauri/issues/12288)).
- El workflow verifica la `.app` suelta y la que va dentro del `.dmg`, y falla si los recursos no
  quedaron sellados.

Para firmar con Developer ID hace falta una cuenta de Apple Developer (USD 99 por año):

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
actualizaciones. Con la firma ad-hoc cada build tiene una identidad distinta: después de actualizar,
macOS puede volver a pedir esos permisos y la contraseña del llavero.

### Abrir la app descargada sin Developer ID

La primera vez, macOS avisa que no puede verificar la app y no la abre:

- **macOS 15 o más nuevo:** intentá abrirla, cerrá el aviso (sin mandar la app a la papelera) y andá
  a *Ajustes del Sistema → Privacidad y seguridad*. En la sección *Seguridad* tocá **Abrir
  igualmente** y confirmá.
- **macOS 14 o anterior:** clic derecho sobre OpenLight → *Abrir*.
- Si en cambio dice que la app **«está dañada»**, no es lo esperado con la firma ad-hoc: reportalo.
  Se destraba con `xattr -dr com.apple.quarantine /Applications/OpenLight.app`.

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

La app trae `tauri-plugin-updater` y consulta
`https://github.com/matiasm0rales/openlight/releases/latest/download/latest.json`. Los paquetes
se firman con una clave propia (distinta de la de Apple). **No generes ni commitees la clave en el
repo.** Pasos, una sola vez:

1. Generá el par de claves en tu máquina:

   ```bash
   pnpm tauri signer generate -w ~/.tauri/openlight.key
   ```

   Te pide una contraseña (puede ser vacía, mejor que no). Crea `~/.tauri/openlight.key`
   (privada) y `~/.tauri/openlight.key.pub` (pública). Guardá la privada en tu gestor de
   contraseñas: si la perdés, las apps ya instaladas no pueden actualizarse.
2. Cargá dos secrets en el repo (*Settings → Secrets and variables → Actions*):
   - `TAURI_SIGNING_PRIVATE_KEY`: el **contenido** de `~/.tauri/openlight.key`.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: la contraseña (omitilo si la dejaste vacía).
3. Pegá la clave **pública** (contenido de `openlight.key.pub`) en
   `app/src-tauri/tauri.conf.json`, en `plugins.updater.pubkey`, reemplazando el placeholder
   `REEMPLAZAR_POR_LA_CLAVE_PUBLICA_DEL_UPDATER...`, y commiteala. Es pública, no hay problema.

Cómo se comporta el workflow:

- Sin `TAURI_SIGNING_PRIVATE_KEY`: el release sale igual, sin paquetes del updater (warning).
- Con la clave pero con el placeholder todavía en `pubkey`: el workflow **falla a propósito**, para
  no publicar una versión que no pueda actualizarse.
- Con ambas: se agrega `app/src-tauri/tauri.updater.conf.json` (`createUpdaterArtifacts`) y
  `tauri-action` sube los `.sig` y el `latest.json` al borrador.

El updater lee `releases/latest`, que ignora borradores y pre-releases: una versión recién
llega a los usuarios cuando publicás la Release. La app todavía no muestra ninguna interfaz de
actualización: eso se conecta en la tarea I1 (comando que llama a `check()` del plugin y permiso
`updater:default` en `capabilities/`).
