"""Parallel comparison of natural recordings vs. NANAMI renderings of the same text.

usage: compare.py REF_DIR TEXTS.txt NANAMI_DIR [N]
  REF_DIR/f{i}.wav (or l{i}.wav) and NANAMI_DIR/n{i}.wav for line i of TEXTS.txt.
Prints mean ± sd of prosodic and voice-quality measures for both, computed
with Praat (parselmouth), so the differences that matter can be targeted.
"""
import glob, os, re, sys
import numpy as np
import parselmouth
from parselmouth.praat import call


def syllables(text):
    n = 0
    for w in re.findall(r"[a-z']+", text.lower()):
        w = re.sub(r"e$", "", w) or w
        n += max(1, len(re.findall(r"[aeiouy]+", w)))
    return n


def measures(path, text):
    snd = parselmouth.Sound(path)
    snd = call(snd, "Resample", 16000, 50)
    intensity = snd.to_intensity(minimum_pitch=100)
    iv = intensity.values[0]
    t_int = intensity.xs()
    thr = iv.max() - 30
    speech = np.where(iv > thr)[0]
    t0, t1 = t_int[speech[0]], t_int[speech[-1]]
    dur = t1 - t0
    inside = iv[speech[0]:speech[-1] + 1]
    # pauses: runs ≥ 100 ms below max−35 dB inside the utterance
    low = inside < iv.max() - 35
    dt = t_int[1] - t_int[0]
    pauses, run = 0.0, 0
    for v in list(low) + [False]:
        if v:
            run += 1
        else:
            if run * dt >= 0.1:
                pauses += run * dt
            run = 0
    pitch = snd.to_pitch_ac(time_step=0.01, pitch_floor=75, pitch_ceiling=600)
    f0 = pitch.selected_array["frequency"]
    voiced = f0 > 0
    st = 12 * np.log2(f0[voiced] / 100.0)
    # F0 movement: mean |dF0/dt| over consecutive voiced frames (st/s)
    idx = np.where(voiced)[0]
    d = [abs(st[k + 1] - st[k]) / 0.01 for k in range(len(idx) - 1) if idx[k + 1] == idx[k] + 1]
    vel = np.median(d) if d else np.nan
    # loudness dynamics in voiced frames and syllable-rate modulation depth
    env = inside - inside.mean()
    spec = np.abs(np.fft.rfft(env * np.hanning(len(env)))) ** 2
    fr = np.fft.rfftfreq(len(env), dt)
    mod = 10 * np.log10(spec[(fr > 2) & (fr < 8)].sum() / max(1e-12, spec[(fr > 0.3)].sum()))
    pp = call(snd, "To PointProcess (periodic, cc)", 75, 600)
    jitter = call(pp, "Get jitter (local)", 0, 0, 0.0001, 0.02, 1.3)
    shimmer = call([snd, pp], "Get shimmer (local)", 0, 0, 0.0001, 0.02, 1.3, 1.6)
    harm = call(snd, "To Harmonicity (cc)", 0.01, 75, 0.1, 1.0)
    hnr = call(harm, "Get mean", 0, 0)
    pc = call(snd, "To PowerCepstrogram", 60, 0.002, 5000, 50)
    cpps = call(pc, "Get CPPS", "yes", 0.02, 0.0005, 60, 330, 0.05, "Parabolic", 0.001, 0.05, "Straight", "Robust")
    return {
        "dur s": dur,
        "syl/s": syllables(text) / max(0.1, dur - pauses),
        "pause s": pauses,
        "voiced %": 100 * voiced[int(t0 / 0.01):int(t1 / 0.01)].mean(),
        "F0 med Hz": np.median(f0[voiced]),
        "F0 range st": np.percentile(st, 90) - np.percentile(st, 10),
        "F0 sd st": np.std(st),
        "F0 vel st/s": vel,
        "int sd dB": np.std(inside[inside > thr]),
        "mod 2-8Hz dB": mod,
        "jitter %": 100 * jitter,
        "shimmer %": 100 * shimmer,
        "HNR dB": hnr,
        "CPPS dB": cpps,
    }


def main():
    ref_dir, texts, nan_dir = sys.argv[1:4]
    n = int(sys.argv[4]) if len(sys.argv) > 4 else 40
    lines = [l.strip() for l in open(texts)][:n]
    R, N = [], []
    for i, text in enumerate(lines):
        ref = (glob.glob(os.path.join(ref_dir, f"[fl]{i}.wav")) or [None])[0]
        nan = os.path.join(nan_dir, f"n{i}.wav")
        if not ref or not os.path.exists(nan):
            continue
        try:
            R.append(measures(ref, text))
            N.append(measures(nan, text))
        except Exception as e:  # unvoiced/empty edge cases
            print("skip", i, e, file=sys.stderr)
    print(f"{len(R)} pairs\n{'measure':14} {'natural':>16} {'nanami':>16}")
    for k in R[0]:
        r = np.array([m[k] for m in R], float)
        q = np.array([m[k] for m in N], float)
        print(f"{k:14} {np.nanmean(r):8.2f} ±{np.nanstd(r):6.2f} {np.nanmean(q):8.2f} ±{np.nanstd(q):6.2f}")


if __name__ == "__main__":
    main()
