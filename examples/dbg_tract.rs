use nanami::tract::{Tract, piecewise};
fn resp(pts: &[(f64,f64)], len: f64) -> Vec<f64> {
    let mut t = Tract::new(96000.0, len, 11.5);
    let n = t.n;
    let a: Vec<f64> = (0..n).map(|i| piecewise(pts, (i as f64 + 0.5) / n as f64 * len)).collect();
    t.snap(&a, 0.0);
    let mut re: Vec<f64> = (0..16384).map(|i| t.step(if i==0 {1.0} else {0.0})).collect();
    let mut im = vec![0.0;16384]; nanami::dsp::fft(&mut re,&mut im);
    let s: Vec<f64> = (0..8192).map(|k| 10.0*(re[k]*re[k]+im[k]*im[k]+1e-30).log10()).collect();
    nanami::analysis::pick_peaks(&s, 96000.0, 16384, 100.0, 5000.0, 4).iter().map(|x| x.round()).collect()
}
fn main() {
    // Story et al. 1996-like /i/ (cm from glottis, cm²)
    let i = [(0.0,0.6),(1.5,1.2),(2.5,3.0),(4.0,7.0),(7.0,9.0),(9.0,7.0),(10.5,3.0),(11.5,0.8),(12.5,0.5),(14.5,0.4),(15.5,1.0),(16.5,2.0),(17.5,2.5)];
    println!("i  {:?}", resp(&i, 17.5));
    let i2 = [(0.0,0.6),(1.5,1.2),(2.5,3.0),(4.0,7.0),(7.0,9.0),(9.0,7.0),(10.5,3.0),(11.5,0.8),(12.5,0.5),(14.5,0.4),(15.5,1.0),(16.5,2.0),(17.5,2.5)];
    let _ = i2;
    let a = [(0.0,0.6),(1.5,1.2),(2.5,1.0),(4.0,0.6),(7.0,0.8),(9.0,2.0),(11.0,5.0),(13.0,7.0),(15.0,7.0),(16.5,5.0),(17.5,4.0)];
    println!("a  {:?}", resp(&a, 17.5));
    let two = [(0.0,8.0),(8.5,8.0),(8.6,0.5),(17.5,0.5)];
    println!("two-tube {:?}", resp(&two, 17.5));
}
