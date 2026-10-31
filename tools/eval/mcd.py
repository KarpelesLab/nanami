"""Mel-cepstral distance between a recording and its copy-synthesis.

usage: mcd.py CLIP.tsv REF.wav SYN.wav [...more triples]
Both renditions share the timing in CLIP.tsv (imitate.py), so each mora's
vowel is compared with DTW over a short window (absorbs small lags). Gain and
spectral tilt are partly removed (c0 dropped), so this scores formant
pattern — peaks *and* valleys — which pooled envelopes and LTAS miss.
Prints per-vowel and overall MCD (dB).
"""
import sys, numpy as np, scipy.io.wavfile as wf, scipy.signal as ss
from scipy.fft import dct

SR = 16000
import os
LO = int(os.environ.get("MCD_LO", "2"))  # skip c0 (gain) and c1 (tilt)
def melfb(n=40, nfft=512, lo=100, hi=7600):
    m = lambda f: 2595 * np.log10(1 + f / 700)
    im = lambda x: 700 * (10 ** (x / 2595) - 1)
    pts = im(np.linspace(m(lo), m(hi), n + 2))
    bins = np.fft.rfftfreq(nfft, 1 / SR)
    fb = np.zeros((n, len(bins)))
    for i in range(n):
        a, c, b = pts[i:i + 3]
        fb[i] = np.clip(np.minimum((bins - a) / (c - a), (b - bins) / (b - c)), 0, None)
    return fb
FB = melfb()

def load(p):
    sr, x = wf.read(p)
    x = x.astype(float)
    if x.ndim > 1: x = x.mean(1)
    return ss.resample_poly(x, SR, sr) / (np.abs(x).max() + 1e-9)

def ceps(x, a, b):
    seg = x[max(0, int(a * SR) - 200):int(b * SR) + 200]
    if len(seg) < 600: return None
    _, _, Z = ss.stft(seg, SR, nperseg=400, noverlap=240, nfft=512)
    P = np.abs(Z) ** 2
    M = np.log(FB @ P + 1e-10)
    C = dct(M, axis=0, norm="ortho")[LO:21]
    return C.T

def dtw(A, B):
    D = np.sqrt(((A[:, None, :] - B[None, :, :]) ** 2).sum(-1))
    n, m = D.shape
    acc = np.full((n + 1, m + 1), np.inf); acc[0, 0] = 0
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            acc[i, j] = D[i - 1, j - 1] + min(acc[i - 1, j], acc[i, j - 1], acc[i - 1, j - 1])
    return acc[n, m] / (n + m)

K = 10 / np.log(10) * np.sqrt(2)
def score(tsv, ref, syn):
    rows = [l.rstrip("\n").split("\t") for l in open(tsv)]
    R, S = load(ref), load(syn)
    out = []
    for i, r in enumerate(rows):
        v = r[1][-1:]
        if v not in "aiueo": continue
        a = float(r[3]) + 0.03
        b = min(float(rows[i + 1][2]) if i + 1 < len(rows) else a + 0.3, a + 0.35)
        if b - a < 0.05: continue
        A, B = ceps(R, a, b), ceps(S, a, b)
        if A is None or B is None: continue
        out.append((v, K * dtw(A, B)))
    return out

if __name__ == "__main__":
    args = sys.argv[1:]
    allv = []
    for k in range(0, len(args), 3):
        allv += score(*args[k:k + 3])
    for v in "aiueo":
        d = [x for w, x in allv if w == v]
        if d: print(f"/{v}/ {np.mean(d):5.2f} (n={len(d)})")
    print(f"MCD {np.mean([x for _, x in allv]):.2f} dB")
