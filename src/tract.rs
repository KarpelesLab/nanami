//! Vocal tract as a Kelly–Lochbaum digital waveguide (lossy concatenated
//! tubes) with a side branch for the nasal cavity coupled at the velum
//! through a three-way scattering junction.
//!
//! Each section is one sample long (length = c / fs). Waves are pressure
//! waves; junction scattering uses the general N-port form
//!     p_J = 2 Σ A_k p_k⁺ / Σ A_k,   p_k⁻ = p_J − p_k⁺
//! Lip and nostril radiation use Flanagan's piston-in-baffle approximation
//! (parallel radiation resistance/inertance), discretized with the bilinear
//! transform, so the radiation load depends on the mouth opening area.
//!
//! References: Kelly & Lochbaum (1962); Flanagan (1972) "Speech Analysis,
//! Synthesis and Perception"; Välimäki & Karjalainen (1994); Story (1995).

pub const SPEED_OF_SOUND: f64 = 350.0; // m/s, warm humid air

/// First-order radiation reflectance at an open end of area `area` (cm²).
#[derive(Clone, Default)]
struct Radiation {
    b0: f64,
    b1: f64,
    a1: f64,
    x1: f64,
    y1: f64,
    area: f64,
    fs: f64,
}

impl Radiation {
    fn new(fs: f64, area: f64) -> Self {
        let mut r = Radiation { fs, ..Default::default() };
        r.set_area(area);
        r
    }

    fn set_area(&mut self, area_cm2: f64) {
        let area_cm2 = area_cm2.max(0.01);
        if (area_cm2 - self.area).abs() < 1e-4 * self.area {
            return;
        }
        self.area = area_cm2;
        // Normalized by tube impedance rho c / A:
        //   R = 128 / (9 pi^2), L = 8 sqrt(A) / (3 pi^1.5 c)
        let a = area_cm2 * 1e-4;
        let r = 128.0 / (9.0 * std::f64::consts::PI.powi(2));
        let l = 8.0 * a.sqrt() / (3.0 * std::f64::consts::PI.powf(1.5) * SPEED_OF_SOUND);
        // Reflectance (z - 1) / (z + 1), z = s l R / (R + s l)
        //   = (s l (R - 1) - R) / (s l (R + 1) + R)
        let (nb1, nb0, na1, na0) = (l * (r - 1.0), -r, l * (r + 1.0), r);
        let k = 2.0 * self.fs;
        let a0 = na1 * k + na0;
        self.b0 = (nb1 * k + nb0) / a0;
        self.b1 = (nb0 - nb1 * k) / a0;
        self.a1 = (na0 - na1 * k) / a0;
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// Nasal cavity area function (cm²), velum → nostrils, after Story (1995)/Maeda,
/// simplified. Sampled along normalized position.
fn nasal_area(x: f64) -> f64 {
    // Nasopharynx widens into the main cavity, then narrows toward the nares.
    let pts = [(0.0, 1.6), (0.15, 2.4), (0.35, 3.6), (0.6, 3.0), (0.8, 1.8), (1.0, 0.9)];
    piecewise(&pts, x)
}

pub fn piecewise(pts: &[(f64, f64)], x: f64) -> f64 {
    if x <= pts[0].0 {
        return pts[0].1;
    }
    for w in pts.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if x <= x1 {
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        }
    }
    pts[pts.len() - 1].1
}

pub struct Tract {
    pub n: usize,
    fs: f64,
    /// Right-going (toward lips) and left-going pressure waves per section.
    fwd: Vec<f64>,
    bwd: Vec<f64>,
    nfwd: Vec<f64>,
    nbwd: Vec<f64>,
    /// Current and target areas (cm²); areas glide to the target over a control block.
    area: Vec<f64>,
    target: Vec<f64>,
    area_step: Vec<f64>,
    loss: Vec<f64>,
    // Nasal branch
    pub nose_n: usize,
    pub velum_index: usize,
    nose_f: Vec<f64>,
    nose_b: Vec<f64>,
    nose_nf: Vec<f64>,
    nose_nb: Vec<f64>,
    nose_area: Vec<f64>,
    nose_loss: Vec<f64>,
    velum: f64,
    velum_target: f64,
    velum_step: f64,
    lips: Radiation,
    nostrils: Radiation,
    /// Glottal-end reflection coefficient (depends on glottal opening).
    pub glottal_reflection: f64,
    pending_fwd: Vec<f64>,
    pending_bwd: Vec<f64>,
    /// Noise generated at the lip opening radiates directly.
    pending_direct: f64,
}

impl Tract {
    /// `length_cm`: vocal tract length glottis→lips. `nose_cm`: velum→nostrils.
    pub fn new(fs: f64, length_cm: f64, nose_cm: f64) -> Tract {
        let seg = SPEED_OF_SOUND / fs * 100.0; // cm per section
        let n = ((length_cm / seg).round() as usize).max(8);
        let nose_n = ((nose_cm / seg).round() as usize).max(4);
        let velum_index = ((n as f64) * 0.47).round() as usize;
        let nose_area: Vec<f64> = (0..nose_n).map(|i| nasal_area(i as f64 / (nose_n - 1) as f64)).collect();
        let nose_loss = nose_area.iter().map(|&a| section_loss(a) * 2.5).collect();
        Tract {
            n,
            fs,
            fwd: vec![0.0; n],
            bwd: vec![0.0; n],
            nfwd: vec![0.0; n],
            nbwd: vec![0.0; n],
            area: vec![3.0; n],
            target: vec![3.0; n],
            area_step: vec![0.0; n],
            loss: vec![section_loss(3.0); n],
            nose_n,
            velum_index,
            nose_f: vec![0.0; nose_n],
            nose_b: vec![0.0; nose_n],
            nose_nf: vec![0.0; nose_n],
            nose_nb: vec![0.0; nose_n],
            nose_area,
            nose_loss,
            velum: 0.0,
            velum_target: 0.0,
            velum_step: 0.0,
            lips: Radiation::new(fs, 3.0),
            nostrils: Radiation::new(fs, 0.9),
            glottal_reflection: 0.75,
            pending_fwd: vec![0.0; n],
            pending_bwd: vec![0.0; n],
            pending_direct: 0.0,
        }
    }

    /// Section length in cm.
    pub fn section_cm(&self) -> f64 {
        SPEED_OF_SOUND / self.fs * 100.0
    }

    pub fn areas(&self) -> &[f64] {
        &self.area
    }

    /// Set new target areas reached linearly over `steps` samples.
    #[allow(clippy::needless_range_loop)]
    pub fn set_target(&mut self, areas: &[f64], velum: f64, steps: usize) {
        let steps = steps.max(1) as f64;
        for i in 0..self.n {
            self.target[i] = areas[i].max(0.0);
            self.area_step[i] = (self.target[i] - self.area[i]) / steps;
        }
        self.velum_target = velum.max(0.0);
        self.velum_step = (self.velum_target - self.velum) / steps;
    }

    /// Snap areas immediately (no glide), e.g. at startup.
    pub fn snap(&mut self, areas: &[f64], velum: f64) {
        self.set_target(areas, velum, 1);
        self.advance_areas();
    }

    fn advance_areas(&mut self) {
        for i in 0..self.n {
            let a = self.area[i] + self.area_step[i];
            // Clamp so we never overshoot the target.
            self.area[i] = if self.area_step[i] >= 0.0 { a.min(self.target[i]) } else { a.max(self.target[i]) };
            self.loss[i] = section_loss(self.area[i]);
        }
        let v = self.velum + self.velum_step;
        self.velum = if self.velum_step >= 0.0 { v.min(self.velum_target) } else { v.max(self.velum_target) };
        // The velopharyngeal port is the first section of the nasal branch.
        self.nose_area[0] = self.velum.max(1e-5);
        self.nose_loss[0] = section_loss(self.nose_area[0]) * 2.5;
        self.lips.set_area(self.area[self.n - 1]);
    }

    /// Queue a jet-turbulence source at fractional section `pos`, radiating
    /// downstream. Sources at the lip opening radiate directly.
    #[inline]
    pub fn add_noise(&mut self, pos: f64, value: f64) {
        if pos >= (self.n - 1) as f64 {
            self.pending_direct += value;
            return;
        }
        let (i, f) = self.split(pos);
        self.pending_fwd[i] += value * (1.0 - f);
        self.pending_fwd[i + 1] += value * f;
    }

    /// Queue an obstacle (dipole) source, e.g. the jet hitting the teeth.
    #[inline]
    pub fn add_dipole(&mut self, pos: f64, value: f64) {
        let (i, f) = self.split(pos);
        let (a, b) = (0.5 * value * (1.0 - f), 0.5 * value * f);
        self.pending_fwd[i] += a;
        self.pending_bwd[i] -= a;
        self.pending_fwd[i + 1] += b;
        self.pending_bwd[i + 1] -= b;
    }

    fn split(&self, pos: f64) -> (usize, f64) {
        let pos = pos.clamp(1.0, (self.n - 1) as f64 - 1e-6);
        let i = pos.floor() as usize;
        (i, pos - i as f64)
    }

    /// Advance one sample. `glottal` is the volume-velocity (derivative) source
    /// injected at the glottis. Returns radiated output (lips + nose).
    pub fn step(&mut self, glottal: f64) -> f64 {
        self.advance_areas();
        let n = self.n;
        const MIN_A: f64 = 1e-5;

        // Glottal termination: flow source into section 0 (impedance rho c / A0).
        self.nfwd[0] = self.bwd[0] * self.glottal_reflection + glottal / self.area[0].max(0.05);

        // Lip termination via radiation reflectance; output = transmitted flow.
        let incident = self.fwd[n - 1];
        let reflected = self.lips.process(incident);
        self.nbwd[n - 1] = reflected;
        let lip_out = (incident - reflected) * self.area[n - 1];

        // Interior two-port junctions between section j-1 and j.
        let v = self.velum_index;
        for j in 1..n {
            let (a1, a2) = (self.area[j - 1].max(MIN_A), self.area[j].max(MIN_A));
            let (f, b) = (self.fwd[j - 1], self.bwd[j]);
            if j == v {
                // Three-way junction with the nasal branch.
                let an = self.nose_area[0];
                let nb = self.nose_b[0];
                let sum = a1 + a2 + an;
                let pj = 2.0 * (a1 * f + a2 * b + an * nb) / sum;
                self.nfwd[j] = pj - b;
                self.nbwd[j - 1] = pj - f;
                self.nose_nf[0] = pj - nb;
            } else {
                let pj = 2.0 * (a1 * f + a2 * b) / (a1 + a2);
                self.nfwd[j] = pj - b;
                self.nbwd[j - 1] = pj - f;
            }
        }

        // Nasal branch: junction 0 handled above; interior + nostrils.
        let nn = self.nose_n;
        for j in 1..nn {
            let (a1, a2) = (self.nose_area[j - 1], self.nose_area[j]);
            let (f, b) = (self.nose_f[j - 1], self.nose_b[j]);
            let pj = 2.0 * (a1 * f + a2 * b) / (a1 + a2);
            self.nose_nf[j] = pj - b;
            self.nose_nb[j - 1] = pj - f;
        }
        let nincident = self.nose_f[nn - 1];
        let nreflected = self.nostrils.process(nincident);
        self.nose_nb[nn - 1] = nreflected;
        let nose_out = (nincident - nreflected) * self.nose_area[nn - 1];

        // Losses and turbulence injection. The jet noise radiates downstream
        // (toward the lips): injecting it as a symmetric dipole right behind a
        // near-closed constriction would cancel against its own reflection.
        for i in 0..n {
            let g = 1.0 - self.loss[i];
            self.fwd[i] = self.nfwd[i] * g + self.pending_fwd[i];
            self.bwd[i] = self.nbwd[i] * g + self.pending_bwd[i];
            self.pending_fwd[i] = 0.0;
            self.pending_bwd[i] = 0.0;
        }
        // Direct radiation of noise produced at the lips, scaled like flow
        // through a typical open mouth.
        let direct = std::mem::take(&mut self.pending_direct) * 3.0;
        for i in 0..nn {
            let g = 1.0 - self.nose_loss[i];
            self.nose_f[i] = self.nose_nf[i] * g;
            self.nose_b[i] = self.nose_nb[i] * g;
        }
        lip_out + nose_out + direct
    }
}

/// Per-sample wave attenuation for a section of area `a` (cm²): a constant
/// (wall vibration/radiation) term plus viscous/thermal boundary-layer losses
/// that scale with perimeter/area ∝ 1/sqrt(A).
#[inline]
fn section_loss(a: f64) -> f64 {
    (0.0006 + 0.0012 / a.max(0.02).sqrt()).min(0.2)
}
