//! Pruebas golden: encode y decode coinciden byte a byte con `fixtures/proto`, generadas con
//! tinytuya 1.20 por `tools/golden/gen_proto.py` (formato en `tools/golden/README.md`).

use std::path::PathBuf;

use serde_json::Value;
use tuya_proto::udp::{BroadcastKind, decode_broadcast, encode_broadcast, encode_req_devinfo};
use tuya_proto::{
    ClientSession, DeviceMessage, MessagePayload, PayloadData, PayloadRequest, ProtocolVersion,
    RetcodeMode, StreamDecoder, cmd, decode_client_payload, encode_device_message, key_from_str,
    unpack_message,
};

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/proto")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn hex(v: &Value) -> Vec<u8> {
    hex::decode(v.as_str().unwrap()).unwrap()
}

fn arr<const N: usize>(v: &Value) -> [u8; N] {
    hex(v).try_into().unwrap()
}

fn plaintext(f: &Value) -> Vec<u8> {
    let p = &f["plaintext"];
    match p.get("utf8") {
        Some(s) => s.as_str().unwrap().as_bytes().to_vec(),
        None => hex(&p["hex"]),
    }
}

fn version(f: &Value) -> ProtocolVersion {
    ProtocolVersion::parse(f["version"].as_str().unwrap()).unwrap()
}

/// Los pedidos de `fixture_version` en `gen_proto.py`, con los mismos argumentos de
/// `generate_payload`.
fn requests<'a>(
    v: ProtocolVersion,
    dev_id: &'a str,
    t: u64,
) -> Vec<(&'static str, PayloadRequest<'a>)> {
    let r = |command| PayloadRequest::new(v, dev_id, command, t);
    vec![
        ("dp_query", r(cmd::DP_QUERY)),
        (
            "control_switch_mode",
            r(cmd::CONTROL).with_data(PayloadData::dps([
                ("20", Value::from(true)),
                ("21", Value::from("colour")),
            ])),
        ),
        (
            "control_colour_v2",
            r(cmd::CONTROL).with_data(PayloadData::dps([("24", "00f003e801f4")])),
        ),
        (
            "control_control_data",
            r(cmd::CONTROL).with_data(PayloadData::dps([("28", "100f003e801f400000000")])),
        ),
        ("heart_beat", r(cmd::HEART_BEAT)),
        (
            "update_dps",
            r(cmd::UPDATEDPS).with_data(PayloadData::Value(serde_json::json!([18, 19, 20]))),
        ),
    ]
}

fn check_version(name: &str) {
    let fx = fixture(name);
    let v = version(&fx);
    let key = key_from_str(fx["local_key"].as_str().unwrap()).unwrap();
    let dev_id = fx["dev_id"].as_str().unwrap();
    let t = fx["t"].as_u64().unwrap();
    // IVs de GCM que consumió tinytuya, en orden (el primero, en 3.4+, es el nonce local).
    let mut randoms: Vec<Vec<u8>> = fx["urandom_used_hex"]
        .as_array()
        .unwrap()
        .iter()
        .map(hex)
        .collect();
    randoms.reverse();
    let first_random = fx.get("negotiation").map(|_| randoms.pop().unwrap());
    let mut next_iv = || -> Option<[u8; 12]> {
        if v == ProtocolVersion::V35 {
            randoms.pop().map(|r| r.try_into().unwrap())
        } else {
            None
        }
    };

    let mut s = ClientSession::new(v, key);

    if let Some(neg) = fx.get("negotiation") {
        let frames = neg["frames"].as_array().unwrap();
        let local_nonce: [u8; 16] = arr(&neg["local_nonce_hex"]);
        let remote_nonce: [u8; 16] = arr(&neg["remote_nonce_hex"]);
        assert_eq!(
            first_random.as_deref(),
            Some(&local_nonce[..]),
            "{name}: el primer aleatorio es el nonce"
        );

        let start = s.start_negotiation(local_nonce);
        assert_eq!(start.payload, plaintext(&frames[0]));
        let f1 = s.encode(&start, next_iv()).unwrap();
        assert_eq!(
            hex::encode(&f1),
            frames[0]["frame_hex"].as_str().unwrap(),
            "{name}: paso 1"
        );

        // Respuesta del foco: el lado foco la arma igual, y la app la desarma.
        let resp_hex = hex(&frames[1]["frame_hex"]);
        let body = tuya_proto::negotiation::device_step2_body(&key, &local_nonce, &remote_nonce);
        assert_eq!(body, plaintext(&frames[1]));
        let device_iv = neg["device_iv_hex"]
            .as_str()
            .map(|h| hex::decode(h).unwrap().try_into().unwrap());
        let armado = encode_device_message(
            v,
            &key,
            &DeviceMessage {
                seqno: 1,
                cmd: cmd::SESS_KEY_NEG_RESP,
                retcode: 0,
                body: &body,
                with_header: false,
            },
            device_iv,
        )
        .unwrap();
        assert_eq!(
            hex::encode(&armado),
            hex::encode(&resp_hex),
            "{name}: paso 2 del foco"
        );
        let resp = unpack_message(&resp_hex, Some(&key), RetcodeMode::Present).unwrap();
        assert!(resp.crc_good);

        let finish = s.handle_negotiation_response(&resp).unwrap();
        assert_eq!(finish.payload, plaintext(&frames[2]));
        let f3 = s.encode(&finish, next_iv()).unwrap();
        assert_eq!(
            hex::encode(&f3),
            frames[2]["frame_hex"].as_str().unwrap(),
            "{name}: paso 3"
        );
        assert!(tuya_proto::negotiation::device_verify_finish(
            &key,
            &remote_nonce,
            &finish.payload
        ));

        let session_key = s.finish_negotiation().unwrap();
        assert_eq!(
            hex::encode(session_key),
            neg["session_key_hex"].as_str().unwrap(),
            "{name}: clave de sesión"
        );
        assert_eq!(
            tuya_proto::derive_session_key(v, &key, &local_nonce, &remote_nonce),
            session_key
        );
    }

    // Frames de la app.
    let client = fx["client_frames"].as_array().unwrap();
    let reqs = requests(v, dev_id, t);
    assert_eq!(client.len(), reqs.len());
    for (f, (req_name, req)) in client.iter().zip(&reqs) {
        assert_eq!(f["name"], *req_name);
        let payload: MessagePayload = s.payload(req);
        assert_eq!(
            payload.cmd,
            f["cmd"].as_u64().unwrap() as u32,
            "{name}/{req_name}: comando"
        );
        assert_eq!(
            String::from_utf8(payload.payload.clone()).unwrap(),
            String::from_utf8(plaintext(f)).unwrap()
        );
        assert_eq!(s.seqno(), f["seqno"].as_u64().unwrap() as u32);
        assert_eq!(hex::encode(s.key()), f["key_hex"].as_str().unwrap());
        let frame = s.encode(&payload, next_iv()).unwrap();
        assert_eq!(
            hex::encode(&frame),
            f["frame_hex"].as_str().unwrap(),
            "{name}/{req_name}: frame"
        );

        // Lado foco: desarmar y descifrar lo que mandó la app.
        let d = unpack_message(&frame, s.frame_key().as_ref(), RetcodeMode::Absent).unwrap();
        assert!(d.crc_good);
        let plain = decode_client_payload(v, s.key(), &d.payload).unwrap();
        assert_eq!(
            plain,
            plaintext(f),
            "{name}/{req_name}: descifrado del lado foco"
        );
    }
    assert!(next_iv().is_none(), "{name}: sobran aleatorios");

    // Frames del foco: armar igual y decodificar como tinytuya.
    let mut stream = StreamDecoder::new();
    let device = fx["device_frames"].as_array().unwrap();
    for f in device {
        let fname = f["name"].as_str().unwrap();
        let expected = hex(&f["frame_hex"]);
        let plain = plaintext(f);
        let header = tuya_proto::version_header(v);
        let with_header = v != ProtocolVersion::V31 && plain.starts_with(&header);
        let body = if with_header {
            &plain[header.len()..]
        } else {
            &plain[..]
        };
        let device_iv = (v == ProtocolVersion::V35).then(|| expected[18..30].try_into().unwrap());
        let armado = encode_device_message(
            v,
            s.key(),
            &DeviceMessage {
                seqno: f["seqno"].as_u64().unwrap() as u32,
                cmd: f["cmd"].as_u64().unwrap() as u32,
                retcode: f["retcode"].as_u64().unwrap() as u32,
                body,
                with_header,
            },
            device_iv,
        )
        .unwrap();
        assert_eq!(
            hex::encode(&armado),
            hex::encode(&expected),
            "{name}/{fname}: frame del foco"
        );

        // Decodificar en pedazos de 5 bytes, todos los frames en el mismo stream.
        let mut got = None;
        for chunk in expected.chunks(5) {
            stream.push(chunk);
            if let Some(r) = stream.next_frame(s.frame_key().as_ref(), RetcodeMode::Present) {
                got = Some(r.unwrap());
            }
        }
        let d = got.expect("frame completo");
        assert_eq!(d.seqno, f["seqno"].as_u64().unwrap() as u32);
        assert_eq!(d.cmd, f["cmd"].as_u64().unwrap() as u32);
        assert_eq!(d.retcode, 0);
        let decoded = s.decode(&d).unwrap();
        let expected_decoded = if f["decoded"].is_null() {
            None
        } else {
            Some(f["decoded"].clone())
        };
        assert_eq!(decoded, expected_decoded, "{name}/{fname}: decoded");
    }
    assert_eq!(stream.buffered(), 0);
}

#[test]
fn golden_v31() {
    check_version("v31.json");
}

#[test]
fn golden_v33() {
    check_version("v33.json");
}

#[test]
fn golden_v34() {
    check_version("v34.json");
}

#[test]
fn golden_v35() {
    check_version("v35.json");
}

#[test]
fn golden_udp() {
    let fx = fixture("udp.json");
    assert_eq!(
        hex::encode(tuya_proto::udp::UDP_KEY),
        fx["udpkey_hex"].as_str().unwrap()
    );
    let randoms: Vec<Vec<u8>> = fx["urandom_used_hex"]
        .as_array()
        .unwrap()
        .iter()
        .map(hex)
        .collect();
    for case in fx["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let frame = hex(&case["frame_hex"]);
        match name {
            "req_devinfo_7000" => {
                let armado =
                    encode_req_devinfo("192.0.2.20", randoms[0].clone().try_into().unwrap());
                assert_eq!(hex::encode(&armado), hex::encode(&frame), "{name}");
                let v = decode_broadcast(&frame).unwrap();
                let plain: Value = serde_json::from_slice(&plaintext(case)).unwrap();
                assert_eq!(v, plain);
            }
            _ => {
                let decoded = decode_broadcast(&frame).unwrap();
                assert_eq!(decoded, case["decoded"], "{name}");
                // Armar el mismo broadcast del lado foco: JSON compacto de `decoded`, en el
                // orden de la fixture (que es el de gen_proto).
                let json = tuya_proto::pyjson::dumps_compact(&ordered(&case["decoded"]));
                let kind = match case["port"].as_u64().unwrap() {
                    6666 => BroadcastKind::Plain6666,
                    6667 => BroadcastKind::Encrypted6667,
                    _ => BroadcastKind::Gcm7000,
                };
                assert_eq!(kind.port() as u64, case["port"].as_u64().unwrap());
                let iv =
                    (kind == BroadcastKind::Gcm7000).then(|| frame[18..30].try_into().unwrap());
                let armado = encode_broadcast(kind, json.as_bytes(), iv);
                assert_eq!(hex::encode(&armado), hex::encode(&frame), "{name}: armado");
            }
        }
    }
}

/// `serde_json::Map` ordena las claves; para armar el broadcast se usa el orden de `info` en
/// `gen_proto.py`.
fn ordered(d: &Value) -> tuya_proto::pyjson::Json {
    let keys = [
        "ip",
        "gwId",
        "active",
        "ablilityCode",
        "encrypt",
        "productKey",
        "version",
    ];
    assert_eq!(d.as_object().unwrap().len(), keys.len());
    tuya_proto::pyjson::Json::Object(
        keys.iter()
            .map(|k| ((*k).to_owned(), d[*k].clone().into()))
            .collect(),
    )
}
