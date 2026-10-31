// Japanese sung vowels of the "idol" voice, fitted by analysis-by-synthesis
// (tools/eval/vowel_env.py) to a recorded pop singer's per-vowel spectral
// envelopes; other vowels fall back to the female set.
pub const VOWEL_SHAPES_IDOL: &[(&str, [f64; 6], [f64; 8])] = &[
    ("a", [0.2820, 0.7915, 4.2070, 0.5383, 1.4979, 0.4228], [0.0; 8]),
    ("i", [0.6806, 0.5281, 3.9712, 0.9720, 0.5107, 0.8584], [0.0; 8]),
    ("ɯ", [0.6216, 0.6194, 1.6883, 1.0540, 0.5401, 0.9385], [0.0; 8]),
    ("e", [0.5778, 0.8564, 4.9037, 1.2394, 0.5008, 0.5633], [0.0; 8]),
    ("o", [0.3658, 0.7545, 1.5152, 1.2679, 1.0757, 1.4977], [0.0; 8]),
];
