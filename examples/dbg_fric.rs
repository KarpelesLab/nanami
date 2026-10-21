use nanami::articulation::*;
use nanami::synth::*;
fn main() {
    let voice = nanami::voice::Voice::nanami();
    for (name, tip_pos, tip_a, obst, lip) in [("s", 0.915, 0.055, 1.0, 4.5), ("s-noobst", 0.915, 0.055, 0.0, 4.5), ("sh", 0.865, 0.11, 1.0, 1.2), ("s-wide", 0.915, 0.1, 1.0, 4.5)] {
        let mut art = Articulators::default();
        let v = nanami::phoneme::VowelShape::lookup("a").unwrap();
        v.apply(&mut art); art.lip_area = lip;
        art.tip_pos = tip_pos; art.tip_area = tip_a; art.obstacle = obst; art.glottis = 0.24; art.lung = 750.0;
        let frames: Vec<ControlFrame> = (0..1000).map(|_| ControlFrame { art: art.clone(), f0: 200.0, rd: 1.0, breath: 0.0, creak: 0.0, perturbation: 1.0 }).collect();
        let mut e = Engine::new(voice.clone(), 3); e.keep_trace = true;
        let raw = e.render_internal(&frames);
        let mut areas = Vec::new(); area_function(&art, &voice.anatomy, e.tract.n, &mut areas);
        let (imin, amin) = min_constriction(&areas, &voice.anatomy);
        let rms = (raw[20000..].iter().map(|x| x*x).sum::<f64>() / (raw.len()-20000) as f64).sqrt();
        let tr = e.trace.last().unwrap();
        println!("{name}: n={} imin={imin} amin={amin:.3} rms={rms:.4} Po={:.0} fric={:.2}", e.tract.n, tr.oral_pressure, tr.frication);
        println!("   areas tail {:?}", areas[30..].iter().map(|a| (a*100.0).round()/100.0).collect::<Vec<_>>());
    }
}
