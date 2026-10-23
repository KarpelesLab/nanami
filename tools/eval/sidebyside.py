"""Side-by-side spectrograms (0-8 kHz) of two recordings of the same text.
usage: sidebyside.py a.wav b.wav out.png [title_a] [title_b]"""
import sys
import numpy as np
import scipy.io.wavfile as w
import scipy.signal as ss
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

fig, axes = plt.subplots(2, 1, figsize=(14, 8), sharex=False)
for ax, f, title in zip(axes, sys.argv[1:3], (sys.argv[4:6] + ["a", "b"])[:2] if len(sys.argv) > 4 else ["a", "b"]):
    fs, x = w.read(f)
    x = x.astype(float)
    if x.ndim > 1:
        x = x.mean(1)
    x = ss.resample_poly(x, 16000, fs)
    x /= np.abs(x).max()
    fr, t, S = ss.spectrogram(x, 16000, nperseg=400, noverlap=360, nfft=1024)
    D = 10 * np.log10(S + 1e-12)
    ax.pcolormesh(t, fr, D, vmin=D.max() - 75, vmax=D.max(), cmap="magma", shading="auto")
    ax.set_title(title)
    ax.set_ylabel("Hz")
plt.tight_layout()
plt.savefig(sys.argv[3], dpi=80)
