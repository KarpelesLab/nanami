import sys, numpy as np, scipy.io.wavfile as w, scipy.signal as ss, torch
torch.hub.set_dir(sys.argv[1])
m = torch.hub.load("tarepan/SpeechMOS:v1.2.0", "utmos22_strong", trust_repo=True)
scores=[]
for f in sys.argv[2:]:
    fs,x=w.read(f); x=x.astype(np.float32); x/=max(1e-9,np.abs(x).max())
    x=ss.resample_poly(x,16000,fs).astype(np.float32)
    with torch.no_grad(): s=m(torch.from_numpy(x)[None], 16000).item()
    scores.append(s)
    if len(sys.argv) <= 6: print(f.split('/')[-1], f"{s:.3f}")
print(f"UTMOS mean {np.mean(scores):.3f} over {len(scores)}")
