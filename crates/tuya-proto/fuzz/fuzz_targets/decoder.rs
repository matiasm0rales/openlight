//! Fuzz de todo lo que lee bytes de la red: stream, frames, payloads y UDP.
//!
//! El primer byte elige versión, modo de retcode, clave y tamaño de los pedazos.
#![no_main]

use libfuzzer_sys::fuzz_target;
use tuya_proto::{
    DevType, ProtocolVersion, RetcodeMode, StreamDecoder, decode_client_payload, decode_payload, udp,
    unpack_message,
};

const KEY: [u8; 16] = *b"OpenLightKey0001";

fuzz_target!(|data: &[u8]| {
    let Some((&sel, rest)) = data.split_first() else { return };
    let version = ProtocolVersion::ALL[usize::from(sel & 3)];
    let mode = [RetcodeMode::Present, RetcodeMode::Absent, RetcodeMode::Detect][usize::from(sel >> 2 & 3) % 3];
    let key = (sel & 0x10 != 0).then_some(KEY);
    let dev_type = if sel & 0x20 != 0 { DevType::Device22 } else { DevType::Default };
    let chunk = usize::from(sel >> 6) * 7 + 1;

    let mut dec = StreamDecoder::new();
    for piece in rest.chunks(chunk) {
        dec.push(piece);
        while let Some(r) = dec.next_frame(key.as_ref(), mode) {
            if let Ok(f) = r {
                let _ = decode_payload(version, &KEY, dev_type, &f.payload);
                let _ = decode_client_payload(version, &KEY, &f.payload);
            }
        }
    }
    assert!(dec.buffered() <= rest.len());

    let _ = unpack_message(rest, key.as_ref(), mode);
    let _ = decode_payload(version, &KEY, dev_type, rest);
    let _ = decode_client_payload(version, &KEY, rest);
    let _ = udp::decode_broadcast(rest);
});
