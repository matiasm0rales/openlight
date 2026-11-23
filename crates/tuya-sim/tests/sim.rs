//! Pruebas del simulador: protocolo en las cuatro versiones y cada falla inyectable.

mod common;

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use common::{Client, Read, T, dps, start};
use openlight_types::ProtocolVersion::{self, V31, V33, V34, V35};
use serde_json::json;
use tokio::net::UdpSocket;
use tuya_sim::{BroadcastConfig, Faults, Profile, SimConfig, cmd};

fn cfg(v: ProtocolVersion) -> SimConfig {
    SimConfig::new(v)
}

#[tokio::test]
async fn status_control_y_latido_en_las_cuatro_versiones() {
    for v in ProtocolVersion::ALL {
        let sim = start(cfg(v)).await;
        let mut c = Client::connect(&sim).await.unwrap();

        let estado = c.status().await.expect("status");
        assert_eq!(estado, Profile::V2.default_dps(), "{v}");

        let push = c
            .set(json!({"20": false, "21": "white"}))
            .await
            .expect("push");
        assert_eq!(push, dps(json!({"20": false, "21": "white"})), "{v}");
        assert_eq!(sim.dp("20"), Some(json!(false)));
        assert_eq!(sim.writes(), vec![dps(json!({"20": false, "21": "white"}))]);

        match c.heartbeat().await {
            Read::Msg(m) => {
                assert_eq!(m.cmd, cmd::HEART_BEAT);
                assert!(m.payload.is_empty());
            }
            other => panic!("{v}: latido sin respuesta: {other:?}"),
        }
        let s = sim.stats();
        assert_eq!(
            (s.queries(), s.controls(), s.heartbeats()),
            (1, 1, 1),
            "{v}"
        );
        assert_eq!(s.negotiations, u32::from(v.uses_session_key()), "{v}");
        sim.shutdown().await;
    }
}

#[tokio::test]
async fn perfil_v1_con_dps_1_a_5() {
    let sim = start(cfg(V33).profile(Profile::V1)).await;
    let mut c = Client::connect(&sim).await.unwrap();
    assert_eq!(c.status().await.unwrap()["2"], json!("white"));
    let push = c
        .set(json!({"2": "colour", "5": "ff00000000ffff"}))
        .await
        .unwrap();
    assert_eq!(push["2"], json!("colour"));
}

#[tokio::test]
async fn valores_invalidos_se_ignoran_como_en_un_foco_real() {
    let sim = start(cfg(V35)).await;
    let mut c = Client::connect(&sim).await.unwrap();
    c.set_nowait(json!({"22": 0, "99": true, "21": "disco"}))
        .await
        .unwrap();
    assert_eq!(
        c.recv_payload(Duration::from_millis(300)).await,
        Read::Timeout
    );
    assert_eq!(sim.dp("22"), Some(json!(1000)));
    assert!(sim.writes().is_empty());
}

#[tokio::test]
async fn updatedps_devuelve_los_dps_pedidos() {
    let sim = start(cfg(V34)).await;
    let mut c = Client::connect(&sim).await.unwrap();
    c.send(cmd::UPDATEDPS, br#"{"dpId":[20,22,99]}"#.to_vec())
        .await
        .unwrap();
    match c.recv_payload(T).await {
        Read::Msg(m) => {
            assert_eq!(m.cmd, cmd::STATUS);
            assert_eq!(
                common::dps_of(&m.payload).unwrap(),
                dps(json!({"20": true, "22": 1000}))
            );
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn cambios_de_afuera_llegan_como_push_a_todas_las_conexiones() {
    let sim = start(cfg(V33)).await;
    let mut a = Client::connect(&sim).await.unwrap();
    let mut b = Client::connect(&sim).await.unwrap();
    sim.set_dps(dps(json!({"20": false})));
    for c in [&mut a, &mut b] {
        match c.recv_payload(T).await {
            Read::Msg(m) => assert_eq!(common::dps_of(&m.payload).unwrap()["20"], json!(false)),
            other => panic!("{other:?}"),
        }
    }
    // Lo que escribe una conexión también le llega a la otra.
    a.set(json!({"22": 500})).await.unwrap();
    match b.recv_payload(T).await {
        Read::Msg(m) => assert_eq!(common::dps_of(&m.payload).unwrap()["22"], json!(500)),
        other => panic!("{other:?}"),
    }
    assert!(
        sim.writes().len() == 1,
        "set_dps no cuenta como escritura por la red"
    );
}

#[tokio::test]
async fn falla_cerrar_cada_n_segundos_y_reconectar() {
    let sim = start(cfg(V35).faults(Faults {
        drop_every: Some(Duration::from_millis(200)),
        ..Faults::default()
    }))
    .await;
    let mut c = Client::connect(&sim).await.unwrap();
    let t0 = Instant::now();
    assert_eq!(c.recv(T).await, Read::Closed);
    assert!(t0.elapsed() >= Duration::from_millis(150));
    assert!(sim.wait_until(T, |s| s.drops == 1).await);

    let mut c = Client::connect(&sim).await.unwrap();
    assert!(c.status().await.is_some(), "reconecta y anda");
    assert_eq!(sim.stats().connections, 2);
}

#[tokio::test]
async fn falla_no_responder_y_recuperarse_en_caliente() {
    let sim = start(cfg(V33)).await;
    let mut c = Client::connect(&sim).await.unwrap();
    sim.update_faults(|f| f.mute = true);
    assert_eq!(c.heartbeat().await, Read::Timeout);
    assert!(sim.stats().muted >= 1);
    sim.set_dps(dps(json!({"20": false})));
    assert_eq!(
        c.recv(Duration::from_millis(200)).await,
        Read::Timeout,
        "mudo: tampoco pushes"
    );

    sim.update_faults(|f| f.mute = false);
    assert!(
        c.status().await.is_some(),
        "la misma conexión vuelve a andar"
    );
}

#[tokio::test]
async fn falla_rechazar_la_clave_en_34_y_35_rompe_la_negociacion() {
    for v in [V34, V35] {
        let sim = start(cfg(v).faults(Faults {
            reject_key: true,
            ..Faults::default()
        }))
        .await;
        assert!(
            Client::connect(&sim).await.is_err(),
            "{v}: la negociación tiene que fallar"
        );
        assert!(sim.wait_until(T, |s| s.key_rejections == 1).await, "{v}");
        assert_eq!(sim.stats().negotiations, 0);

        sim.set_faults(Faults::default());
        assert!(
            Client::connect(&sim).await.is_ok(),
            "{v}: con la clave buena vuelve"
        );
    }
}

#[tokio::test]
async fn falla_rechazar_la_clave_en_31_y_33_corta_la_conexion() {
    for v in [V31, V33] {
        let sim = start(cfg(v)).await;
        let mut c = Client::connect(&sim).await.unwrap();
        assert!(c.status().await.is_some());
        // Activarla en caliente corta lo abierto, como un foco que se re-vinculó.
        sim.update_faults(|f| f.reject_key = true);
        assert_eq!(c.recv(T).await, Read::Closed, "{v}");

        // En 3.1 la consulta va en claro: el corte llega con el primer comando cifrado.
        let mut c = Client::connect(&sim).await.unwrap();
        let respuesta = if v == V31 {
            c.set(json!({"20": false})).await
        } else {
            c.status().await
        };
        assert_eq!(respuesta, None, "{v}");
        assert!(sim.wait_until(T, |s| s.key_rejections >= 1).await, "{v}");
    }
}

#[tokio::test]
async fn cliente_con_otra_clave_no_negocia() {
    let sim = start(cfg(V35)).await;
    let r = Client::connect_with_key(sim.addr(), V35, *b"OtraClaveCualq16").await;
    assert!(r.is_err());
    assert!(sim.wait_until(T, |s| s.key_rejections == 1).await);
}

#[tokio::test]
async fn falla_latencia() {
    let sim = start(cfg(V34).faults(Faults {
        latency: Duration::from_millis(250),
        ..Faults::default()
    }))
    .await;
    let mut c = Client::connect(&sim).await.unwrap();
    let t0 = Instant::now();
    assert!(c.status().await.is_some());
    assert!(t0.elapsed() >= Duration::from_millis(250));
}

#[tokio::test]
async fn falla_limite_de_comandos_por_segundo() {
    let sim = start(cfg(V33).faults(Faults {
        max_commands_per_s: Some(2),
        ..Faults::default()
    }))
    .await;
    let mut c = Client::connect(&sim).await.unwrap();
    for i in 0..5 {
        c.set_nowait(json!({"22": 100 + i})).await.unwrap();
    }
    assert!(sim.wait_until(T, |s| s.controls() == 5).await);
    assert_eq!(sim.writes().len(), 2);
    assert_eq!(sim.stats().rate_limited, 3);
    assert_eq!(sim.dp("22"), Some(json!(101)));
}

#[tokio::test]
async fn falla_otra_app_ocupa_la_conexion() {
    // exclusive: la segunda conexión se rechaza mientras la primera siga abierta.
    let sim = start(cfg(V33).faults(Faults {
        exclusive: true,
        ..Faults::default()
    }))
    .await;
    let mut primera = Client::connect(&sim).await.unwrap();
    let mut segunda = Client::connect(&sim).await.unwrap();
    assert_eq!(segunda.recv(T).await, Read::Closed);
    assert_eq!(sim.stats().rejected, 1);
    assert!(primera.status().await.is_some(), "la primera sigue andando");
    drop(primera);
    assert!(sim.wait_until(T, |_| sim.active_connections() == 0).await);
    let mut tercera = Client::connect(&sim).await.unwrap();
    assert!(tercera.status().await.is_some());

    // occupied: otra app (Smart Life) ya la tiene; se rechaza todo.
    sim.update_faults(|f| f.occupied = true);
    let mut otra = Client::connect(&sim).await.unwrap();
    assert_eq!(otra.recv(T).await, Read::Closed);
    assert_eq!(sim.stats().rejected, 2);
}

#[tokio::test]
async fn desconectar_todo() {
    let sim = start(cfg(V35)).await;
    let mut a = Client::connect(&sim).await.unwrap();
    let mut b = Client::connect(&sim).await.unwrap();
    sim.disconnect_all();
    assert_eq!(a.recv(T).await, Read::Closed);
    assert_eq!(b.recv(T).await, Read::Closed);
    assert!(sim.wait_until(T, |_| sim.active_connections() == 0).await);
}

async fn free_udp_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test]
async fn broadcasts_udp_y_req_devinfo() {
    let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let req_port = free_udp_port().await;
    let sim = start(cfg(V35).broadcast(BroadcastConfig {
        target: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: Some(port),
        interval: Duration::from_millis(50),
        advertised_ip: Some(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10))),
        listen_port: Some(req_port),
        reply_port: port,
    }))
    .await;
    let suite = common::suite();
    let mut buf = [0u8; 2048];
    let (n, _) = tokio::time::timeout(T, listener.recv_from(&mut buf))
        .await
        .unwrap()
        .unwrap();
    let (_, body) = suite.udp_decode(&buf[..n]).unwrap();
    let anuncio: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(anuncio["gwId"], json!(sim.dev_id().0));
    assert_eq!(anuncio["version"], json!("3.5"));
    assert_eq!(anuncio["ip"], json!("192.0.2.10"));

    // REQ_DEVINFO al "7000": contesta a la IP del pedido, en el puerto de respuesta.
    let app = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pedido = tuya_proto::udp::encode_req_devinfo("127.0.0.1", [7; 12]);
    app.send_to(&pedido, ("127.0.0.1", req_port)).await.unwrap();
    // Llegan anuncios periódicos y la respuesta; alcanza con que llegue alguno más.
    let (n, _) = tokio::time::timeout(T, listener.recv_from(&mut buf))
        .await
        .unwrap()
        .unwrap();
    assert!(suite.udp_decode(&buf[..n]).is_some());
}
