"""Transcribe WAV files with faster-whisper: asr.py MODEL file.wav ..."""
import sys
import numpy as np
import scipy.io.wavfile as w
import scipy.signal as ss
from faster_whisper import WhisperModel

m = WhisperModel(sys.argv[1], device="cuda", compute_type="float16")
for f in sys.argv[2:]:
    fs, x = w.read(f)
    x = x.astype(np.float32)
    if x.ndim > 1:
        x = x.mean(1)
    x /= max(1e-9, np.abs(x).max())
    x = ss.resample_poly(x, 16000, fs).astype(np.float32)
    segs, _ = m.transcribe(x, language="en", beam_size=5, condition_on_previous_text=False, vad_filter=False)
    segs = list(segs)
    lp = np.mean([s.avg_logprob for s in segs]) if segs else float("nan")
    print(f.split("/")[-1], "|", " ".join(s.text.strip() for s in segs), f"| logprob {lp:.2f}")
