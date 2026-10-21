import sys, glob, numpy as np, scipy.io.wavfile as w, scipy.signal as ss
acc=None
for f in sys.argv[1:]:
    fs,x=w.read(f); x=x.astype(float)
    fr,P=ss.welch(x,fs,nperseg=2048)
    acc=P if acc is None else acc+P
L=10*np.log10(acc+1e-20)
# third-octave bands
cs=[125,160,200,250,315,400,500,630,800,1000,1250,1600,2000,2500,3150,4000,5000,6300,8000,10000]
vals=[]
for c in cs:
    m=(fr>=c/2**(1/6))&(fr<c*2**(1/6)); vals.append(10*np.log10(acc[m].sum()+1e-20))
ref=max(vals)
print(" ".join(f"{c}:{v-ref:.0f}" for c,v in zip(cs,vals)))
