//! Framing y cripto de los protocolos locales de Tuya 3.1/3.3/3.4/3.5, sin IO.
//!
//! Todo es determinista: el tiempo (`t`), los nonces de la negociación y los IV de GCM se
//! reciben por parámetro. Cada detalle está verificado contra tinytuya 1.20
//! (`tinytuya/core/`) y las pruebas golden comparan byte a byte con `fixtures/proto`.
//!
//! # Piezas
//!
//! | módulo | qué hace | equivalente en tinytuya |
//! |---|---|---|
//! | [`frame`] | frames 55AA y 6699: empaquetar, leer encabezado, desarmar | `message_helper.py` |
//! | [`crypto`] | AES-ECB/PKCS7, AES-GCM, HMAC-SHA256, CRC32, firma MD5 de 3.1 | `crypto_helper.py` |
//! | [`payload`] | JSON de los comandos y remapeo por versión | `XenonDevice.generate_payload` |
//! | [`codec`] | cifrar lo que manda la app y descifrar lo que manda el foco | `_encode_message`, `_decode_payload` |
//! | [`negotiation`] | clave de sesión de 3.4/3.5 | `_negotiate_session_key_*` |
//! | [`stream`] | decodificador incremental con resincronización | `XenonDevice._receive` |
//! | [`device`] | el lado del foco (para `tuya-sim`) | `tools/golden/gen_proto.py` |
//! | [`udp`] | broadcasts de discovery y `REQ_DEVINFO` | `udp_helper.py`, `scanner.py` |
//!
//! # Ejemplo: sesión 3.4 del lado de la app
//!
//! ```
//! use openlight_types::ProtocolVersion;
//! use tuya_proto::{cmd, ClientSession, PayloadData, PayloadRequest, RetcodeMode, StreamDecoder};
//!
//! let local_key = *b"OpenLightKey0001";
//! let mut s = ClientSession::new(ProtocolVersion::V34, local_key);
//! // 1. nonce local: en producción, 16 bytes aleatorios.
//! let start = s.start_negotiation([7; 16]);
//! let frame = s.encode(&start, None).unwrap();
//! # let _ = frame;
//! // 2. con la respuesta del foco (cmd 4) desarmada con `s.frame_key()`:
//! // let finish = s.handle_negotiation_response(&respuesta)?;  // se manda finish
//! // 3. después, los comandos van con la clave de sesión:
//! let req = PayloadRequest::new(ProtocolVersion::V34, "dev-id", cmd::CONTROL, 1_795_392_000)
//!     .with_data(PayloadData::dps([("20", true)]));
//! let control = s.encode(&s.payload(&req), None).unwrap();
//! # let _ = control;
//! let mut dec = StreamDecoder::new();
//! dec.push(&[]);
//! assert!(dec.next_frame(s.frame_key().as_ref(), RetcodeMode::Present).is_none());
//! ```

pub mod codec;
pub mod consts;
pub mod crypto;
pub mod device;
mod error;
pub mod frame;
pub mod negotiation;
pub mod payload;
pub mod pyjson;
mod session;
pub mod stream;
pub mod udp;

pub use codec::{decode_payload, encode_message};
pub use consts::{NO_PROTOCOL_HEADER_CMDS, cmd, version_header};
pub use crypto::{Key, key_from_str};
pub use device::{DeviceMessage, decode_client_payload, encode_device_message};
pub use error::{ProtoError, Result};
pub use frame::{
    DecodedFrame, Header, Message, Prefix, RetcodeMode, pack_message, parse_header, unpack_message,
};
pub use negotiation::derive_session_key;
pub use openlight_types::ProtocolVersion;
pub use payload::{
    DevType, MessagePayload, OrderedDps, PayloadData, PayloadRequest, generate_payload,
};
pub use session::ClientSession;
pub use stream::{StreamDecoder, StreamError};
