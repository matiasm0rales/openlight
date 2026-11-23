# Setup legado FALSO

Copia **inventada** de la carpeta del script Python (`~/Documents/Open RGB`): la misma forma de
`devices.json`, `config.json`, `calibracion.json` y `foco_estado.json`, pero con ids, claves, MACs
e IPs de mentira (IPs del rango de documentación `192.0.2.0/24`). El repo es público: acá no
puede haber ningún valor real.

`devices.json` está en el `.gitignore` a propósito (para que nunca se suba el real), así que la
fixture se guarda como `devices.fake.json` y las pruebas la copian a una carpeta temporal con el
nombre que lee el importador.
