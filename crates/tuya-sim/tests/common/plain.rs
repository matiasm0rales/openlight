//! `Suite` de prueba SIN cripto real: framing trivial y una "autenticación" que solo detecta
//! si las dos puntas usan la misma clave. Sirve para probar la lógica del simulador mientras
//! `tuya-proto` (tarea A) no está mergeado. No se usa fuera de las pruebas.

use openlight_types::ProtocolVersion;
use tuya_sim::{Msg, Role, Suite, Wire, WireError};

const MAGIC: &[u8; 4] = b"PLN!";
const HEADER: usize = 4 + 4 + 4 + 1 + 4 + 4;

fn fnv(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in *p {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

pub struct PlainSuite;

struct PlainWire {
    key: [u8; 16],
}

impl Wire for PlainWire {
    fn decode(&mut self, buf: &mut Vec<u8>) -> Result<Option<Msg>, WireError> {
        let Some(start) = buf.windows(4).position(|w| w == MAGIC) else {
            let keep = buf.len().saturating_sub(3);
            buf.drain(..keep);
            return Ok(None);
        };
        buf.drain(..start);
        if buf.len() < HEADER {
            return Ok(None);
        }
        let u = |at: usize| u32::from_be_bytes(buf[at..at + 4].try_into().unwrap());
        let (seqno, cmd, has_ret, ret, len) = (u(4), u(8), buf[12] == 1, u(13), u(17) as usize);
        if buf.len() < HEADER + len + 4 {
            return Ok(None);
        }
        let payload = buf[HEADER..HEADER + len].to_vec();
        let tag = u32::from_be_bytes(buf[HEADER + len..HEADER + len + 4].try_into().unwrap());
        buf.drain(..HEADER + len + 4);
        if tag != fnv(&[&self.key, &payload]) as u32 {
            return Err(WireError::BadAuth);
        }
        Ok(Some(Msg {
            seqno,
            cmd,
            retcode: has_ret.then_some(ret),
            payload,
        }))
    }

    fn encode(&mut self, msg: &Msg, _with_header: bool) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend(msg.seqno.to_be_bytes());
        out.extend(msg.cmd.to_be_bytes());
        out.push(u8::from(msg.retcode.is_some()));
        out.extend(msg.retcode.unwrap_or(0).to_be_bytes());
        out.extend((msg.payload.len() as u32).to_be_bytes());
        out.extend(&msg.payload);
        out.extend((fnv(&[&self.key, &msg.payload]) as u32).to_be_bytes());
        out
    }

    fn set_key(&mut self, key: [u8; 16]) {
        self.key = key;
    }
}

impl Suite for PlainSuite {
    fn wire(&self, _v: ProtocolVersion, key: [u8; 16], _role: Role) -> Box<dyn Wire> {
        Box::new(PlainWire { key })
    }

    fn hmac_sha256(&self, key: &[u8], data: &[u8]) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, chunk) in out.chunks_mut(8).enumerate() {
            chunk.copy_from_slice(&fnv(&[key, data, &[i as u8]]).to_be_bytes());
        }
        out
    }

    fn session_key(&self, _v: ProtocolVersion, k: &[u8; 16], a: &[u8; 16], b: &[u8; 16]) -> [u8; 16] {
        std::array::from_fn(|i| k[i] ^ a[i] ^ b[i])
    }

    fn udp_encode(&self, _v: ProtocolVersion, payload: &[u8]) -> Vec<u8> {
        let mut out = b"UDP!".to_vec();
        out.extend(tuya_sim::cmd::UDP_NEW.to_be_bytes());
        out.extend(payload);
        out
    }

    fn udp_decode(&self, frame: &[u8]) -> Option<(u32, Vec<u8>)> {
        let rest = frame.strip_prefix(b"UDP!")?;
        let cmd = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?);
        Some((cmd, rest[4..].to_vec()))
    }
}

/// Frame UDP de prueba con un comando cualquiera (para mandar `REQ_DEVINFO`).
pub fn udp_frame(cmd: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = b"UDP!".to_vec();
    out.extend(cmd.to_be_bytes());
    out.extend(payload);
    out
}
