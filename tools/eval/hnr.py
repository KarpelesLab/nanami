# Band harmonics-to-noise ratio in voiced frames via autocorrelation of band-passed signal.
import sys, numpy as np, scipy.io.wavfile as w, scipy.signal as ss
bands=[(0,1000),(1000,2000),(2000,4000),(4000,7500)]
res={b:[] for b in bands}
for f in sys.argv[1:]:
    fs,x=w.read(f); x=x.astype(float); x=ss.resample_poly(x,16000,fs); fs=16000
    win=int(0.04*fs); hop=win//2
    e=np.array([np.sqrt(np.mean(x[i:i+win]**2)) for i in range(0,len(x)-win,hop)])
    thr=np.percentile(e,60)
    for b in bands:
        if b[0]==0: bb,aa=ss.butter(4,b[1]/(fs/2))
        else: bb,aa=ss.butter(4,[b[0]/(fs/2),b[1]/(fs/2)],'band')
        y=ss.filtfilt(bb,aa,x)
        for k,i in enumerate(range(0,len(x)-win,hop)):
            if e[k]<thr: continue
            s=y[i:i+win]*np.hanning(win); r=np.correlate(s,s,'full')[win-1:]
            if r[0]<=0: continue
            r/=r[0]; lo,hi=int(fs/400),int(fs/70)
            m=r[lo:hi].max()
            m=min(m,0.999)
            res[b].append(10*np.log10(m/(1-m)))
print(" ".join(f"{b[0]}-{b[1]}:{np.median(v):.1f}dB" for b,v in res.items()))
