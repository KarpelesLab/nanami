"""Copy-synthesis: transcribe a sung (or spoken) Japanese recording into NML.

usage: imitate.py VOCAL.wav START END "かな text" OUT.nml [--ref OUT_REF.wav]

Kana are split into morae and force-aligned to the recording with the MMS
CTC aligner (torchaudio). Each mora becomes a `<sing>` note starting at its
vowel onset (NANAMI's note time), carrying the singer's own F0 contour as
40 ms melisma steps, so a rendering differs from the original only in
voice and articulation — the timbre comparison we want. Vibrato is part of
the contour, so the song is written with vibrato="0".
"""
import re, subprocess, sys
import numpy as np
import parselmouth
import torch, torchaudio

SMALL = {"ゃ": "ya", "ゅ": "yu", "ょ": "yo", "ぁ": "a", "ぃ": "i", "ぅ": "u", "ぇ": "e", "ぉ": "o"}
ROW = {
    "あ": "a", "い": "i", "う": "u", "え": "e", "お": "o",
    "か": "ka", "き": "ki", "く": "ku", "け": "ke", "こ": "ko", "が": "ga", "ぎ": "gi", "ぐ": "gu", "げ": "ge", "ご": "go",
    "さ": "sa", "し": "shi", "す": "su", "せ": "se", "そ": "so", "ざ": "za", "じ": "ji", "ず": "zu", "ぜ": "ze", "ぞ": "zo",
    "た": "ta", "ち": "chi", "つ": "tsu", "て": "te", "と": "to", "だ": "da", "ぢ": "ji", "づ": "zu", "で": "de", "ど": "do",
    "な": "na", "に": "ni", "ぬ": "nu", "ね": "ne", "の": "no",
    "は": "ha", "ひ": "hi", "ふ": "fu", "へ": "he", "ほ": "ho", "ば": "ba", "び": "bi", "ぶ": "bu", "べ": "be", "ぼ": "bo",
    "ぱ": "pa", "ぴ": "pi", "ぷ": "pu", "ぺ": "pe", "ぽ": "po",
    "ま": "ma", "み": "mi", "む": "mu", "め": "me", "も": "mo", "や": "ya", "ゆ": "yu", "よ": "yo",
    "ら": "ra", "り": "ri", "る": "ru", "れ": "re", "ろ": "ro", "わ": "wa", "を": "o", "ゔ": "vu",
}


def hira(s):
    return "".join(chr(ord(c) - 0x60) if "ァ" <= c <= "ヶ" else c for c in s)


def morae(text):
    """[(kana, romaji)]; ー and っ handled by merging."""
    out = []
    for c in hira(re.sub(r"\s+", "", text)):
        if c in SMALL and out and out[-1][0] not in ("ん", "っ", "ー"):
            k, r = out[-1]
            if c in "ゃゅょ":
                base = r[:-1]  # き → k, し → sh, ち → ch, じ → j
                r = base + (SMALL[c][1:] if base.endswith(("sh", "ch", "j")) else SMALL[c])
            else:
                r = r[:-1] + SMALL[c]
            out[-1] = (k + c, r)
        elif c == "ん":
            out.append((c, "n"))
        elif c == "っ":
            out.append((c, ""))
        elif c == "ー":
            out.append((c, out[-1][1][-1] if out else "a"))
        elif c in ROW:
            out.append((c, ROW[c]))
    # っ joins the next mora (gemination: doubled consonant letter).
    merged = []
    for i, (k, r) in enumerate(out):
        if merged and merged[-1][0].endswith("っ") and merged[-1][1] == "":
            pk, _ = merged.pop()
            merged.append((pk + k, r[0] + r))
        else:
            merged.append((k, r))
    return merged


def align(wav16, mor):
    bundle = torchaudio.pipelines.MMS_FA
    model = bundle.get_model(with_star=False)
    dic = bundle.get_dict(star=None)
    letters, owner = [], []
    for i, (_, r) in enumerate(mor):
        for ch in r:
            letters.append(dic[ch])
            owner.append(i)
    with torch.inference_mode():
        em, _ = model(torch.from_numpy(wav16).float()[None])
    ali, _ = torchaudio.functional.forced_align(em, torch.tensor([letters], dtype=torch.int32), blank=0)
    ali = ali[0].tolist()
    spans = torchaudio.functional.merge_tokens(torch.tensor(ali), torch.ones(len(ali)))
    fps = em.shape[1] / (len(wav16) / 16000)
    return [(owner[j], s.start / fps, s.end / fps, mor[owner[j]][1], j) for j, s in enumerate(spans)]


def main():
    wav, t0, t1, text, out = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), sys.argv[4], sys.argv[5]
    ref = sys.argv[sys.argv.index("--ref") + 1] if "--ref" in sys.argv else None
    pcm = subprocess.run(["ffmpeg", "-v", "error", "-ss", str(t0), "-to", str(t1), "-i", wav, "-ac", "1", "-ar", "16000", "-f", "f32le", "-"], capture_output=True).stdout
    x16 = np.frombuffer(pcm, np.float32).copy()
    x16 /= max(1e-6, np.abs(x16).max())
    mor = morae(text)
    spans = align(x16, mor)
    # Per mora: first-letter start, vowel (last letter) start, last-letter end.
    first, vstart, last = {}, {}, {}
    for i, a, b, r, j in spans:
        first.setdefault(i, a)
        vstart[i] = a  # overwritten until the last letter (the vowel / ん)
        last[i] = b
    snd = parselmouth.Sound(wav).extract_part(t0, t1)
    if ref:
        snd.save(ref, "WAV")
    pitch = snd.to_pitch_ac(time_step=0.01, pitch_floor=80, pitch_ceiling=1100)
    pt, pf = pitch.xs(), pitch.selected_array["frequency"]
    ok = pf > 0
    f0 = lambda t: float(np.interp(t, pt[ok], pf[ok]))
    toks, t = [], 0.0
    n = len(mor)
    lead = 0.37  # NANAMI starts at 0.12 s, plus the 0.25 s song lead-in
    first_on = vstart[0]
    # Rest before the first note so the rendering lines up with the original.
    if first_on > lead + 0.05:
        toks.append(f"_:{first_on - lead:.3f}")
    for i, (kana, _) in enumerate(mor):
        on = vstart[i]
        nxt = vstart[i + 1] if i + 1 < n else last[i] + 0.05
        # CTC spans are spikes: the vowel really lasts as long as voicing
        # continues; a silent stretch > 150 ms before the next mora is a rest.
        stop = first[i + 1] if i + 1 < n else t1 - t0
        k = np.searchsorted(pt, on)
        while k < len(pt) and pt[k] < stop and pf[k] > 0:
            k += 1
        voice_end = pt[min(k, len(pt) - 1)] + 0.02
        rest = 0.0
        if i + 1 < n and first[i + 1] - voice_end > 0.15:
            end = max(voice_end, on + 0.06)
            rest = nxt - end
        elif i + 1 == n:
            end = max(voice_end, on + 0.1)
        else:
            end = nxt
        steps, s = [], on
        while s < end - 1e-3:
            d = min(0.04, end - s)
            mid = s + d / 2
            steps.append(f"{f0(mid):.1f}hz:{d:.3f}")
            s += d
        lyric = "-" if kana == "ー" else kana
        legato = "~" if rest == 0.0 and i + 1 < n else ""
        toks.append(f"{lyric}{legato}@{','.join(steps)}")
        if rest > 0:
            toks.append(f"_:{rest:.3f}")
    with open(out, "w") as fh:
        fh.write('<sing tempo="60" vibrato="0" portamento="0.012" lang="ja">\n')
        fh.write(" ".join(toks) + "\n</sing>\n")
    # Alignment table (times relative to the extracted clip).
    with open(re.sub(r"\.nml$", "", out) + ".tsv", "w") as fh:
        for i, (k, r) in enumerate(mor):
            line = f"{k}\t{r}\t{first[i]:.3f}\t{vstart[i]:.3f}\t{last[i]:.3f}"
            fh.write(line + "\n")
            print(line)


if __name__ == "__main__":
    main()
