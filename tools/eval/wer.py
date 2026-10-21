import sys,re
ref=[l.strip() for l in open(sys.argv[1]) if l.strip()]
hyp={}
for l in open(sys.argv[2]):
    p=l.split('|')
    if len(p)>=2 and p[0].strip().startswith('s'): hyp[int(p[0].strip()[1:-4])]=p[1]
norm=lambda s: re.sub(r"[^a-z' ]"," ",s.lower()).split()
def ed(a,b):
    d=list(range(len(b)+1))
    for i in range(1,len(a)+1):
        pr=d[:]; d[0]=i
        for j in range(1,len(b)+1): d[j]=min(pr[j]+1,d[j-1]+1,pr[j-1]+(a[i-1]!=b[j-1]))
    return d[-1]
E=N=0
for i,r in enumerate(ref,1):
    h=hyp.get(i,""); e=ed(norm(r),norm(h)); E+=e; N+=len(norm(r))
    if len(sys.argv)>3 or e: print(f"{i:2} e={e} | {h.strip()}")
print(f"WER {100*E/N:.1f}%  ({E}/{N})")
