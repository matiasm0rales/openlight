#!/usr/bin/env bash
# Comprueba, en una Mac y sin tocar tu llavero, que un certificado propio da una identidad de
# código estable: dos builds distintas firmadas con él piden el mismo «designated requirement»
# (lo que macOS guarda con el permiso de Grabación de pantalla), y con firma ad-hoc no.
#
# Todo es descartable: el certificado, la clave y el llavero se crean en una carpeta temporal y se
# borran al terminar. No sirve para publicar (para eso está crear-certificado-macos.sh).
#
# Uso: tools/release/probar-firma-macos.sh [App1.app App2.app]
#   Sin argumentos arma dos apps mínimas. Con dos .app (de dos builds distintas) las copia y
#   firma las copias.
set -euo pipefail

[ "$(uname)" = "Darwin" ] || { echo "Solo corre en macOS." >&2; exit 1; }

aqui=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
casa="$tmp/casa"
llavero="$tmp/runner/openlight-firma.keychain-db"
trap 'HOME="$casa" security delete-keychain "$llavero" >/dev/null 2>&1 || true; rm -rf "$tmp"' EXIT

fallar() {
  echo "FALLÓ: $*" >&2
  exit 1
}

clave=$(/usr/bin/openssl rand -hex 16)
OPENLIGHT_P12_PASSWORD="$clave" "$aqui/crear-certificado-macos.sh" "$tmp/certificado" >/dev/null
p12="$tmp/certificado/openlight-firma.p12"
[ -f "$p12" ] || fallar "no se creó el .p12"
[ "$(stat -f %Lp "$p12")" = 600 ] || fallar "el .p12 lo puede leer otra cuenta"
# Lo que el script se niega a hacer.
crear() { OPENLIGHT_P12_PASSWORD="$1" "$aqui/crear-certificado-macos.sh" "$2" >/dev/null 2>&1; }
if crear "$clave" "$tmp/certificado"; then fallar "pisó un certificado existente"; fi
if crear corta "$tmp/corta"; then fallar "aceptó una contraseña corta"; fi
if crear "$clave" "$aqui/no-deberia-existir"; then fallar "escribió una clave dentro del repo"; fi
[ ! -e "$aqui/no-deberia-existir" ] || fallar "creó una carpeta en el repo antes de negarse"

# El paso de release.yml, tal cual, con su propio HOME: la lista de llaveros que modifica es la de
# esa carpeta y la tuya no se toca.
mkdir -p "$casa/Library/Preferences" "$tmp/runner"
preparar() {
  : >"$tmp/env"
  : >"$tmp/path"
  env -i PATH="/usr/bin:/bin:/usr/sbin:/sbin" HOME="$casa" RUNNER_TEMP="$tmp/runner" \
    GITHUB_ENV="$tmp/env" GITHUB_PATH="$tmp/path" "$@" "$aqui/preparar-firma-macos.sh"
}
# El valor que quedó en $GITHUB_ENV para la variable `$1`.
exportado() {
  awk -v nombre="$1" '
    $0 == nombre "<<__FIN_" nombre "__" { dentro = 1; next }
    $0 == "__FIN_" nombre "__" { dentro = 0 }
    dentro' "$tmp/env"
}

echo "== Elección de la firma"
preparar >"$tmp/salida"
[ "$(exportado OPENLIGHT_FIRMA)" = adhoc ] || fallar "sin secrets tenía que quedar ad-hoc"
grep -q '^::warning::.*Grabación de pantalla' "$tmp/salida" || fallar "falta el aviso de ad-hoc"
[ -z "$(exportado APPLE_SIGNING_IDENTITY)" ] || fallar "ad-hoc no define identidad"
echo "sin secrets: ad-hoc con aviso"

if preparar MACOS_CERTIFICATE=algo >"$tmp/salida" 2>&1; then
  fallar "con un solo secret del certificado propio tenía que cortar"
fi
grep -q '^::error::.*MACOS_CERTIFICATE_PASSWORD MACOS_SIGNING_IDENTITY' "$tmp/salida" ||
  fallar "el error no dice qué secrets faltan"
echo "certificado a medio configurar: corta con error"

preparar APPLE_CERTIFICATE=a APPLE_CERTIFICATE_PASSWORD=b APPLE_SIGNING_IDENTITY=c \
  MACOS_CERTIFICATE=d MACOS_CERTIFICATE_PASSWORD=e MACOS_SIGNING_IDENTITY=f >/dev/null
[ "$(exportado OPENLIGHT_FIRMA)" = apple ] || fallar "los secrets de Apple tienen prioridad"
[ "$(exportado APPLE_SIGNING_IDENTITY)" = c ] || fallar "identidad de Apple equivocada"
[ ! -s "$tmp/path" ] || fallar "con Apple no se intercepta codesign"
echo "con secrets de Apple: Apple"

secretos=(
  MACOS_CERTIFICATE="$(base64 -i "$p12")"
  MACOS_CERTIFICATE_PASSWORD="$clave"
  MACOS_SIGNING_IDENTITY="OpenLight Code Signing"
)
if preparar "${secretos[@]}" MACOS_SIGNING_IDENTITY="Otra identidad" >"$tmp/salida" 2>&1; then
  fallar "con una identidad que no está en el .p12 tenía que cortar"
fi
rm -f "$llavero"
HOME="$casa" security list-keychains -d user -s >/dev/null
echo "identidad equivocada: corta con error"

preparar "${secretos[@]}" >/dev/null
[ "$(exportado OPENLIGHT_FIRMA)" = propia ] || fallar "tenía que usar el certificado propio"
[ ! -f "$tmp/runner/openlight-firma.p12" ] || fallar "quedó el .p12 en disco"
identidad=$(exportado APPLE_SIGNING_IDENTITY)
[ -z "$(exportado APPLE_CERTIFICATE)" ] || fallar "Tauri no tiene que importar el certificado"
atajo=$(cat "$tmp/path")
echo "con el certificado propio: identidad «$identidad»"
echo

# `codesign` como lo llama Tauri: por nombre en el PATH, sin `--keychain`.
firmar() {
  env HOME="$casa" PATH="$atajo:$PATH" codesign --force -s "$identidad" "$@"
}

# Una app mínima cuyo binario contiene `$2`, para que las dos "builds" sean distintas.
app_minima() {
  mkdir -p "$1/Contents/MacOS"
  printf '#include <stdio.h>\nint main(void) { puts("%s"); return 0; }\n' "$2" |
    cc -x c - -o "$1/Contents/MacOS/prueba"
  cat >"$1/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>io.github.matiasm0rales.openlight.prueba</string>
  <key>CFBundleExecutable</key><string>prueba</string>
  <key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
PLIST
}

uno="$tmp/uno/Prueba.app"
dos="$tmp/dos/Prueba.app"
if [ $# -eq 2 ]; then
  uno="$tmp/uno/$(basename "$1")"
  dos="$tmp/dos/$(basename "$2")"
  mkdir -p "$tmp/uno" "$tmp/dos"
  cp -R "$1" "$uno"
  cp -R "$2" "$dos"
else
  app_minima "$uno" "version 1"
  app_minima "$dos" "version 2"
fi

requisito() {
  codesign --display --requirements - "$1" 2>/dev/null | sed -n 's/^\(# \)\{0,1\}designated => //p'
}
cdhash() {
  codesign --display --verbose=4 "$1" 2>&1 | sed -n 's/^CDHash=//p'
}

echo "== Firma ad-hoc (lo de hoy)"
for app in "$uno" "$dos"; do
  codesign --force --sign - --options runtime "$app" 2>/dev/null
done
adhoc_uno=$(requisito "$uno")
adhoc_dos=$(requisito "$dos")
echo "build 1: $adhoc_uno"
echo "build 2: $adhoc_dos"
[ -n "$adhoc_uno" ] && [ "$adhoc_uno" != "$adhoc_dos" ] ||
  fallar "con firma ad-hoc esperaba requisitos distintos"

echo
echo "== Certificado propio (los mismos argumentos que usa Tauri)"
for app in "$uno" "$dos"; do
  firmar --options runtime "$app"
  codesign --verify --deep --strict "$app"
done
propio_uno=$(requisito "$uno")
propio_dos=$(requisito "$dos")
echo "build 1: $propio_uno  (cdhash $(cdhash "$uno"))"
echo "build 2: $propio_dos  (cdhash $(cdhash "$dos"))"
[ "$(cdhash "$uno")" != "$(cdhash "$dos")" ] || fallar "las dos builds son iguales: no prueba nada"
[ -n "$propio_uno" ] && [ "$propio_uno" = "$propio_dos" ] ||
  fallar "los requisitos no coinciden"
case "$propio_uno" in
  *cdhash*) fallar "el requisito depende del cdhash" ;;
  *'certificate leaf = H"'* | *'certificate root = H"'*) ;;
  *) fallar "el requisito no nombra al certificado" ;;
esac
# La comprobación que hace macOS: la build nueva cumple el requisito guardado de la vieja.
codesign --verify --test-requirement="=$propio_uno" "$dos" ||
  fallar "la build 2 no cumple el requisito de la build 1"
echo "la build 2 cumple el requisito de la build 1"

if [ $# -eq 0 ]; then
  [ "$("$dos/Contents/MacOS/prueba")" = "version 2" ] || fallar "la app firmada no arranca"
  echo "la app firmada (hardened runtime) arranca"
fi

hdiutil create -quiet -volname Prueba -srcfolder "$dos" "$tmp/prueba.dmg"
firmar "$tmp/prueba.dmg" 2>/dev/null
if /usr/bin/codesign --display "$tmp/prueba.dmg" >/dev/null 2>&1; then
  fallar "el .dmg quedó firmado"
fi
# Control: sin el intermediario sí se firmaría, así que la comprobación de arriba dice algo.
cp "$tmp/prueba.dmg" "$tmp/control.dmg"
env HOME="$casa" /usr/bin/codesign --force -s "$identidad" "$tmp/control.dmg"
/usr/bin/codesign --display "$tmp/control.dmg" >/dev/null 2>&1 || fallar "el control no se firmó"
echo "el .dmg queda sin firmar"

echo
echo "OK: con el certificado propio el requisito se mantiene entre builds."
