//! Safe, allocation-free vector DSP operations.
//!
//! macOS uses Accelerate/vDSP for normal audio blocks; other platforms and short slices use the
//! same scalar operations. The public API is safe and supports in-place realtime processing.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod ffi;

#[cfg(target_os = "macos")]
use std::{cell::RefCell, collections::HashMap};

/// Below this length, the function-call overhead is usually greater than vectorization's gain.
const MIN_VECTOR_LEN: usize = 64;
const FILTER_CHUNK: usize = 256;

#[cfg(target_os = "macos")]
thread_local! {
    // Apple states that a DFT setup must not execute concurrently. A cache per worker thread
    // both reuses the expensive setup and makes that constraint structural, without a mutex.
    static FFT_PLANS: RefCell<HashMap<usize, ffi::DftPlan>> = RefCell::new(HashMap::new());
}

/// Clears every sample.
#[inline]
pub fn clear(values: &mut [f32]) {
    #[cfg(target_os = "macos")]
    if values.len() >= MIN_VECTOR_LEN {
        ffi::clear(values);
        return;
    }
    values.fill(0.0);
}

/// Multiplies every sample by one scalar, in place.
#[inline]
pub fn scale(values: &mut [f32], gain: f32) {
    #[cfg(target_os = "macos")]
    if values.len() >= MIN_VECTOR_LEN {
        ffi::scale(values, gain);
        return;
    }
    values.iter_mut().for_each(|x| *x *= gain);
}

/// Adds `source * gain` to `destination`. Only the shared prefix is touched.
#[inline]
pub fn add_scaled(destination: &mut [f32], source: &[f32], gain: f32) {
    let n = destination.len().min(source.len());
    let Some(dst) = destination.get_mut(..n) else { return };
    let Some(src) = source.get(..n) else { return };
    #[cfg(target_os = "macos")]
    if n >= MIN_VECTOR_LEN {
        ffi::add_scaled(dst, src, gain);
        return;
    }
    for (d, s) in dst.iter_mut().zip(src) {
        *d += *s * gain;
    }
}

/// Multiplies `destination` by `source` in place. Only the shared prefix is touched.
#[inline]
pub fn multiply(destination: &mut [f32], source: &[f32]) {
    let n = destination.len().min(source.len());
    let Some(dst) = destination.get_mut(..n) else { return };
    let Some(src) = source.get(..n) else { return };
    #[cfg(target_os = "macos")]
    if n >= MIN_VECTOR_LEN {
        ffi::multiply(dst, src);
        return;
    }
    for (d, s) in dst.iter_mut().zip(src) {
        *d *= *s;
    }
}

/// Largest absolute finite sample, or zero for an empty/non-finite slice.
#[inline]
pub fn peak(values: &[f32]) -> f32 {
    #[cfg(target_os = "macos")]
    if values.len() >= MIN_VECTOR_LEN {
        let peak = ffi::peak(values);
        if peak.is_finite() {
            return peak;
        }
    }
    values.iter().fold(0.0, |m, v| if v.is_finite() { m.max(v.abs()) } else { m })
}

/// Applies one two-pole/two-zero IIR section in place and preserves its input/output history.
///
/// Coefficients are `[b0, b1, b2, a1, a2]` for
/// `y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]`.
/// History arrays are ordered `[older, newest]`. This function performs no heap allocation.
pub fn biquad(values: &mut [f32], coefficients: [f32; 5], input_history: &mut [f32; 2], output_history: &mut [f32; 2]) {
    #[cfg(target_os = "macos")]
    if values.len() >= 16 {
        for chunk in values.chunks_mut(FILTER_CHUNK) {
            let n = chunk.len();
            let mut input = [0.0; FILTER_CHUNK + 2];
            let mut output = [0.0; FILTER_CHUNK + 2];
            input[0..2].copy_from_slice(input_history);
            output[0..2].copy_from_slice(output_history);
            if let Some(dst) = input.get_mut(2..n + 2) {
                dst.copy_from_slice(chunk);
            }
            ffi::biquad(&input[..n + 2], &coefficients, &mut output[..n + 2], n);
            if let Some(src) = output.get(2..n + 2) {
                chunk.copy_from_slice(src);
            }
            *input_history = [input[n], input[n + 1]];
            *output_history = [output[n], output[n + 1]];
        }
        if values.iter().all(|x| x.is_finite()) {
            for state in input_history.iter_mut().chain(output_history.iter_mut()) {
                if state.abs() < 1.0e-30 {
                    *state = 0.0;
                }
            }
            return;
        }
        values.iter_mut().filter(|x| !x.is_finite()).for_each(|x| *x = 0.0);
        *input_history = [0.0; 2];
        *output_history = [0.0; 2];
        return;
    }
    biquad_scalar(values, coefficients, input_history, output_history);
}

fn biquad_scalar(values: &mut [f32], c: [f32; 5], input_history: &mut [f32; 2], output_history: &mut [f32; 2]) {
    for value in values {
        let x = *value;
        let y = c[0] * x + c[1] * input_history[1] + c[2] * input_history[0] - c[3] * output_history[1] - c[4] * output_history[0];
        input_history[0] = input_history[1];
        input_history[1] = x;
        output_history[0] = output_history[1];
        output_history[1] = if y.abs() < 1.0e-30 { 0.0 } else { y };
        if y.is_finite() {
            *value = y;
        } else {
            *value = 0.0;
            *input_history = [0.0; 2];
            *output_history = [0.0; 2];
        }
    }
}

/// Performs a forward complex FFT with a cached vDSP plan on macOS.
///
/// The four slices must have the same power-of-two length. Plan creation can allocate on the
/// first call for a size, so this operation is intended for analyzers and offline work rather
/// than a realtime audio callback. Returns `false` when Accelerate is unavailable or rejects the
/// transform size.
pub fn fft_forward(real: &[f32], imag: &[f32], output_real: &mut [f32], output_imag: &mut [f32]) -> bool {
    let n = real.len();
    if n < 2 || !n.is_power_of_two() || imag.len() != n || output_real.len() != n || output_imag.len() != n {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        FFT_PLANS.with(|plans| {
            let mut plans = plans.borrow_mut();
            if let std::collections::hash_map::Entry::Vacant(entry) = plans.entry(n) {
                let Some(plan) = ffi::DftPlan::new(n) else { return false };
                entry.insert(plan);
            }
            let Some(plan) = plans.get(&n) else { return false };
            plan.forward(real, imag, output_real, output_imag);
            true
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (real, imag, output_real, output_imag);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_ops_match_scalar_results() {
        let source: Vec<f32> = (0..257).map(|i| i as f32 * 0.01 - 1.0).collect();
        let mut scaled = source.clone();
        scale(&mut scaled, -0.25);
        assert!(scaled.iter().zip(&source).all(|(a, b)| (*a - *b * -0.25).abs() < 1e-6));
        let mut sum = vec![0.5; source.len()];
        add_scaled(&mut sum, &source, 0.75);
        assert!(sum.iter().zip(&source).all(|(a, b)| (*a - (0.5 + *b * 0.75)).abs() < 1e-6));
        let mut product = vec![2.0; source.len()];
        multiply(&mut product, &source);
        assert!(product.iter().zip(&source).all(|(a, b)| (*a - *b * 2.0).abs() < 1e-6));
        assert!((peak(&source) - 1.56).abs() < 1e-6);
        clear(&mut sum);
        assert!(sum.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn peak_ignores_non_finite_values() {
        assert_eq!(peak(&[f32::NAN, -2.0, f32::INFINITY]), 2.0);
    }

    #[test]
    fn biquad_matches_the_difference_equation_across_chunks() {
        let c = [0.2, 0.1, 0.05, -0.3, 0.12];
        let mut actual: Vec<f32> = (0..777).map(|i| (i as f32 * 0.031).sin()).collect();
        let mut expected = actual.clone();
        let (mut ax, mut ay, mut ex, mut ey) = ([0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]);
        biquad(&mut actual, c, &mut ax, &mut ay);
        biquad_scalar(&mut expected, c, &mut ex, &mut ey);
        assert!(actual.iter().zip(&expected).all(|(a, b)| (*a - *b).abs() < 2.0e-5));
        assert!(ax.iter().zip(ex).all(|(a, b)| (*a - b).abs() < 1.0e-6));
        assert!(ay.iter().zip(ey).all(|(a, b)| (*a - b).abs() < 2.0e-5));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn vdsp_fft_finds_a_bin_and_reuses_its_plan() {
        let n = 256;
        let real: Vec<f32> = (0..n).map(|i| (std::f32::consts::TAU * 7.0 * i as f32 / n as f32).sin()).collect();
        let imag = vec![0.0; n];
        let mut out_r = vec![0.0; n];
        let mut out_i = vec![0.0; n];
        assert!(fft_forward(&real, &imag, &mut out_r, &mut out_i));
        assert!(out_r[7].hypot(out_i[7]) > 127.0);
        assert!(fft_forward(&real, &imag, &mut out_r, &mut out_i));
        FFT_PLANS.with(|plans| assert_eq!(plans.borrow().len(), 1));
    }
}
