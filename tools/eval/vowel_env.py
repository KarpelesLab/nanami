"""Per-vowel spectral envelopes from harmonic amplitudes (high-F0 safe).

usage: vowel_env.py WAV:TSV [WAV:TSV ...]
TSV rows from imitate.py: kana, romaji, first, vowel_start, last (s, clip time).
At singing pitches LPC formant tracking fails (harmonics are 300–500 Hz
apart), but every frame samples the envelope at its harmonics; pooling many
frames at different F0s (notes, vibrato) fills the envelope in. Per vowel we
take the median harmonic level in 1/12-octave bins, re the frame's peak
harmonic, and report the peaks as formant estimates.
"""
import sys, numpy as np, parselmouth

VOW = {"a": "a", "i": "i", "u": "u", "e": "e", "o": "o"}
BINS = 300 * 2 ** (np.arange(0, 12 * 4.4) / 12)  # 300 Hz – ~6.3 kHz


def frames(wav, tsv):
    snd = parselmouth.Sound(wav)
    sr = snd.sampling_frequency
    x = snd.values[0]
    p = snd.to_pitch_ac(time_step=0.01, pitch_floor=120, pitch_ceiling=1000)
    pt, pf = p.xs(), p.selected_array["frequency"]
    rows = [l.rstrip("\n").split("\t") for l in open(tsv)]
    N = 2048
    win = np.hanning(N)
    fr = np.fft.rfftfreq(N * 4, 1 / sr)
    for i, r in enumerate(rows):
        v = r[1][-1:]
        if v not in VOW:
            continue
        a = float(r[3]) + 0.04
        b = float(rows[i + 1][2]) - 0.03 if i + 1 < len(rows) else a + 0.3
        for k in np.where((pt >= a) & (pt <= b) & (pf > 0))[0]:
            c = int(pt[k] * sr)
            seg = x[c - N // 2:c + N // 2]
            if len(seg) < N:
                continue
            S = np.abs(np.fft.rfft(seg * win, N * 4))
            f0 = pf[k]
            hs = np.arange(1, int(6500 / f0))
            amps = []
            for h in hs:
                m = (fr > (h - 0.3) * f0) & (fr < (h + 0.3) * f0)
                amps.append(20 * np.log10(S[m].max() + 1e-12))
            amps = np.array(amps)
            yield v, hs * f0, amps - amps.max()


def envelopes(pairs):
    pool = {v: [] for v in VOW}
    for wav, tsv in pairs:
        for v, f, a in frames(wav, tsv):
            pool[v].append((f, a))
    out = {}
    for v, lst in pool.items():
        if not lst:
            continue
        F = np.concatenate([f for f, _ in lst])
        A = np.concatenate([a for _, a in lst])
        env = np.full(len(BINS), np.nan)
        for j, c in enumerate(BINS):
            m = (F > c * 2 ** (-1 / 24)) & (F < c * 2 ** (1 / 24))
            if m.sum() >= 4:
                env[j] = np.percentile(A[m], 75)  # upper quartile ≈ harmonics near the envelope
        # light smoothing over 3 bins
        e = np.copy(env)
        for j in range(1, len(env) - 1):
            w = env[j - 1:j + 2]
            if np.isfinite(w).all():
                e[j] = (w[0] + 2 * w[1] + w[2]) / 4
        out[v] = (e, len(lst))
    return out


def peaks(e):
    pk = [j for j in range(1, len(e) - 1) if np.isfinite(e[j - 1:j + 2]).all() and e[j] >= e[j - 1] and e[j] > e[j + 1]]
    return [(BINS[j], e[j]) for j in pk]


if __name__ == "__main__":
    pairs = [a.split(":") for a in sys.argv[1:]]
    for v, (e, n) in envelopes(pairs).items():
        pk = " ".join(f"{f:.0f}({a:+.0f})" for f, a in peaks(e))
        print(f"/{v}/ n={n:4d}  peaks: {pk}")
