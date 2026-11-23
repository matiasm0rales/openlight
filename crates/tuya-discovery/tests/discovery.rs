//! Discovery con los broadcasts de `fixtures/proto/udp.json` (generados con tinytuya 1.20).

use std::net::{IpAddr, Ipv4Addr, UdpSocket as StdUdp};
use std::path::PathBuf;
use std::time::Duration;

use openlight_types::ProtocolVersion;
use serde_json::Value;
use tuya_discovery::{Datagram, ScanConfig, parse_datagram, scan};

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const SRC: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10));

fn udp_fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/proto/udp.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn case(name: &str) -> Value {
    udp_fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("falta el caso {name}"))
        .clone()
}

fn frame(name: &str) -> Vec<u8> {
    hex::decode(case(name)["frame_hex"].as_str().unwrap()).unwrap()
}

/// Un puerto libre en localhost (se suelta enseguida para que lo use el escáner).
fn free_port() -> u16 {
    StdUdp::bind((LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn descifra_los_tres_broadcasts() {
    for (name, version) in [
        ("broadcast_6666_v31", ProtocolVersion::V31),
        ("broadcast_6667_v33", ProtocolVersion::V33),
        ("broadcast_7000_v35", ProtocolVersion::V35),
    ] {
        let decoded = case(name)["decoded"].clone();
        let Datagram::Device(d) = parse_datagram(&frame(name), SRC) else {
            panic!("{name}: no se reconoció el foco");
        };
        assert_eq!(d.id.0, decoded["gwId"].as_str().unwrap(), "{name}");
        assert_eq!(d.version, version, "{name}");
        assert_eq!(
            d.product_key.as_deref(),
            decoded["productKey"].as_str(),
            "{name}"
        );
        assert_eq!(d.ip, "192.0.2.10");
    }
}

#[test]
fn el_pedido_de_la_app_se_ignora() {
    assert_eq!(
        parse_datagram(&frame("req_devinfo_7000"), SRC),
        Datagram::App
    );
}

#[test]
fn un_byte_cambiado_invalida_el_gcm() {
    let mut f = frame("broadcast_7000_v35");
    let mid = f.len() / 2;
    f[mid] ^= 0x01;
    assert_eq!(parse_datagram(&f, SRC), Datagram::Invalid);
}

#[test]
fn req_devinfo_igual_a_tinytuya_con_el_mismo_iv() {
    let c = case("req_devinfo_7000");
    let iv: [u8; 12] = hex::decode(udp_fixture()["urandom_used_hex"][0].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let got = tuya_proto::udp::encode_req_devinfo("192.0.2.20", iv);
    assert_eq!(hex::encode(got), c["frame_hex"].as_str().unwrap());
    // Y el que arma este crate (IV al azar) se descifra al mismo JSON.
    let ours = tuya_discovery::parse::req_devinfo("192.0.2.20".parse().unwrap());
    let json = tuya_proto::udp::decode_broadcast(&ours).unwrap();
    assert_eq!(json["from"], "app");
    assert_eq!(json["ip"], "192.0.2.20");
}

fn listen_only(ports: Vec<u16>) -> ScanConfig {
    ScanConfig {
        listen_ip: LOCALHOST,
        ports,
        request_targets: Some(vec![]),
        request_interval: None,
    }
}

async fn send_later(frames: Vec<Vec<u8>>, port: u16) {
    tokio::time::sleep(Duration::from_millis(150)).await;
    let s = tokio::net::UdpSocket::bind((LOCALHOST, 0)).await.unwrap();
    for f in frames {
        s.send_to(&f, (LOCALHOST, port)).await.unwrap();
    }
}

#[tokio::test]
async fn escucha_cada_puerto_por_socket() {
    for name in [
        "broadcast_6666_v31",
        "broadcast_6667_v33",
        "broadcast_7000_v35",
    ] {
        let port = free_port();
        let config = listen_only(vec![port]);
        let sender = tokio::spawn(send_later(vec![frame(name)], port));
        let found = scan(&config, Duration::from_millis(700)).await.unwrap();
        sender.await.unwrap();
        assert_eq!(found.len(), 1, "{name}");
        assert_eq!(found[0].ip, "127.0.0.1");
    }
}

#[tokio::test]
async fn deduplica_por_id_y_gana_el_ultimo() {
    let port = free_port();
    let config = listen_only(vec![port]);
    let frames = vec![
        frame("broadcast_6667_v33"),
        frame("broadcast_6667_v33"),
        b"basura".to_vec(),
        frame("req_devinfo_7000"),
        frame("broadcast_7000_v35"),
    ];
    let sender = tokio::spawn(send_later(frames, port));
    let found = scan(&config, Duration::from_millis(700)).await.unwrap();
    sender.await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].version, ProtocolVersion::V35);
}

#[tokio::test]
async fn manda_req_devinfo_al_destino() {
    let rx = tokio::net::UdpSocket::bind((LOCALHOST, 0)).await.unwrap();
    let target = rx.local_addr().unwrap();
    let config = ScanConfig {
        listen_ip: LOCALHOST,
        ports: vec![free_port()],
        request_targets: Some(vec![(LOCALHOST, target)]),
        request_interval: Some(Duration::from_secs(60)),
    };
    let scanner = tokio::spawn(async move { scan(&config, Duration::from_millis(500)).await });

    let mut buf = [0u8; 512];
    let (n, _) = tokio::time::timeout(Duration::from_secs(2), rx.recv_from(&mut buf))
        .await
        .expect("no llegó el REQ_DEVINFO")
        .unwrap();
    let json = tuya_proto::udp::decode_broadcast(&buf[..n]).unwrap();
    assert_eq!(json["from"], "app");
    assert_eq!(json["ip"], "127.0.0.1");
    assert!(scanner.await.unwrap().unwrap().is_empty());
}

#[tokio::test]
async fn sin_puertos_disponibles_es_error() {
    let config = ScanConfig {
        listen_ip: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 123)), // no es de esta máquina
        ports: vec![free_port()],
        request_targets: Some(vec![]),
        request_interval: None,
    };
    let err = scan(&config, Duration::from_millis(100)).await.unwrap_err();
    assert!(matches!(err, tuya_discovery::DiscoveryError::Bind(_)));
}
