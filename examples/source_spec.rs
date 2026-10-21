//! Harmonic spectrum of the LF flow derivative: `source_spec rd f0`.
use nanami::glottis::*;
fn main() {
    let a: Vec<f64> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let (rd, f0) = (a[0], a[1]);
    let fs = 96000.0;
    let mut g = Glottis::new(fs, 1);
    let p = SourceParams { f0, rd, amplitude: 1.0, jitter: 0.0, shimmer: 0.0, flutter: 0.0, creak: 0.0 };
    let n = 1 << 16;
    let mut re: Vec<f64> = (0..n + 20000).map(|_| g.step(&p).dflow).skip(20000).collect();
    for (i, v) in re.iter_mut().enumerate() { *v *= 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos(); }
    let mut im = vec![0.0; n];
    nanami::dsp::fft(&mut re, &mut im);
    let db = |f: f64| { let k = (f * n as f64 / fs).round() as usize; let m = (k-3..k+4).map(|j| re[j]*re[j]+im[j]*im[j]).fold(0.0, f64::max); 10.0 * m.log10() };
    let h1 = db(f0);
    for h in [1.0, 2.0, 4.0, 8.0] { print!("H{h}:{:.1} ", db(h * f0) - h1); }
    for f in [1000.0, 2000.0, 3000.0, 4000.0, 6000.0] { let h = (f / f0).round(); print!("{f}:{:.1} ", db(h * f0) - h1); }
    println!();
}
