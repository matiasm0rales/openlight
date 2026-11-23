//! Código común a los tres backends de captura: qué monitor seguir, reducción de cuadros y
//! detección de cuadros vacíos. No depende del sistema operativo, así que sus pruebas corren
//! en cualquier CI.
//!
//! Dueño: tarea E1 (ver `docs/AGENTS.md`). E2 y E3 lo usan desde `src/windows` y `src/linux`.

use openlight_types::{DisplayId, DisplayInfo};

/// Número de monitor a seguir (desde 1), o `None` si no está.
///
/// Port fiel de `elegir_monitor` de `ambilight.py`. `builtin` dice, en el orden del sistema,
/// si cada pantalla es la integrada (la de la notebook). Sin `requested`, se sigue el primer
/// monitor externo, y si no hay ninguno, el primero de la lista.
pub fn choose_monitor(requested: Option<usize>, builtin: &[bool]) -> Option<usize> {
    if let Some(requested) = requested {
        // El script aceptaba 0 (en mss es "todos los monitores juntos"); acá la numeración es
        // desde 1 y el 0 no es ningún monitor.
        return (1..=builtin.len())
            .contains(&requested)
            .then_some(requested);
    }
    if builtin.is_empty() {
        return None;
    }
    Some(builtin.iter().position(|&b| !b).map_or(1, |i| i + 1))
}

/// Monitor a seguir entre `displays` (en el orden que da el sistema).
///
/// Con `requested`, devuelve ese monitor si está conectado y `None` si no (hay que esperar a que
/// aparezca, como hace el script). Sin `requested`, aplica [`choose_monitor`]: el primer externo,
/// o el integrado si no hay externos. Llamada cada vez que cambia la lista, también implementa
/// "volver al externo cuando aparece".
pub fn pick_display(requested: Option<&DisplayId>, displays: &[DisplayInfo]) -> Option<DisplayId> {
    if let Some(requested) = requested {
        return displays
            .iter()
            .find(|d| &d.id == requested)
            .map(|d| d.id.clone());
    }
    let builtin: Vec<bool> = displays.iter().map(|d| d.builtin).collect();
    choose_monitor(None, &builtin).map(|n| displays[n - 1].id.clone())
}

/// Copia un búfer BGRA con relleno al final de cada fila (`stride` bytes por fila) a uno
/// compacto de `width * height * 4` bytes. Devuelve `None` si el búfer es más chico de lo que
/// dicen las medidas.
pub fn pack_rows(src: &[u8], width: usize, height: usize, stride: usize) -> Option<Vec<u8>> {
    let row = width.checked_mul(4)?;
    if stride < row {
        return None;
    }
    if height == 0 || width == 0 {
        return Some(Vec::new());
    }
    let needed = stride.checked_mul(height - 1)?.checked_add(row)?;
    if src.len() < needed {
        return None;
    }
    let mut out = Vec::with_capacity(row * height);
    for y in 0..height {
        out.extend_from_slice(&src[y * stride..y * stride + row]);
    }
    Some(out)
}

/// Reduce un búfer BGRA de `src_w`×`src_h` (con `stride` bytes por fila) a `dst_w`×`dst_h`
/// promediando cada bloque de píxeles (filtro de caja). Sirve para los backends que reducen en
/// la CPU. Devuelve `None` si las medidas no cierran o alguna es cero.
///
/// Promediar (y no saltear píxeles, como hacía el script con `img[::paso, ::paso]`) hace que un
/// detalle chico y brillante no haga titilar el color.
pub fn downscale_bgra(
    src: &[u8],
    (src_w, src_h): (usize, usize),
    stride: usize,
    (dst_w, dst_h): (usize, usize),
) -> Option<Vec<u8>> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 || stride < src_w.checked_mul(4)? {
        return None;
    }
    let needed = stride.checked_mul(src_h - 1)?.checked_add(src_w * 4)?;
    if src.len() < needed {
        return None;
    }
    let mut out = Vec::with_capacity(dst_w * dst_h * 4);
    for dy in 0..dst_h {
        let y0 = dy * src_h / dst_h;
        let y1 = ((dy + 1) * src_h / dst_h).max(y0 + 1);
        for dx in 0..dst_w {
            let x0 = dx * src_w / dst_w;
            let x1 = ((dx + 1) * src_w / dst_w).max(x0 + 1);
            let mut sum = [0u64; 4];
            for y in y0..y1 {
                let fila = &src[y * stride + x0 * 4..y * stride + x1 * 4];
                for px in fila.as_chunks::<4>().0 {
                    for (s, &c) in sum.iter_mut().zip(px) {
                        *s += u64::from(c);
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u64;
            // Redondeo al más cercano; el resultado entra en u8 porque es un promedio de u8.
            out.extend(sum.iter().map(|&s| ((s + n / 2) / n) as u8));
        }
    }
    Some(out)
}

/// Si un cuadro BGRA está vacío: sin bytes, o todos en cero (alfa incluido).
///
/// Una captura real siempre tiene alfa 255, aun con la pantalla en negro; un búfer todo en
/// cero sale cuando el sistema entrega un cuadro sin contenido (pantalla que se está apagando,
/// captura cortada). El backend publica `None` en ese caso en lugar de un "negro" falso.
pub fn is_empty_frame(bgra: &[u8]) -> bool {
    bgra.iter().all(|&b| b == 0)
}

/// Color promedio (r, g, b) en `0..1` de un cuadro BGRA. Lo usan los ejemplos y las pruebas
/// manuales; el color de verdad lo calcula `color-engine`.
pub fn average_rgb(bgra: &[u8]) -> Option<(f32, f32, f32)> {
    let n = bgra.len() / 4;
    if n == 0 {
        return None;
    }
    let mut sum = [0u64; 3];
    for px in bgra.as_chunks::<4>().0 {
        sum[0] += u64::from(px[2]);
        sum[1] += u64::from(px[1]);
        sum[2] += u64::from(px[0]);
    }
    let f = |s: u64| s as f32 / (n as f32 * 255.0);
    Some((f(sum[0]), f(sum[1]), f(sum[2])))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pantalla(id: &str, builtin: bool) -> DisplayInfo {
        DisplayInfo {
            id: DisplayId(id.into()),
            name: id.into(),
            width: 1920,
            height: 1080,
            builtin,
            primary: false,
        }
    }

    // port: test_ambilight.py::ElegirMonitor::test_casos
    #[test]
    fn elegir_monitor_casos() {
        type Caso<'a> = (Option<usize>, &'a [bool], Option<usize>, &'a str);
        let casos: &[Caso] = &[
            (None, &[true, false], Some(2), "externo a la derecha"),
            (None, &[false, true], Some(1), "externo como principal"),
            (None, &[true], Some(1), "solo la MacBook"),
            (None, &[], None, "sin pantallas"),
            (Some(1), &[true, false], Some(1), "pedido explícito"),
            (Some(3), &[true, false], None, "el pedido no está"),
        ];
        for &(pedido, integrados, esperado, caso) in casos {
            assert_eq!(choose_monitor(pedido, integrados), esperado, "{caso}");
        }
    }

    #[test]
    fn el_monitor_cero_no_existe() {
        assert_eq!(choose_monitor(Some(0), &[true, false]), None);
        assert_eq!(choose_monitor(Some(0), &[]), None);
    }

    #[test]
    fn pick_display_sigue_al_externo_y_vuelve_cuando_aparece() {
        let solo_mac = [pantalla("1", true)];
        assert_eq!(pick_display(None, &solo_mac), Some(DisplayId("1".into())));
        let con_externo = [pantalla("1", true), pantalla("7", false)];
        assert_eq!(
            pick_display(None, &con_externo),
            Some(DisplayId("7".into()))
        );
        assert_eq!(pick_display(None, &[]), None);
    }

    #[test]
    fn pick_display_con_pedido_espera_si_no_esta() {
        let pantallas = [pantalla("1", true), pantalla("7", false)];
        let pedido = DisplayId("1".into());
        assert_eq!(pick_display(Some(&pedido), &pantallas), Some(pedido));
        assert_eq!(pick_display(Some(&DisplayId("9".into())), &pantallas), None);
    }

    #[test]
    fn pack_rows_saca_el_relleno() {
        // 2×2 con 4 bytes de relleno por fila.
        let src = [
            1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 9, 10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 0,
        ];
        let out = pack_rows(&src, 2, 2, 12).unwrap();
        assert_eq!(out, (1..=16).collect::<Vec<u8>>());
        // La última fila puede venir sin relleno.
        assert!(pack_rows(&src[..20], 2, 2, 12).is_some());
        assert!(pack_rows(&src[..19], 2, 2, 12).is_none());
        assert!(
            pack_rows(&src, 4, 2, 12).is_none(),
            "stride más chico que la fila"
        );
    }

    #[test]
    fn downscale_promedia_cada_bloque() {
        // 4×2 → 2×1: el bloque izquierdo es negro y blanco, el derecho todo rojo.
        let negro = [0, 0, 0, 255];
        let blanco = [255, 255, 255, 255];
        let rojo = [0, 0, 255, 255];
        let mut src = Vec::new();
        for px in [negro, blanco, rojo, rojo, blanco, negro, rojo, rojo] {
            src.extend_from_slice(&px);
        }
        let out = downscale_bgra(&src, (4, 2), 16, (2, 1)).unwrap();
        assert_eq!(out, vec![128, 128, 128, 255, 0, 0, 255, 255]);
    }

    #[test]
    fn downscale_con_destino_mas_grande_repite_pixeles() {
        let src = [10, 20, 30, 255];
        let out = downscale_bgra(&src, (1, 1), 4, (2, 2)).unwrap();
        assert_eq!(out, [10, 20, 30, 255].repeat(4));
    }

    #[test]
    fn downscale_rechaza_medidas_invalidas() {
        assert!(downscale_bgra(&[0; 16], (2, 2), 8, (0, 1)).is_none());
        assert!(downscale_bgra(&[0; 15], (2, 2), 8, (1, 1)).is_none());
        assert!(downscale_bgra(&[0; 16], (2, 2), 4, (1, 1)).is_none());
    }

    #[test]
    fn cuadro_vacio_no_es_pantalla_negra() {
        assert!(is_empty_frame(&[]));
        assert!(is_empty_frame(&[0; 16]));
        assert!(
            !is_empty_frame(&[0, 0, 0, 255].repeat(4)),
            "negro real tiene alfa"
        );
    }

    #[test]
    fn promedio_en_rgb() {
        let (r, g, b) = average_rgb(&[0, 0, 255, 255, 255, 0, 0, 255]).unwrap();
        assert!((r - 0.5).abs() < 1e-6 && g.abs() < 1e-6 && (b - 0.5).abs() < 1e-6);
        assert!(average_rgb(&[]).is_none());
    }
}
