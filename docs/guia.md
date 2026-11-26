# Guía de usuario

Cómo instalar OpenLight, configurarlo y usarlo todos los días. Si algo falla, buscá el código
del error en [errores frecuentes](errores.md).

- [Antes de empezar](#antes-de-empezar)
- [Instalar](#instalar)
- [Primer arranque](#primer-arranque)
- [El asistente, paso a paso](#el-asistente-paso-a-paso)
- [Uso diario](#uso-diario)
- [Ajustes](#ajustes)
- [Actualizaciones](#actualizaciones)
- [Dónde se guardan las cosas](#dónde-se-guardan-las-cosas)
- [Desinstalar](#desinstalar)

> **Plataformas:** OpenLight está probado con focos reales en **macOS**. En **Windows** y
> **Linux** compila y pasa las pruebas automáticas, pero todavía no se probó con hardware real:
> lo que sigue para esos sistemas puede no coincidir con lo que veas.

## Antes de empezar

Necesitás:

- tus focos ya agregados en la app **Smart Life** (o Tuya Smart) del celular, con color;
- la compu y los focos en la **misma red** (cuidado con las redes de invitados);
- el celular a mano, para escanear un QR;
- unos 15 minutos, una sola vez.

## Instalar

Bajá el instalador de la web:

> **[PENDIENTE: URL de la web de descarga]**

También van a estar en [GitHub Releases](https://github.com/matiasm0rales/openlight/releases).
Todavía no hay ninguna versión publicada; para probarla antes, hay que
[compilarla](../README.md#compilar-desde-el-código).

Los instaladores **no están firmados** por ahora. El sistema va a desconfiar la primera vez:
bajalos solo de la web de OpenLight o de GitHub Releases.

### macOS (12.3 o más nuevo)

1. Abrí el `.dmg` y arrastrá OpenLight a **Aplicaciones**.
2. Al abrirla, macOS va a decir que no puede verificar al desarrollador.
   - En macOS 15 o más nuevo: **Ajustes del Sistema → Privacidad y seguridad**, bajá hasta el
     aviso de OpenLight y tocá **Abrir igualmente**.
   - En versiones anteriores: clic derecho sobre OpenLight → **Abrir**.
3. Si dice que la app "está dañada", es la marca de descarga. Se saca con:

```bash
xattr -dr com.apple.quarantine /Applications/OpenLight.app
```

### Windows

1. Ejecutá `-setup.exe` (instala solo para tu usuario, sin pedir administrador) o el `.msi`.
2. Si aparece "Windows protegió su PC": **Más información → Ejecutar de todas formas**.
3. La primera vez que busque focos, Windows pregunta por el firewall: permitilo en **redes
   privadas**. Si lo cancelaste, los focos no van a aparecer; mirá
   [`firewall_blocked`](errores.md#firewall_blocked).

### Linux

| Formato | Cómo se instala |
|---|---|
| `.deb` (Debian, Ubuntu) | `sudo apt install ./<archivo>.deb` |
| `.rpm` (Fedora y similares) | `sudo dnf install ./<archivo>.rpm` |
| `.AppImage` | `chmod +x <archivo>.AppImage` y ejecutalo (necesita FUSE 2) |

La captura de pantalla usa PipeWire en Wayland. Para guardar las claves hace falta un llavero del
sistema (GNOME Keyring, KWallet u otro compatible con *Secret Service*).

## Primer arranque

- La app abre el **asistente**. El idioma es español, salvo que el sistema esté en inglés; lo
  cambiás con el selector al pie de la barra de pasos.
- OpenLight deja un ícono en la barra de menú (macOS) o en la bandeja del sistema (Windows y
  Linux). **Cerrar la ventana no cierra la app**: sigue ahí. La ventana se libera (así ocupa menos
  memoria) y al volver a abrirla aparece donde estaba, en el Panel.
- Hay una sola instancia: abrirla de nuevo trae la ventana al frente.

## El asistente, paso a paso

Son 11 pasos. El progreso se guarda: podés volver atrás, tocar **Seguir después** y retomar. Cada
paso habilita **Siguiente** cuando está completo.

### 1. Bienvenida

Explica qué vas a necesitar.

**¿Ya usabas `ambilight.py`?** Tocá **Ya tengo devices.json** y poné la carpeta del script.
OpenLight lee `devices.json`, `config.json`, `calibracion.json` y `foco_estado.json` sin
modificar nada: las claves pasan al llavero y la calibración, a su foco. Revisá los avisos del
resultado (por ejemplo, si tuvo que asumir la versión 3.3) y tocá **Seguir con las zonas**.
Volver a importar es seguro: conserva tus zonas y ajustes.

### 2. Permisos del sistema

El estado se vuelve a revisar solo cada pocos segundos.

| Sistema | Qué pide |
|---|---|
| macOS | **Grabación de pantalla** y **Red local**. Después de activar la grabación de pantalla hay que **reabrir la app**: lo exige macOS. |
| Windows | permitir OpenLight en el **firewall** (redes privadas). |
| Linux con Wayland | el escritorio pregunta qué pantalla compartir cuando empieza la captura. |
| Linux con X11 | nada. |

La imagen de tu pantalla se procesa en tu compu y no se manda a ningún lado.

En macOS, si la grabación de pantalla figura activada pero OpenLight no ve nada (puede pasar después
de actualizar), tocá **Reparar permiso**, volvé a activarla y reabrí la app
([detalle](errores.md#screen_permission_stale)).

### 3. Cuenta en Tuya Developer

Cada foco tiene una **clave local** que solo se puede leer desde el portal de Tuya. OpenLight la
lee una vez; después todo funciona sin nube.

1. Entrá a [iot.tuya.com](https://iot.tuya.com/) y tocá **Sign Up**.
2. Confirmá el código que te llega por mail.
3. Si pregunta el tipo de cuenta, elegí **Individual developer**.
4. Marcá **Ya tengo cuenta en iot.tuya.com**.

El portal está en inglés y cambia seguido: si un botón no se llama igual, buscá uno parecido.

### 4. Crear el proyecto Cloud

1. **Cloud → Development → Create Cloud Project**.
2. Nombre: el que quieras. **Industry** y **Development Method**: *Smart Home*.
3. **Data Center**: el que corresponde a tu cuenta de Smart Life (se ve en la app, en
   *Yo → Ajustes → Cuenta y seguridad → Región*).

   | Dónde creaste la cuenta | Data Center |
   |---|---|
   | Argentina, Uruguay, Chile, México y el resto de América | Western America |
   | Estados Unidos (costa este) | Eastern America |
   | España, Alemania, Italia y casi toda Europa | Central Europe |
   | Reino Unido, Francia y Europa occidental | Western Europe |
   | India | India |
   | China | China |
   | Sudeste asiático | Singapur |

4. Aceptá los servicios que propone. Tiene que estar **IoT Core**.

IoT Core viene con una prueba gratis que vence cada unos meses. Solo hace falta para agregar
focos o renovar claves, y se extiende gratis (mirá
[`cloud_trial_expired`](errores.md#cloud_trial_expired)).

### 5. Vincular Smart Life

1. En el proyecto: pestaña **Devices → Link App Account → Add App Account**. Aparece un QR.
2. En el celular: **Smart Life → Yo →** ícono de escanear (arriba a la derecha).
3. Escaneá y confirmá. En el portal aparecen tu cuenta y tus focos.

### 6. Claves del proyecto

En la pestaña **Overview** del proyecto están el **Access ID** (unos 20 caracteres) y el
**Access Secret** (32). Copialos con el botón de copiar, pegalos, elegí el Data Center y tocá
**Probar**. Si el Data Center no es el correcto, OpenLight prueba los demás y te ofrece usar el
que funciona. Las claves se guardan en el llavero del sistema.

### 7. Elegir los focos

Aparecen los dispositivos de tu cuenta, cruzados con lo que se ve en tu red ("En tu red").
Marcá los que querés que sigan la pantalla. Las luces sin color y los dispositivos que no son
luces no se pueden elegir. Si un foco dice "No aparece en tu red", prendelo desde la tecla,
cerrá Smart Life y tocá **Buscar de nuevo**.

Al tocar **Siguiente** se agregan los focos marcados y se quitan los que destildaste.

### 8. Probar cada foco

**Probar rojo, verde y azul** muestra cada color un segundo y medio y devuelve el foco a como
estaba. Si los viste, tocá **Sí, los vi**. Si no, la app repasa con vos lo habitual: la misma
Wi-Fi, Smart Life cerrada, el foco prendido desde la tecla y la clave al día. También podés
hacerlo parpadear o quitarlo.

### 9. Monitor y zonas

Elegí el monitor y qué parte sigue cada foco: **Toda**, **Izquierda**, **Derecha**, **Arriba**,
**Abajo**, una de las cuatro esquinas (**Arriba izquierda**, **Arriba derecha**, **Abajo
izquierda**, **Abajo derecha**), **Centro**, o arrastrá el rectángulo. Con el teclado: flechas
para moverlo, Mayús + flechas para el tamaño y Alt para pasos finos.

Un foco nuevo arranca siguiendo todo el primer monitor externo (o el integrado, si no hay otro).
Con dos focos, izquierda y derecha queda muy bien.

Para repartir el monitor entre todos de una vez, usá **Distribución para todos los focos**. Solo
aparecen las que sirven para la cantidad que tenés:

| Focos | Distribuciones |
| --- | --- |
| 1 | Pantalla completa |
| 2 | Izquierda y derecha · Arriba y abajo |
| 3 | Una arriba y dos abajo · Tres columnas |
| 4 | Cuatro esquinas |

Los lugares se reparten en el orden de los focos en el Panel (los números de cada miniatura). Se
cambia la zona de cada foco en el monitor elegido; las de otros monitores no se tocan. Después
podés ajustar cualquiera a mano. Con cinco focos o más, las zonas se eligen una por una.

### 10. Calibrar (recomendado)

Cada foco interpreta los colores a su manera. La calibración muestra un color a pantalla completa
y vos movés el foco hasta que se parezca.

- **12 tonos**, o **modo rápido** con 6.
- Tono: hasta ±90°. Saturación: de 30 % a 150 %.
- Teclas: ← → tono, ↑ ↓ saturación, Enter pasa al siguiente.
- **Guardar y salir** guarda la tabla de ese foco; **Salir sin guardar** la descarta. En los dos
  casos el foco vuelve a como estaba.

Calibrá con la luz de la habitación como la usás normalmente. Podés saltear este paso y hacerlo
después desde el panel.

> La **prueba A/B** de la pantalla de calibración todavía no sirve para comparar: los cambios que
> prueba no llegan al foco con el efecto andando.

### 11. Listo

Elegí si querés **Abrir OpenLight al iniciar sesión** (arranca en el tray, sin abrir la ventana)
y tocá **Arrancar**.

## Uso diario

### Panel

- El **interruptor grande** arranca y detiene el efecto.
- **Perfil** elige qué conjunto de ajustes usar.
- Cada foco tiene su tarjeta: estado, color de la pantalla y color enviado, latencia, cortes en
  la última hora, motivo del último corte y un control de **brillo** (es el brillo máximo propio
  de ese foco; no baja de su brillo mínimo ni del 5 %).
- **Sigue la pantalla** saca un foco del efecto o lo vuelve a incluir; los demás no se cortan.
  Es lo mismo que tildarlo en el menú del tray, y OpenLight lo recuerda.
- Con un foco fuera del efecto, la tarjeta tiene **Blanco** y **Apagar** para usarlo aparte.
- Desde la tarjeta vas a **Control manual**, **Calibrar** y **Ajustes del foco**.

> Los cambios de ajustes, brillo, perfil y zonas **valen al instante**, también con el efecto
> andando. Cambiar los cuadros por segundo o de monitor rearma la captura sin desconectar los
> focos.

Con el efecto andando:

- Si no hay pantalla durante un rato (monitor apagado o desconectado), los focos **vuelven a como
  estaban** y siguen solos cuando la pantalla vuelve.
- Si un foco se corta, OpenLight **reconecta solo**; los demás no se frenan.
- Al **detener** o **salir**, cada foco vuelve al estado que tenía antes de empezar.

Antes de suspender la compu conviene detener el efecto: la app todavía no se entera de que el
sistema se va a dormir.

### Menú del tray

| Ítem | Qué hace |
|---|---|
| Estado | si el efecto está apagado, conectando, activo, en pausa o restaurando |
| Arrancar / Detener | lo mismo que el interruptor del panel |
| Perfil | elegir un perfil, o ninguno |
| Focos | tildar o destildar cada foco del efecto (vale al instante) |
| Abrir OpenLight | trae la ventana |
| Salir (restaura los focos) | devuelve los focos a su estado y cierra la app |

En macOS el menú se abre con un clic en el ícono. En Windows, el clic izquierdo abre la ventana y
el derecho el menú.

### Control manual

Encendido, color (con colores rápidos), blanco con temperatura y brillo. Si el foco está
siguiendo la pantalla, primero tocá **Sacar este foco del efecto**: los demás siguen. El foco
queda como lo dejaste y, cuando quieras, lo volvés a incluir. Las escenas propias del foco se
eligen desde Smart Life.

Lo mismo vale para **Calibrar** y para **Identificar**: si el foco está en el efecto, el aviso
[`device_in_effect`](errores.md#device_in_effect) ofrece sacarlo.

Para apagar un foco usá **Apagar** (fuera del efecto): el brillo máximo en lo más bajo es un
5 %, no lo apaga.

### Dispositivos

- **Agregar focos** vuelve al asistente, en el paso de elegir focos.
- **Editar**: nombre y dirección IP (automática o fija). La IP fija sirve si la búsqueda en la
  red no funciona; reservala en tu router para que no cambie.
- **Renovar clave**: la vuelve a bajar de Tuya con las claves guardadas. Hace falta si volviste a
  vincular el foco en Smart Life.
- **Identificar** lo hace parpadear. **Protocolo y DPs** muestra lo que detectó la app.
- **Quitar** borra su clave del llavero y su calibración.
- Abajo se editan las **zonas**.

### Diagnóstico

Gráficos de capturas y envíos por segundo, salud de cada enlace (estado, latencia, tráfico,
reconexiones y motivo del último corte) y el registro de la app, con filtros.

**Exportar diagnóstico** guarda un `.zip` en tu carpeta de descargas, para adjuntar a un issue.
No incluye claves ni el Access Secret. Sí incluye nombres, ids e IPs locales de tus focos:
miralo antes de publicarlo.

## Ajustes

Hay tres niveles: **Básico**, **Avanzado** y **Experto**. Cada ajuste se puede restaurar a su
valor por defecto, y se aplica a uno de estos ámbitos:

- **Todos los focos** (global);
- **un perfil**, que se usa mientras está activo;
- **un foco**.

Si el mismo ajuste está en varios, gana el más específico: foco, después perfil activo, después
global y por último el valor por defecto. En **Todos los focos** se ve el valor global, con un
aviso cuando el perfil activo o algún foco usan el suyo (ahí el cambio no se nota). Por cada foco
con valor propio aparece **Usar el valor de todos**, que lo borra. En un foco, cada ajuste dice si
es un **Valor propio** (y cuál usaría sin él) o si viene **De todos los focos** o del perfil activo.

El brillo máximo no puede quedar por debajo del brillo mínimo. Si al combinar un perfil y los
valores de un foco el mínimo quedara arriba, manda el máximo.

<!-- ajustes:inicio (generado con `python3 tools/gen_ajustes.py`; no editar a mano) -->

### Nivel básico

| Ajuste | Qué hace | Por defecto | Valores |
|---|---|---|---|
| **Modo de color** (`color_mode`) | Dominante: el color que más se ve en la zona. Promedio: la mezcla de toda la zona. | Dominante | Dominante, Promedio |
| **Realce de colores** (`saturation`) | Multiplica la saturación. Más alto, colores más intensos. | ×1,30 | ×0,50 a ×2,50 |
| **Brillo mínimo** (`brightness_min`) | Brillo del foco cuando la pantalla está oscura. | 5 % | 0 % a 100 % |
| **Brillo máximo** (`brightness_max`) | Tope de brillo del foco con la pantalla bien clara. No apaga el foco: para eso usá «Sacar del efecto» o «Apagar». | 100 % | 5 % a 100 % |
| **Suavizado** (`smoothing_ms`) | Más alto, transiciones más lentas y sin parpadeo. Los cortes de escena siempre van más rápido. | 300 ms | 0 ms a 2000 ms |

### Nivel avanzado

| Ajuste | Qué hace | Por defecto | Valores |
|---|---|---|---|
| **Canal del foco** (`send_channel`) | Por dónde se le manda el color. Si los colores se ven entrecortados, probá el canal de control. | Color (normal) | Color (normal), Control directo, Control con transición |
| **Capturas por segundo** (`capture_fps`) | Cuántas veces por segundo se mira la pantalla. Más alto, más rápido pero más consumo. | 20 fps | 5 fps a 60 fps |
| **Comandos por segundo** (`max_sends_per_s`) | Máximo de colores que se le mandan al foco por segundo. Bajalo si el foco se traba. | 8/s | 1/s a 20/s |
| **Umbral de envío** (`send_threshold`) | Los cambios más chicos que esto no se mandan, porque no se notan. | 1,0 % | 0,0 % a 10,0 % |
| **Bandas negras** (`black_bar`) | Las filas y columnas más oscuras que esto se toman como bandas negras y no cuentan para el brillo. | 6 % | 0 % a 30 % |
| **Umbral de gris** (`grey_threshold`) | Con menos saturación que esto, la pantalla se considera gris y el foco da luz blanca. | 2,0 % | 0,0 % a 20,0 % |
| **Pantalla oscura** (`dark_screen`) | Por debajo de este brillo se conserva el último color en vez de pasar a blanco. | 12 % | 0 % a 50 % |
| **Estabilidad del tono** (`hue_stickiness`) | Evita que el foco salte entre dos colores que ocupan casi lo mismo de la pantalla. | ×1,40 | ×1,00 a ×3,00 |
| **Espera sin pantalla** (`no_screen_wait_s`) | Si no hay pantalla durante este tiempo, el foco vuelve a como estaba hasta que la pantalla vuelva. | 20 s | 5 s a 300 s |

### Nivel experto

| Ajuste | Qué hace | Por defecto | Valores |
|---|---|---|---|
| **Señal de vida** (`heartbeat_s`) | Cada cuánto se le avisa al foco que la conexión sigue viva. | 10 s | 3 s a 30 s |
| **Conexión muerta tras** (`dead_after_s`) | Sin ninguna respuesta del foco durante este tiempo, se reconecta. | 30 s | 5 s a 120 s |
| **Reintentar cada** (`retry_every_s`) | Tiempo entre intentos de reconexión. | 3 s | 1 s a 30 s |

<!-- ajustes:fin -->

### Perfiles

Un perfil guarda un conjunto de valores. Vienen tres de fábrica; podés editar sus valores, pero
no borrarlos ni renombrarlos. Los propios se crean, duplican, renombran y borran.

| Perfil de fábrica | Qué cambia |
|---|---|
| Películas | suavizado 500 ms, realce ×1,40, brillo máximo 80 % |
| Juegos | suavizado 120 ms, 30 capturas y 10 comandos por segundo |
| Trabajo | realce ×1,00, brillo máximo 50 %, suavizado 800 ms |

A cada perfil se le puede asignar un atajo; por ahora se guarda y se muestra, pero **todavía no
funciona como atajo de teclado**.

## Actualizaciones

La idea es que OpenLight se actualice sola desde GitHub Releases. **Esa función todavía no está
disponible** y no hay versiones publicadas.

Para actualizar a mano, salí de la app e instalá la versión nueva encima: la configuración, la
calibración y las claves se conservan.

En macOS, si la versión nueva viene firmada distinto que la anterior, el permiso de **Grabación de
pantalla** deja de servir aunque figure activado: tocá **Reparar permiso**
([detalle](errores.md#screen_permission_stale)). También puede volver a pedir la contraseña del
**llavero**: elegí **Permitir siempre**. Las versiones firmadas siempre con el mismo certificado
conservan el permiso ([cómo se firman](release.md#macos-con-qué-se-firma)).

## Dónde se guardan las cosas

Las **claves** (la de cada foco, el Access ID y el Access Secret) van al llavero del sistema,
bajo el servicio `openlight`: Acceso a Llaveros en macOS, Administrador de credenciales en Windows
y el llavero del escritorio en Linux. En macOS van todas juntas en una sola entrada; en Windows y
Linux, una por clave. Nunca se escriben en un archivo.

El resto (focos, zonas, ajustes, perfiles y calibración) son archivos JSON:

| Sistema | Datos | Registros |
|---|---|---|
| macOS | `~/Library/Application Support/io.github.matiasm0rales.openlight` | `~/Library/Logs/io.github.matiasm0rales.openlight` |
| Windows | `%APPDATA%\io.github.matiasm0rales.openlight` | `%LOCALAPPDATA%\io.github.matiasm0rales.openlight\logs` |
| Linux | `~/.local/share/io.github.matiasm0rales.openlight` | `~/.local/share/io.github.matiasm0rales.openlight/logs` |

Se guardan los registros de los últimos 7 días.

## Desinstalar

1. **Apagá el inicio automático**, si lo activaste. El interruptor está en el último paso del
   asistente; también podés sacarlo desde el sistema (macOS: *Ajustes del Sistema → General →
   Ítems de inicio*; Windows: *Administrador de tareas → Aplicaciones de arranque*; Linux: el
   archivo de OpenLight en `~/.config/autostart`).
2. Si querés que se borren las claves de los focos, quitá cada foco en **Dispositivos**.
3. Elegí **Salir** en el menú del tray: los focos vuelven a su estado.
4. Desinstalá la app:
   - macOS: arrastrá OpenLight de Aplicaciones a la Papelera;
   - Windows: *Configuración → Aplicaciones → Aplicaciones instaladas → OpenLight → Desinstalar*;
   - Linux: `sudo apt remove` o `sudo dnf remove` del paquete, o borrá el `.AppImage`.
5. Borrá las carpetas de [datos y registros](#dónde-se-guardan-las-cosas).
6. En el llavero del sistema, borrá las entradas del servicio `openlight` que queden. El Access
   ID y el Access Secret no se borran desde la app (en macOS están en la entrada única).
7. Opcional: en iot.tuya.com podés desvincular la cuenta de Smart Life o borrar el proyecto
   Cloud, y en macOS quitar OpenLight de las listas de Privacidad y seguridad.
