//! Lista los dispositivos de la cuenta de Tuya y el mapeo de DPs de las luces.
//!
//! Lee las credenciales de variables de entorno (nunca de un archivo):
//!
//! ```bash
//! TUYA_ACCESS_ID=... TUYA_ACCESS_SECRET=... TUYA_REGION=us \
//!     cargo run -p tuya-cloud --example listar
//! ```
//!
//! `TUYA_REGION` acepta los códigos de tinytuya (`us`, `us-e`, `eu`, `eu-w`, `in`, `sg`, `cn`) y
//! por defecto es `us`. Con `--mapeo` también descarga el mapeo de DPs de cada luz.
//!
//! **No imprime las claves locales**: solo dice si la nube devolvió una.

use std::process::ExitCode;

use openlight_types::{CloudCredentials, Region, Secret, TuyaCloud};
use tuya_cloud::{TuyaCloudClient, to_user_facing};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let (Some(access_id), Some(access_secret)) = (var("TUYA_ACCESS_ID"), var("TUYA_ACCESS_SECRET"))
    else {
        eprintln!("Faltan TUYA_ACCESS_ID y/o TUYA_ACCESS_SECRET.");
        return ExitCode::from(2);
    };
    let region_code = var("TUYA_REGION").unwrap_or_else(|| "us".into());
    let Some(region) = Region::parse(&region_code) else {
        eprintln!("Región desconocida: {region_code}. Usá us, us-e, eu, eu-w, in, sg o cn.");
        return ExitCode::from(2);
    };
    let with_mapping = std::env::args().any(|a| a == "--mapeo");

    let cloud = match TuyaCloudClient::new(CloudCredentials {
        access_id: access_id.trim().to_owned(),
        access_secret: Secret::new(access_secret.trim().to_owned()),
        region,
    }) {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };

    let account = match cloud.validate().await {
        Ok(a) => a,
        Err(e) => return fail(&e),
    };
    println!(
        "Cuenta OK en {} (uid {})",
        account.region.code(),
        account.uid
    );

    let devices = match cloud.devices().await {
        Ok(d) => d,
        Err(e) => return fail(&e),
    };
    println!("{} dispositivos:", devices.len());
    for d in &devices {
        println!(
            "- {:<28} id={} categoría={} producto={} modelo={} {} {} clave={}",
            d.name,
            d.id.0,
            d.category,
            d.product_id,
            d.model.as_deref().unwrap_or("-"),
            if d.is_light() { "luz" } else { "otro" },
            if d.online {
                "en línea"
            } else {
                "desconectado"
            },
            if d.local_key.expose().is_empty() {
                "no"
            } else {
                "sí"
            },
        );
        if with_mapping && d.is_light() {
            match cloud.mapping(&d.id).await {
                Ok(m) => {
                    for (dp, schema) in &m.0 {
                        println!(
                            "    {dp:>4} {:<18} {:<8} {}",
                            schema.code, schema.kind, schema.values
                        );
                    }
                }
                Err(e) => println!("    (no se pudo leer el mapeo: {e})"),
            }
        }
    }
    ExitCode::SUCCESS
}

fn fail(e: &openlight_types::CloudError) -> ExitCode {
    let ui = to_user_facing(e);
    eprintln!("Error: {e} ({})", ui.code.as_str());
    if !ui.params.is_empty() {
        eprintln!("  {:?}", ui.params);
    }
    ExitCode::FAILURE
}
