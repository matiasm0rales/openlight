//! Un foco Tuya falso en la red local, para desarrollar la UI sin hardware.
//!
//! ```text
//! cargo run -p tuya-sim --example foco -- [opciones]
//!
//!   --version 3.1|3.3|3.4|3.5   protocolo (3.5)
//!   --bind IP:PUERTO            dónde escucha (0.0.0.0:6668; puerto 0 = efímero)
//!   --profile v1|v2             DPs 1–5 o 20–34 (v2)
//!   --id ID --key CLAVE         id (22) y clave local (16) falsos
//!   --no-broadcast              no anuncia por UDP
//!   --drop-every S  --latency-ms MS  --rate N  --mute  --reject-key  --exclusive  --occupied
//!   --stdin                     modo máquina (lo usa tools/golden/contra_sim.py)
//! ```
//!
//! Con `--stdin` imprime una línea JSON con `port`, `id`, `key` y `version`, y después acepta
//! comandos, uno por línea, contestando siempre una línea JSON:
//! `dps`, `stats`, `set {json}`, `kick`, `mute on|off`, `reject on|off`, `exclusive on|off`,
//! `occupied on|off`, `drop S|off`, `latency MS`, `rate N|off`, `quit`.

use std::net::SocketAddr;
use std::time::Duration;

use openlight_types::ProtocolVersion;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tuya_sim::{BroadcastConfig, Faults, Profile, SimConfig, SimHandle, SimStats};

fn usage(msg: &str) -> ! {
    eprintln!("foco: {msg} (ver la ayuda al principio de examples/foco.rs)");
    std::process::exit(2)
}

fn parse_args() -> (SimConfig, bool) {
    let mut cfg = SimConfig::default().broadcast(BroadcastConfig::default());
    let mut faults = Faults::default();
    let mut machine = false;
    let mut args = std::env::args().skip(1);
    let next = |args: &mut dyn Iterator<Item = String>, name: &str| {
        args.next()
            .unwrap_or_else(|| usage(&format!("falta el valor de {name}")))
    };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--version" => {
                let v = next(&mut args, &a);
                cfg.version =
                    ProtocolVersion::parse(&v).unwrap_or_else(|| usage("versión inválida"));
            }
            "--bind" => {
                cfg.bind = next(&mut args, &a)
                    .parse::<SocketAddr>()
                    .unwrap_or_else(|_| usage("dirección inválida"));
            }
            "--profile" => {
                cfg.profile = match next(&mut args, &a).as_str() {
                    "v1" => Profile::V1,
                    "v2" => Profile::V2,
                    _ => usage("perfil inválido"),
                };
            }
            "--id" => cfg.dev_id = openlight_types::DeviceId(next(&mut args, &a)),
            "--key" => {
                let k = next(&mut args, &a);
                cfg.local_key = k
                    .as_bytes()
                    .try_into()
                    .unwrap_or_else(|_| usage("la clave tiene que tener 16 bytes"));
            }
            "--no-broadcast" => cfg.broadcast = None,
            "--drop-every" => {
                let s: f64 = next(&mut args, &a)
                    .parse()
                    .unwrap_or_else(|_| usage("segundos"));
                faults.drop_every = Some(Duration::from_secs_f64(s));
            }
            "--latency-ms" => {
                let ms: u64 = next(&mut args, &a).parse().unwrap_or_else(|_| usage("ms"));
                faults.latency = Duration::from_millis(ms);
            }
            "--rate" => {
                faults.max_commands_per_s = Some(
                    next(&mut args, &a)
                        .parse()
                        .unwrap_or_else(|_| usage("número")),
                );
            }
            "--mute" => faults.mute = true,
            "--reject-key" => faults.reject_key = true,
            "--exclusive" => faults.exclusive = true,
            "--occupied" => faults.occupied = true,
            "--stdin" => machine = true,
            other => usage(&format!("opción desconocida: {other}")),
        }
    }
    (cfg.faults(faults), machine)
}

fn stats_json(s: &SimStats) -> Value {
    json!({
        "connections": s.connections, "rejected": s.rejected, "drops": s.drops,
        "key_rejections": s.key_rejections, "negotiations": s.negotiations,
        "commands": s.commands(), "controls": s.controls(), "queries": s.queries(),
        "heartbeats": s.heartbeats(), "rate_limited": s.rate_limited, "muted": s.muted,
        "pushes": s.pushes,
    })
}

fn on(arg: &str) -> Option<bool> {
    match arg {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// Ejecuta un comando de stdin. `None` = salir.
fn command(sim: &SimHandle, line: &str) -> Option<Value> {
    let (name, arg) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
    let arg = arg.trim();
    let ok = json!({"ok": true});
    let bad = json!({"ok": false, "error": format!("comando inválido: {line}")});
    Some(match name {
        "quit" => return None,
        "dps" => json!(sim.dps()),
        "stats" => stats_json(&sim.stats()),
        "faults" => json!(format!("{:?}", sim.faults())),
        "kick" => {
            sim.disconnect_all();
            ok
        }
        "set" => match serde_json::from_str(arg) {
            Ok(dps) => json!(sim.set_dps(dps)),
            Err(_) => bad,
        },
        "mute" | "reject" | "exclusive" | "occupied" => match on(arg) {
            Some(v) => {
                sim.update_faults(|f| match name {
                    "mute" => f.mute = v,
                    "reject" => f.reject_key = v,
                    "exclusive" => f.exclusive = v,
                    _ => f.occupied = v,
                });
                ok
            }
            None => bad,
        },
        "drop" => match arg {
            "off" => {
                sim.update_faults(|f| f.drop_every = None);
                ok
            }
            s => match s.parse::<f64>() {
                Ok(s) => {
                    sim.update_faults(|f| f.drop_every = Some(Duration::from_secs_f64(s)));
                    ok
                }
                Err(_) => bad,
            },
        },
        "latency" => match arg.parse::<u64>() {
            Ok(ms) => {
                sim.update_faults(|f| f.latency = Duration::from_millis(ms));
                ok
            }
            Err(_) => bad,
        },
        "rate" => match arg {
            "off" => {
                sim.update_faults(|f| f.max_commands_per_s = None);
                ok
            }
            n => match n.parse::<u32>() {
                Ok(n) => {
                    sim.update_faults(|f| f.max_commands_per_s = Some(n));
                    ok
                }
                Err(_) => bad,
            },
        },
        _ => bad,
    })
}

#[tokio::main]
async fn main() {
    let (cfg, machine) = parse_args();
    let key = String::from_utf8_lossy(&cfg.local_key).into_owned();
    let sim = tuya_sim::start(cfg)
        .await
        .unwrap_or_else(|e| usage(&format!("no pude escuchar: {e}")));

    if machine {
        println!(
            "{}",
            json!({"port": sim.port(), "id": sim.dev_id().0, "key": key,
                   "version": sim.version().as_str()})
        );
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            match command(&sim, &line) {
                Some(out) => println!("{out}"),
                None => break,
            }
        }
        println!("{}", json!({"ok": true}));
        sim.shutdown().await;
        return;
    }

    println!("Foco simulado {} en {}", sim.version(), sim.addr());
    println!("  id:    {}", sim.dev_id());
    println!("  clave: {key}  (falsa, solo para desarrollo)");
    println!("  DPs:   {}", json!(sim.dps()));
    println!("Ctrl+C para salir.");
    let mut changes = sim.subscribe();
    loop {
        tokio::select! {
            change = changes.recv() => {
                if let Ok(dps) = change {
                    println!("cambio: {}", json!(dps));
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    sim.shutdown().await;
}
