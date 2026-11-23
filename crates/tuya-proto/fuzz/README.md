# Fuzz de `tuya-proto`

Un solo target, `decoder`, que ejercita todo lo que lee bytes de la red: `StreamDecoder` (con
pedazos de distinto tamaño), `unpack_message`, `decode_payload`, `decode_client_payload` y
`udp::decode_broadcast`. El primer byte de cada entrada elige versión, modo de retcode, si hay
clave y el tamaño de los pedazos.

Este crate está fuera del workspace (tiene su propio `[workspace]`) porque necesita nightly.

```bash
rustup toolchain install nightly      # solo para el fuzz
cargo install cargo-fuzz
cd crates/tuya-proto/fuzz
cargo +nightly fuzz run decoder -- -max_total_time=600
```

Criterio de aceptación de la tarea A: 10 minutos sin panics.
