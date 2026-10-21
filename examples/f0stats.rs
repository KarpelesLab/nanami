//! Mean F0 / duration / level per emotion for one sentence (autocorrelation pitch).
fn main() {
    let v = nanami::voice::Voice::nanami();
    let text = "I can't believe you did that for me.";
    for emo in ["neutral", "happy", "sad", "angry", "fear", "calm", "bored", "excited"] {
        let r = nanami::render_markup(&format!("<emotion {emo}>{text}</emotion>"), &v, nanami::ipa::Lang::English);
        let x: Vec<f64> = r.audio.iter().map(|&s| s as f64).collect();
        let (w, hop) = (1200usize, 480usize);
        let mut f0s = Vec::new();
        let mut i = 0;
        while i + w < x.len() {
            let fr = &x[i..i + w];
            let e: f64 = fr.iter().map(|a| a * a).sum();
            if e / w as f64 > 1e-3 {
                let (mut best, mut bl) = (0.0, 0);
                for lag in 60..480 { let c: f64 = (0..w - lag).map(|k| fr[k] * fr[k + lag]).sum::<f64>() / e; if c > best { best = c; bl = lag; } }
                if best > 0.5 { f0s.push(48000.0 / bl as f64); }
            }
            i += hop;
        }
        f0s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = f0s[f0s.len() / 2];
        let (p10, p90) = (f0s[f0s.len() / 10], f0s[f0s.len() * 9 / 10]);
        println!("{emo:8} dur {:.2}s  F0 median {med:5.0} Hz  range p10-p90 {:4.1} st", x.len() as f64 / 48000.0, 12.0 * (p90 / p10).log2());
    }
}
