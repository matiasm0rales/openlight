# Errores frecuentes

Cada error de OpenLight muestra un **código** debajo del mensaje. Buscalo acá. Los códigos no
cambian entre versiones.

- [Antes que nada](#antes-que-nada): los tres casos más comunes.
- Sistema y pantalla: [`screen_permission_denied`](#screen_permission_denied) ·
  [`local_network_denied`](#local_network_denied) · [`firewall_blocked`](#firewall_blocked) ·
  [`capture_unsupported`](#capture_unsupported) · [`no_display`](#no_display)
- Focos: [`device_not_found`](#device_not_found) ·
  [`device_not_found_on_lan`](#device_not_found_on_lan) · [`lan_scan_failed`](#lan_scan_failed) ·
  [`device_key_rejected`](#device_key_rejected) · [`device_timeout`](#device_timeout) ·
  [`device_busy`](#device_busy) · [`device_no_colour`](#device_no_colour)
- Nube de Tuya: [`cloud_invalid_signature`](#cloud_invalid_signature) ·
  [`cloud_invalid_token`](#cloud_invalid_token) ·
  [`cloud_permission_denied`](#cloud_permission_denied) ·
  [`cloud_wrong_region`](#cloud_wrong_region) ·
  [`cloud_service_not_authorized`](#cloud_service_not_authorized) ·
  [`cloud_trial_expired`](#cloud_trial_expired) · [`cloud_network`](#cloud_network) ·
  [`cloud_no_credentials`](#cloud_no_credentials)
- Configuración y otros: [`no_zones`](#no_zones) · [`import_invalid`](#import_invalid) ·
  [`invalid_setting`](#invalid_setting) · [`storage`](#storage) ·
  [`not_implemented`](#not_implemented) · [`internal`](#internal)

## Antes que nada

La mayoría de los problemas con un foco son uno de estos tres.

### Otra app está ocupando el foco

Muchos focos atienden muy pocas conexiones locales a la vez. Los sospechosos habituales:

- la app **Smart Life** abierta en un celular;
- el **script anterior** (`ambilight.py`), la app que lo lanzaba o cualquier programa que use
  tinytuya, andando en esta u otra compu;
- otra integración del hogar (por ejemplo, un servidor de domótica).

No siempre se ve como `device_busy`. También puede aparecer como `device_timeout`,
`device_not_found`, `device_not_found_on_lan`, `lan_scan_failed` (el script ocupa además el
puerto de búsqueda) y hasta `device_key_rejected`.

**Arreglo:** cerrá todo lo que pueda estar hablando con el foco (en el celular, cerrá Smart Life
del todo) y reintentá.

### Un foco 3.5 que acepta una sola conexión

Los focos con protocolo 3.5 (por ejemplo, el W509Z2) aceptan **una sola conexión** a la vez. Si
alguien ya la tiene, OpenLight no puede entrar, y como en 3.4 y 3.5 cada conexión empieza con
una negociación de clave, una negociación que falla se informa como **clave rechazada** aunque la
clave esté bien.

**Arreglo:** antes de renovar la clave, cerrá las otras apps, apagá el foco, esperá unos
segundos, prendelo y reintentá. Si no volviste a vincular el foco en Smart Life, es muy raro que
la clave haya cambiado.

### El foco está apagado desde la tecla

Sin corriente, el foco no existe en la red: no contesta ni se anuncia. Vas a ver
`device_timeout`, `device_not_found` o `device_not_found_on_lan`, y con el efecto andando la
tarjeta queda en **Reconectando**.

**Arreglo:** prendelo desde la tecla y esperá unos 10 segundos. Con el efecto andando no hace
falta tocar nada: OpenLight reintenta solo, cada vez más espaciado (3, 6 y 12 segundos con los
valores por defecto), y cuando el foco vuelve lo pone otra vez en modo color. Para apagar la luz
usá la app, no la tecla.

## Sistema y pantalla

### screen_permission_denied

**«Falta el permiso de Grabación de pantalla»**

- **Qué significa:** la app no puede ver la pantalla.
- **Por qué pasa:** en macOS no se concedió el permiso, o se concedió y no se reabrió la app. En
  Linux con Wayland, se canceló el diálogo para compartir pantalla.
- **Arreglo:** en macOS, **Ajustes del Sistema → Privacidad y seguridad → Grabación de
  pantalla**, activá OpenLight, salí y volvé a abrirla. Si ya figura activada pero sigue el error
  (suele pasar después de actualizar una app sin firmar), quitala de la lista con **−** y agregala
  de nuevo. En Wayland, volvé a arrancar el efecto y elegí el monitor en el diálogo.

### local_network_denied

**«Falta el permiso de Red local»** (macOS)

- **Qué significa:** no se pudo escuchar la red para buscar focos.
- **Por qué pasa:** macOS tiene bloqueado el acceso de OpenLight a la red local.
- **Arreglo:** **Ajustes del Sistema → Privacidad y seguridad → Red local**, activá OpenLight y
  reintentá. Si no aparece en la lista, reabrí la app. Si el permiso ya está, mirá
  [`lan_scan_failed`](#lan_scan_failed): otro programa puede estar usando los puertos.

### firewall_blocked

**«El firewall bloquea la búsqueda de focos»** (Windows y Linux)

- **Qué significa:** no se pudieron abrir los puertos UDP 6666, 6667 y 7000 para escuchar a los
  focos.
- **Por qué pasa:** el firewall lo impide u otro programa tiene los puertos. Ojo: un firewall
  también puede bloquear los avisos **sin dar ningún error**; en ese caso los focos simplemente
  no aparecen.
- **Arreglo:** en Windows, permití OpenLight en redes privadas (**Seguridad de Windows →
  Firewall y protección de red → Permitir una aplicación**). Si quedó una regla de bloqueo,
  borrala o creá la de permiso desde una terminal de administrador (comando en
  [`release.md`](release.md#windows)). En Linux, abrí esos puertos UDP en tu firewall. Mientras
  tanto podés poner la IP fija del foco en **Dispositivos**.

### capture_unsupported

**«No se puede capturar la pantalla»**

- **Qué significa:** este sistema no ofrece una forma de capturar la pantalla que OpenLight sepa
  usar.
- **Por qué pasa:** en Linux, un escritorio Wayland sin el portal *ScreenCast* (o sin PipeWire),
  o una sesión sin entorno gráfico.
- **Arreglo:** instalá el portal de tu escritorio (`xdg-desktop-portal` y el específico de GNOME,
  KDE o wlroots) y PipeWire, o iniciá una sesión X11.

### no_display

**«No hay monitor»**

- **Qué significa:** no se encontró la pantalla que había que capturar.
- **Por qué pasa:** el monitor de una zona está apagado o desconectado, está dormido y no entrega
  imagen, o (en macOS) falta el permiso de grabación y la vista previa llega vacía.
- **Arreglo:** prendé o conectá el monitor y reintentá, o elegí otro en las zonas
  (**Dispositivos**). Si el monitor desaparece **con el efecto andando**, no hace falta hacer
  nada: los focos vuelven a su estado y siguen solos cuando la pantalla vuelve.

## Focos

### device_not_found

**«No encuentro el foco»**

- **Qué significa:** el foco no respondió en el tiempo de espera, o la app ya no lo tiene
  configurado.
- **Por qué pasa:** está apagado desde la tecla, en otra Wi-Fi, cambió de IP (y tiene IP fija),
  otra app lo ocupa, o se quitó mientras la pantalla todavía lo mostraba.
- **Arreglo:** prendelo, confirmá que la compu está en la misma red y cerrá Smart Life. Si tiene
  **IP fija** en Dispositivos, pasalo a **Automática** o corregila.

### device_not_found_on_lan

**«No encontré el foco en tu red»**

- **Qué significa:** al agregar el foco no apareció en la búsqueda, así que no se sabe su IP ni
  su versión de protocolo. OpenLight no adivina: no lo agrega.
- **Por qué pasa:** apagado desde la tecla, en otra red (invitados, otra banda o VLAN), ocupado
  por otro programa, o el router no deja que los equipos se vean entre sí (*aislamiento de
  clientes*).
- **Arreglo:** prendelo, cerrá Smart Life y el script anterior, esperá unos segundos y tocá
  **Buscar de nuevo**.

### lan_scan_failed

**«No pude escuchar la red para buscar el foco»**

- **Qué significa:** la búsqueda ni siquiera pudo empezar. El mensaje trae el detalle técnico.
- **Por qué pasa:** otro programa tiene tomado el puerto de búsqueda (el script anterior es el
  caso típico), falta el permiso de Red local en macOS, o el firewall lo impide.
- **Arreglo:** cerrá los otros programas que usen tus focos, revisá el
  [permiso](#local_network_denied) o el [firewall](#firewall_blocked) y tocá **Buscar de nuevo**.

Si el foco ya estaba agregado, un error de búsqueda no le borra nada: conserva su IP y versión.

### device_key_rejected

**«El foco rechazó la clave»** (error 914 de Tuya)

- **Qué significa:** no se pudo establecer la conexión cifrada con la clave local guardada.
- **Por qué pasa:**
  - el foco se volvió a vincular en Smart Life, así que **cambió su clave**;
  - en un foco 3.4 o 3.5, la negociación falló por otra causa: [otra conexión lo
    ocupa](#un-foco-35-que-acepta-una-sola-conexión) o quedó colgado;
  - no hay clave en el llavero (se importó un foco sin clave o se borró la entrada).
- **Arreglo:** primero cerrá las otras apps y reiniciá el foco con la tecla. Si sigue, tocá
  **Renovar clave**: la baja de Tuya con las credenciales guardadas y, si hace falta, reinicia el
  efecto. Si renovar falla con un error de la nube, mirá ese código.

### device_timeout

**«El foco no responde»**

- **Qué significa:** el foco no contestó a tiempo o cortó la conexión.
- **Por qué pasa:** apagado desde la tecla, señal de Wi-Fi débil, router reiniciándose, otra app
  ocupándolo o demasiados comandos por segundo.
- **Arreglo:** prendelo, acercalo al router y reintentá. Si se corta seguido, bajá
  **Comandos por segundo** en Ajustes y mirá el motivo del último corte en **Diagnóstico**.

### device_busy

**«Otra app está usando el foco»**

- **Qué significa:** el foco está tomado o rechazó la conexión.
- **Por qué pasa:** casi siempre es **la propia OpenLight**: querés usar el control manual,
  calibrar, identificar o probar un foco mientras el efecto (o una calibración) lo está usando.
  También puede ser otra app que ocupa el foco, como Smart Life.
- **Arreglo:** detené el efecto o cerrá la calibración y volvé a intentar. Si no había nada de
  eso andando, cerrá Smart Life y los otros programas.

### device_no_colour

**«Este dispositivo no muestra colores»**

- **Qué significa:** el dispositivo no tiene lo necesario para lo que se le pidió.
- **Por qué pasa:** es una luz solo blanca, no es una luz, o se pidió una escena que el foco no
  tiene.
- **Arreglo:** no se puede usar para seguir la pantalla. Quitalo y elegí un foco con color.

## Nube de Tuya

Estos errores solo aparecen al configurar, agregar focos o renovar claves. El uso diario no
pasa por la nube.

### cloud_invalid_signature

**«El Access Secret no coincide»**

- **Qué significa:** Tuya no aceptó la firma del pedido.
- **Por qué pasa:** el Access Secret o el Access ID están mal copiados, o son de otro proyecto.
- **Arreglo:** en iot.tuya.com, pestaña **Overview** del proyecto, copiá los dos con el botón de
  copiar y pegalos de nuevo.

### cloud_invalid_token

**«La sesión con Tuya venció»**

- **Qué significa:** Tuya rechazó el permiso temporal, incluso después de pedir uno nuevo.
- **Por qué pasa:** algo pasajero de Tuya o un Access ID equivocado.
- **Arreglo:** reintentá. Si sigue, revisá el Access ID.

### cloud_permission_denied

**«Tuya no da permiso»**

- **Qué significa:** las claves son válidas pero el proyecto no puede ver tus dispositivos.
- **Por qué pasa:** falta [vincular Smart Life](guia.md#5-vincular-smart-life) al proyecto, o el
  Data Center elegido no es el de tu cuenta.
- **Arreglo:** en el proyecto, **Devices → Link App Account**, y verificá que aparezcan tus
  focos. Revisá el [Data Center](guia.md#4-crear-el-proyecto-cloud).

### cloud_wrong_region

**«Data Center equivocado»**

- **Qué significa:** tus claves no funcionan en la región elegida, pero sí en otra. El mensaje
  dice en cuál (`us`, `us-e`, `eu`, `eu-w`, `in`, `sg` o `cn`).
- **Arreglo:** usá el botón que te ofrece esa región, o elegila en la lista y probá de nuevo.

### cloud_service_not_authorized

**«Falta un servicio en el proyecto»**

- **Qué significa:** el proyecto no tiene acceso a la API que OpenLight necesita.
- **Por qué pasa:** no se agregó **IoT Core** (u otro de los servicios propuestos) al crear el
  proyecto.
- **Arreglo:** en iot.tuya.com, abrí tu proyecto → **Service API** y agregá **IoT Core**.
  Esperá un minuto y reintentá.

### cloud_trial_expired

**«Venció la prueba de IoT Core»**

- **Qué significa:** el plan gratis del servicio venció o se agotó su cupo.
- **Por qué pasa:** la prueba de IoT Core dura un tiempo limitado.
- **Arreglo:** en iot.tuya.com, **Cloud → Cloud Services → IoT Core → Extend Trial Period** (es
  gratis; la aprobación suele ser automática). Tus focos siguen andando mientras tanto: esto solo
  hace falta para agregar focos o renovar claves.

### cloud_network

**«Sin conexión con Tuya»**

- **Qué significa:** el pedido no llegó a la API o no hubo respuesta.
- **Por qué pasa:** sin internet, un proxy, un firewall o un bloqueador, o Tuya caído.
- **Arreglo:** revisá la conexión y reintentá.

### cloud_no_credentials

**«Faltan las credenciales de Tuya»**

- **Qué significa:** hace falta la nube y no hay Access ID y Access Secret guardados.
- **Por qué pasa:** se llegó a agregar o renovar sin pasar por el paso de claves (por ejemplo,
  después de importar el script), o se borraron del llavero.
- **Arreglo:** completá el [paso 6 del asistente](guia.md#6-claves-del-proyecto).

## Configuración y otros

### no_zones

**«Ningún foco tiene zona»**

- **Qué significa:** no hay ningún foco que pueda seguir la pantalla.
- **Por qué pasa:** ningún foco tiene zona asignada, o están todos destildados en el menú del
  tray.
- **Arreglo:** elegí una zona para cada foco en **Dispositivos**, o tildalos en el tray.

### import_invalid

**«No pude importar esa carpeta»**

- **Qué significa:** en la carpeta no hay una configuración del script que se pueda usar.
- **Por qué pasa:** la ruta está mal, falta `devices.json` (o `config.json`), el archivo está
  dañado o no tiene ninguna luz.
- **Arreglo:** poné la carpeta del script, la que contiene `devices.json`. OpenLight solo la lee.

### invalid_setting

**«Valor no válido»**

- **Qué significa:** se mandó un dato que la app no acepta. El mensaje dice cuál.
- **Por qué pasa:** un ajuste fuera de rango, una IP mal escrita, un Access ID o Secret vacío, un
  nombre de perfil vacío o de más de 60 caracteres, demasiadas zonas, o una acción sobre una
  calibración que ya se cerró.
- **Arreglo:** corregí el valor y reintentá. Si pasa sin que hayas escrito nada, exportá el
  diagnóstico y [abrí un issue](https://github.com/matiasm0rales/openlight/issues).

### storage

**«No pude guardar la configuración»**

- **Qué significa:** falló una lectura o escritura de la configuración **o del llavero**.
- **Por qué pasa:**
  - disco lleno o sin permiso en la carpeta de datos;
  - un archivo de configuración dañado, o escrito por una versión más nueva de OpenLight (no se
    pisa);
  - el llavero no está disponible: bloqueado, acceso denegado, o en Linux no hay un servicio de
    claves (GNOME Keyring, KWallet);
  - en macOS, la entrada de OpenLight en el llavero está dañada o la escribió una versión más
    nueva (tampoco se pisa).
- **Arreglo:** liberá espacio, desbloqueá o instalá el llavero (en macOS, **Permitir siempre**
  cuando lo pida), o actualizá la app. El registro en **Diagnóstico** dice qué falló. Las
  carpetas están en la [guía](guia.md#dónde-se-guardan-las-cosas).

### not_implemented

**«Todavía no está disponible»**

- **Qué significa:** se llamó a una función que no está hecha.
- **Por qué pasa:** quedó de las primeras etapas del desarrollo; las versiones actuales no
  deberían mostrarlo.
- **Arreglo:** actualizá. Si lo ves igual, abrí un issue con el nombre de la función que muestra.

### internal

**«Algo salió mal»**

- **Qué significa:** un error que OpenLight no sabe clasificar.
- **Por qué pasa:** una respuesta inesperada del foco o de Tuya (por ejemplo, Tuya rechaza los
  pedidos si el reloj de la compu está muy corrido), un error de red del sistema, o que no se
  pudo abrir un panel de ajustes, activar el inicio automático o escribir el diagnóstico.
- **Arreglo:** revisá la fecha y hora, reintentá y, si sigue, **Diagnóstico → Exportar
  diagnóstico** y [abrí un issue](https://github.com/matiasm0rales/openlight/issues).

## Problemas que no muestran un código

| Síntoma | Qué hacer |
|---|---|
| Cambié un ajuste, el brillo, el perfil o la zona y no pasa nada | detené el efecto y volvé a arrancarlo |
| El atajo de un perfil no hace nada | los atajos todavía no están conectados |
| La prueba A/B no muestra diferencias | todavía no funciona: elegí el valor en Ajustes |
| macOS pide la contraseña del llavero después de actualizar o recompilar | la firma del programa cambió; elegí **Permitir siempre** en cada pedido |
| Los colores se ven entrecortados | probá otro **Canal del foco**, o bajá los comandos por segundo |
| Al despertar la compu los focos quedaron con un color | detené y arrancá el efecto; conviene detenerlo antes de suspender |
