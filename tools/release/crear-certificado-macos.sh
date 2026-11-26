#!/usr/bin/env bash
# Crea el certificado autofirmado de firma de código con el que se firman TODAS las versiones de
# OpenLight para macOS cuando no hay cuenta de Apple Developer (ver docs/release.md).
#
# Corrélo una sola vez, en tu máquina:
#
#   tools/release/crear-certificado-macos.sh [carpeta-de-salida]
#
# Deja `openlight-firma.p12` (certificado + clave privada, con contraseña) en la carpeta de salida
# (por defecto, ~/openlight-firma) y explica cómo cargarlo en GitHub. La clave sin cifrar solo
# existe en una carpeta temporal mientras corre el script.
#
# No lo vuelvas a generar: macOS ata los permisos a ESTE certificado. Con otro distinto, cada
# persona que ya tenga la app pierde el permiso de Grabación de pantalla al actualizar.
#
# Variables opcionales: OPENLIGHT_P12_PASSWORD (si no está, la pide) y OPENSSL.
set -euo pipefail
umask 077

# El nombre va en el certificado y en el secret MACOS_SIGNING_IDENTITY.
identidad="OpenLight Code Signing"
repo="matiasm0rales/openlight"
# Veinte años: un certificado vencido no puede firmar, y cambiarlo rompe los permisos.
dias=7300

fallar() {
  echo "error: $*" >&2
  exit 1
}

# El openssl del sistema (LibreSSL) viene con macOS y escribe un .p12 que `security` entiende.
openssl=${OPENSSL:-/usr/bin/openssl}
[ -x "$openssl" ] || openssl=$(command -v openssl) || fallar "no encuentro openssl"

salida=${1:-"$HOME/openlight-firma"}
# Antes de crear nada: la carpeta (o la primera que exista hacia arriba) no puede estar en un repo.
existente=$salida
while [ ! -d "$existente" ]; do existente=$(dirname "$existente"); done
if command -v git >/dev/null && git -C "$existente" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  fallar "$salida está dentro de un repositorio git: la clave no puede quedar donde se pueda commitear."
fi
mkdir -p "$salida"
salida=$(cd "$salida" && pwd)
p12="$salida/openlight-firma.p12"
if [ -e "$p12" ]; then
  fallar "ya existe $p12. Usá ese: un certificado nuevo les rompe los permisos a quienes ya tienen la app."
fi

clave=${OPENLIGHT_P12_PASSWORD:-}
if [ -z "$clave" ]; then
  [ -t 0 ] || fallar "falta OPENLIGHT_P12_PASSWORD (sin terminal no puedo pedir la contraseña)."
  read -r -s -p "Contraseña para el .p12 (12 caracteres o más): " clave
  echo
  read -r -s -p "Repetila: " repetida
  echo
  [ "$clave" = "$repetida" ] || fallar "las contraseñas no coinciden."
fi
[ "${#clave}" -ge 12 ] || fallar "la contraseña tiene que tener 12 caracteres o más."

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

cat >"$tmp/openssl.cnf" <<EOF
[req]
distinguished_name = nombre
x509_extensions = firma_de_codigo
prompt = no

[nombre]
CN = $identidad
O = OpenLight

[firma_de_codigo]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
EOF

"$openssl" req -x509 -newkey rsa:3072 -sha256 -days "$dias" -nodes \
  -config "$tmp/openssl.cnf" -keyout "$tmp/clave.pem" -out "$tmp/certificado.pem" 2>"$tmp/req.log" ||
  fallar "openssl no pudo crear el certificado: $(cat "$tmp/req.log")"

# 3DES y SHA-1 en el contenedor: es lo que `security import` de macOS entiende (OpenSSL 3 usa por
# defecto algoritmos que macOS rechaza). La contraseña va por variable de entorno, no por argumento.
OPENLIGHT_P12_PASSWORD="$clave" "$openssl" pkcs12 -export \
  -inkey "$tmp/clave.pem" -in "$tmp/certificado.pem" -name "$identidad" \
  -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 \
  -passout env:OPENLIGHT_P12_PASSWORD -out "$tmp/firma.p12"
mv "$tmp/firma.p12" "$p12"

huella=$("$openssl" x509 -in "$tmp/certificado.pem" -noout -fingerprint -sha1 | sed 's/.*=//; s/://g')

cat <<EOF
Listo: $p12
Identidad: $identidad
Huella SHA-1 del certificado: $huella

1. Guardá una copia del .p12 y su contraseña en tu gestor de contraseñas. Si se pierden, habrá que
   crear otro certificado y todas las personas con la app tendrán que volver a dar el permiso.

2. Cargá los tres secrets en GitHub:

   base64 -i "$p12" | gh secret set MACOS_CERTIFICATE --repo $repo

   gh secret set MACOS_CERTIFICATE_PASSWORD --repo $repo
   (la pide y no queda en el historial)

   gh secret set MACOS_SIGNING_IDENTITY --repo $repo --body "$identidad"

   También se puede en Settings → Secrets and variables → Actions.

3. Probalo sin publicar nada:

   gh workflow run release.yml --repo $repo

   En el job de macOS tiene que decir que firma con el certificado propio.

No commitees el .p12 ni se lo pases a nadie: quien lo tenga puede firmar apps que macOS tome por
OpenLight.
EOF
