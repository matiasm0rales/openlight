//! Réplicas exactas de las operaciones de Python y numpy que usa `ambilight.py`.
//!
//! El script calcula las imágenes en `float32` (numpy) y el resto en `float64` (Python y
//! `colorsys`). Para quedar dentro de la tolerancia de las fixtures golden no alcanza con
//! "la misma fórmula": hay que repetir el tipo, el orden de las sumas y el redondeo de cada
//! paso. Cada función dice a qué código de numpy o de CPython corresponde.

/// `a % b` de Python (y `np.remainder`) para `float64`: el resultado tiene el signo de `b`.
///
/// CPython (`float_rem`) y numpy (`npy_divmod`) hacen `fmod` y, si el signo no coincide con el
/// del divisor, le suman el divisor. Un cero queda con el signo del divisor.
///
/// Con `b > 0` y `a` en `(-b, 2b)` evita el `fmod` (caro, y se llama por píxel): ahí `fmod` es
/// exacto y da `a` o `a - b` (Sterbenz), así que el resultado es el mismo bit a bit.
pub(crate) fn py_mod(a: f64, b: f64) -> f64 {
    if b > 0.0 {
        if a > 0.0 && a < b {
            return a;
        }
        if a < 0.0 && a > -b {
            return a + b;
        }
        if a >= b && a < 2.0 * b {
            return a - b;
        }
    }
    let m = a % b; // `%` de Rust en flotantes es `fmod`
    if m != 0.0 {
        if (b < 0.0) != (m < 0.0) { m + b } else { m }
    } else {
        0.0_f64.copysign(b)
    }
}

/// `np.remainder` para `float32` (`npy_divmodf`): igual que [`py_mod`], en `f32`.
/// Con el mismo atajo que [`py_mod`].
pub(crate) fn py_mod_f32(a: f32, b: f32) -> f32 {
    if b > 0.0 {
        if a > 0.0 && a < b {
            return a;
        }
        if a < 0.0 && a > -b {
            return a + b;
        }
        if a >= b && a < 2.0 * b {
            return a - b;
        }
    }
    let m = a % b;
    if m != 0.0 {
        if (b < 0.0) != (m < 0.0) { m + b } else { m }
    } else {
        0.0_f32.copysign(b)
    }
}

/// Bloque de la suma por pares de numpy (`PW_BLOCKSIZE` en `loops_utils.h.src`).
const PW_BLOCKSIZE: usize = 128;

/// `np.sum` de un arreglo `float32` contiguo: la suma por pares de numpy
/// (`pairwise_sum_FLOAT`), con ocho acumuladores por bloque de hasta 128 elementos.
///
/// Verificado contra numpy 2.5: da exactamente el mismo `float32` para 7 a 5184 elementos.
pub(crate) fn pairwise_sum_f32(a: &[f32]) -> f32 {
    let n = a.len();
    if n < 8 {
        let mut res = 0.0_f32;
        for &x in a {
            res += x;
        }
        res
    } else if n <= PW_BLOCKSIZE {
        let mut r = [0.0_f32; 8];
        r.copy_from_slice(&a[..8]);
        let mut i = 8;
        while i < n - (n % 8) {
            for (j, acc) in r.iter_mut().enumerate() {
                *acc += a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        for &x in &a[i..] {
            res += x;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise_sum_f32(&a[..n2]) + pairwise_sum_f32(&a[n2..])
    }
}

/// `colorsys.rgb_to_hsv` de CPython 3.14, en `float64`. Devuelve `(h, s, v)` en `0..1`.
pub(crate) fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let rangec = maxc - minc;
    let v = maxc;
    if minc == maxc {
        return (0.0, 0.0, v);
    }
    let s = rangec / maxc;
    let rc = (maxc - r) / rangec;
    let gc = (maxc - g) / rangec;
    let bc = (maxc - b) / rangec;
    let h = if r == maxc {
        bc - gc
    } else if g == maxc {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    (py_mod(h / 6.0, 1.0), s, v)
}

/// `np.interp(x, xp, fp)` para un `x` escalar (`arr_interp` en `compiled_base.c`).
///
/// `xp` tiene que estar ordenado de menor a mayor y tener el mismo largo que `fp` (al menos
/// un punto). Fuera del rango devuelve el primer o el último valor, sin extrapolar.
pub(crate) fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    debug_assert_eq!(xp.len(), fp.len());
    let len = xp.len();
    if len == 1 {
        // numpy: con un solo punto, a los dos lados vale fp[0]
        return fp[0];
    }
    let last = len - 1;
    if x.is_nan() {
        return x;
    }
    if x > xp[last] {
        return fp[last];
    }
    if x == xp[last] {
        return fp[last];
    }
    if x < xp[0] {
        return fp[0];
    }
    // Índice j con xp[j] <= x < xp[j + 1]
    let j = xp.partition_point(|&p| p <= x) - 1;
    if j >= last {
        return fp[last];
    }
    if xp[j] == x {
        return fp[j];
    }
    let slope = (fp[j + 1] - fp[j]) / (xp[j + 1] - xp[j]);
    let mut res = slope * (x - xp[j]) + fp[j];
    if res.is_nan() {
        res = slope * (x - xp[j + 1]) + fp[j + 1];
        if res.is_nan() && fp[j] == fp[j + 1] {
            res = fp[j];
        }
    }
    res
}

/// Pasa un ajuste `f32` del catálogo a `f64` con el valor decimal que se escribió.
///
/// `0.06_f32 as f64` da `0.0599999986588955`; el script usa el `float64` `0.06`. El `Display`
/// de `f32` imprime el decimal más corto que vuelve al mismo `f32`, así que releerlo como `f64`
/// recupera la constante de Python.
pub(crate) fn decimal_f64(x: f32) -> f64 {
    x.to_string().parse().unwrap_or(f64::from(x))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modulo_como_python() {
        assert_eq!(py_mod(-1.0, 360.0), 359.0);
        assert_eq!(py_mod(370.0, 360.0), 10.0);
        assert_eq!(py_mod(-0.25, 1.0), 0.75);
        assert!(py_mod(-360.0, 360.0).is_sign_positive());
        assert_eq!(py_mod_f32(-0.5, 6.0), 5.5);
        // Un negativo diminuto da 6 al redondear, como en numpy
        assert_eq!(py_mod_f32(-1e-9, 6.0), 6.0);
    }

    #[test]
    fn el_atajo_da_lo_mismo_que_fmod() {
        fn lento(a: f64, b: f64) -> f64 {
            let m = a % b;
            if m != 0.0 {
                if (b < 0.0) != (m < 0.0) { m + b } else { m }
            } else {
                0.0_f64.copysign(b)
            }
        }
        let mut x = -1000.0_f64;
        while x < 1500.0 {
            for a in [x, x + 1e-13, -1e-300, 1e-300, 359.999_999_999_999_94] {
                assert_eq!(py_mod(a, 360.0).to_bits(), lento(a, 360.0).to_bits(), "{a}");
                assert_eq!(py_mod(a, 1.0).to_bits(), lento(a, 1.0).to_bits(), "{a}");
            }
            x += 0.37;
        }
        for a in [-0.0_f32, 0.0, -1.0, -1e-30, 1.0, 5.999_999, 6.0, 7.5, 13.0] {
            let m = a % 6.0;
            let fmod = if m != 0.0 {
                if m < 0.0 { m + 6.0 } else { m }
            } else {
                0.0
            };
            assert_eq!(py_mod_f32(a, 6.0).to_bits(), fmod.to_bits(), "{a}");
        }
    }

    #[test]
    fn interp_como_numpy() {
        let xp = [0.0, 120.0, 240.0, 360.0];
        let fp = [-10.0, 100.0, 240.0, 350.0];
        assert_eq!(interp(-5.0, &xp, &fp), -10.0);
        assert_eq!(interp(400.0, &xp, &fp), 350.0);
        assert_eq!(interp(120.0, &xp, &fp), 100.0);
        assert_eq!(interp(360.0, &xp, &fp), 350.0);
        assert!((interp(60.0, &xp, &fp) - 45.0).abs() < 1e-12);
        assert_eq!(interp(10.0, &[0.0], &[3.0]), 3.0);
    }

    #[test]
    fn hsv_como_colorsys() {
        assert_eq!(rgb_to_hsv(0.5, 0.5, 0.5), (0.0, 0.0, 0.5));
        let (h, s, v) = rgb_to_hsv(1.0, 0.0, 0.0);
        assert_eq!((h, s, v), (0.0, 1.0, 1.0));
        let (h, _, _) = rgb_to_hsv(1.0, 0.0, 0.1);
        assert!(h > 0.9 && h < 1.0);
    }

    #[test]
    fn suma_por_pares_chica_y_grande() {
        let v: Vec<f32> = (0..1000).map(|i| (i as f32) * 0.001).collect();
        let exacta: f64 = v.iter().map(|&x| f64::from(x)).sum();
        assert!((f64::from(pairwise_sum_f32(&v)) - exacta).abs() < 1e-3);
        assert_eq!(pairwise_sum_f32(&[]), 0.0);
        assert_eq!(pairwise_sum_f32(&[1.0, 2.0, 3.0]), 6.0);
    }

    #[test]
    fn decimal_recupera_la_constante() {
        assert_eq!(decimal_f64(0.06), 0.06);
        assert_eq!(decimal_f64(1.4), 1.4);
        assert_eq!(decimal_f64(300.0), 300.0);
    }
}
