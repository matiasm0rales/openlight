#!/usr/bin/env bash
# Paso de release.yml (runner de macOS): elige con qué se firma la app y deja las variables que lee
# Tauri en $GITHUB_ENV. Por orden de preferencia (ver docs/release.md):
#
#   1. Apple: APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD y APPLE_SIGNING_IDENTITY (con APPLE_ID,
#      APPLE_PASSWORD y APPLE_TEAM_ID además notariza). Tauri importa el certificado él mismo.
#   2. Certificado propio: MACOS_CERTIFICATE, MACOS_CERTIFICATE_PASSWORD y MACOS_SIGNING_IDENTITY.
#      Se importa a un llavero temporal y a Tauri solo se le pasa la identidad.
#   3. Nada: firma ad-hoc (`signingIdentity: "-"` en tauri.conf.json).
#
# Deja además OPENLIGHT_FIRMA = apple | propia | adhoc, para la verificación posterior.
set -euo pipefail

: "${GITHUB_ENV:?falta GITHUB_ENV}"

exportar() {
  printf '%s<<__FIN_%s__\n%s\n__FIN_%s__\n' "$1" "$1" "$2" "$1" >>"$GITHUB_ENV"
}

if [ -n "${APPLE_CERTIFICATE:-}" ] && [ -n "${APPLE_CERTIFICATE_PASSWORD:-}" ] &&
  [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  exportar APPLE_CERTIFICATE "$APPLE_CERTIFICATE"
  exportar APPLE_CERTIFICATE_PASSWORD "$APPLE_CERTIFICATE_PASSWORD"
  exportar APPLE_SIGNING_IDENTITY "$APPLE_SIGNING_IDENTITY"
  exportar OPENLIGHT_FIRMA apple
  echo "Se firma con la identidad Developer ID configurada."
  if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
    exportar APPLE_ID "$APPLE_ID"
    exportar APPLE_PASSWORD "$APPLE_PASSWORD"
    exportar APPLE_TEAM_ID "$APPLE_TEAM_ID"
    echo "Se notariza con la cuenta de Apple configurada."
  else
    echo "::warning::Faltan APPLE_ID, APPLE_PASSWORD o APPLE_TEAM_ID: la .app se firma pero no se notariza."
  fi
  exit 0
fi

if [ -n "${MACOS_CERTIFICATE:-}${MACOS_CERTIFICATE_PASSWORD:-}${MACOS_SIGNING_IDENTITY:-}" ]; then
  # A medio cargar no se cae a ad-hoc en silencio: esa versión le rompería el permiso a la gente.
  faltan=""
  for nombre in MACOS_CERTIFICATE MACOS_CERTIFICATE_PASSWORD MACOS_SIGNING_IDENTITY; do
    [ -n "${!nombre:-}" ] || faltan="$faltan $nombre"
  done
  if [ -n "$faltan" ]; then
    echo "::error::El certificado propio está a medio configurar: faltan$faltan (ver docs/release.md)."
    exit 1
  fi
  : "${RUNNER_TEMP:?falta RUNNER_TEMP}" "${GITHUB_PATH:?falta GITHUB_PATH}"

  llavero="$RUNNER_TEMP/openlight-firma.keychain-db"
  clave_llavero=$(openssl rand -hex 24)
  p12="$RUNNER_TEMP/openlight-firma.p12"
  trap 'rm -f "$p12"' EXIT
  (umask 077 && printf '%s' "$MACOS_CERTIFICATE" | base64 --decode >"$p12")

  security create-keychain -p "$clave_llavero" "$llavero"
  security set-keychain-settings -lut 21600 "$llavero"
  security unlock-keychain -p "$clave_llavero" "$llavero"
  if ! security import "$p12" -k "$llavero" -f pkcs12 -P "$MACOS_CERTIFICATE_PASSWORD" \
    -T /usr/bin/codesign >/dev/null; then
    echo "::error::No se pudo importar MACOS_CERTIFICATE: revisá el base64 y la contraseña."
    exit 1
  fi
  # Sin esto codesign pide permiso con una ventana que en el runner nadie puede contestar.
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$clave_llavero" \
    "$llavero" >/dev/null
  # Tauri llama a codesign sin `--keychain`: el llavero tiene que estar en la lista de búsqueda.
  anteriores=()
  while IFS= read -r otro; do
    [ -n "$otro" ] && anteriores+=("$otro")
  done < <(security list-keychains -d user | sed 's/^ *"//; s/"$//')
  security list-keychains -d user -s "$llavero" ${anteriores[@]+"${anteriores[@]}"}

  identidades=$(security find-identity -p codesigning "$llavero")
  if ! grep -qF "\"$MACOS_SIGNING_IDENTITY\"" <<<"$identidades"; then
    echo "::error::El certificado no tiene la identidad «${MACOS_SIGNING_IDENTITY}» (MACOS_SIGNING_IDENTITY)."
    echo "$identidades"
    exit 1
  fi

  # Tauri también firma el .dmg con la identidad configurada, y un .dmg firmado con un certificado
  # propio puede no abrirse (tauri#12288). Este `codesign` intermedio saltea solo ese pedido.
  atajo="$RUNNER_TEMP/openlight-codesign"
  mkdir -p "$atajo"
  cat >"$atajo/codesign" <<'ATAJO'
#!/bin/bash
firma=0
for argumento in "$@"; do
  case "$argumento" in -s | --sign) firma=1 ;; esac
  ultimo=$argumento
done
if [ "$firma" = 1 ] && [[ "${ultimo:-}" == *.dmg ]]; then
  echo "OpenLight: el .dmg queda sin firmar ($ultimo)" >&2
  exit 0
fi
exec /usr/bin/codesign "$@"
ATAJO
  chmod +x "$atajo/codesign"
  echo "$atajo" >>"$GITHUB_PATH"

  exportar APPLE_SIGNING_IDENTITY "$MACOS_SIGNING_IDENTITY"
  exportar OPENLIGHT_FIRMA propia
  echo "Se firma con el certificado propio «${MACOS_SIGNING_IDENTITY}», sin notarizar."
  exit 0
fi

exportar OPENLIGHT_FIRMA adhoc
echo "::warning::Sin secrets de firma, la .app sale con firma ad-hoc: cada actualización le rompe a la gente el permiso de Grabación de pantalla. Cargá el certificado propio (docs/release.md)."
