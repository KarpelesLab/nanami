# NANAMI

**A physically based voice for machines.** NANAMI synthesizes speech and
singing by simulating how a human voice is made — lungs, vocal folds, and a
moving vocal tract — instead of generating audio with a neural network. It is
driven by a small markup language (NML) designed so that language models can
directly write *what* to say (text or IPA phonemes) and *how* (emotion,
prosody, voice quality, melody).

```sh
cargo run --release -- say "Hello! <happy>I'm Nanami.</happy> My name is /ˈnɑnəmi/." -o hello.wav
cargo run --release -- say "<ja>こんにちは、ななみです。</ja>" --play
cargo run --release -- say '<sing tempo="90">twinkle@C4,C4 twinkle@G4,G4 little@A4,A4 star@G4:2</sing>' -o star.flac
cargo run --release -- midi song.mid --lyrics "la la la" -o song.wav
cargo run --release -- phonemes "I can't believe it's 2026!"
```

See [docs/NML.md](docs/NML.md) for the full markup reference (written to be
pasted into an LLM's system prompt).

## How it works

```
NML markup ─▶ G2P ─▶ prosody ─▶ gestural score ─▶ articulators ─▶ aerodynamics ─▶ glottis ─▶ vocal tract ─▶ audio
 (markup.rs)  (g2p/)  (prosody.rs) (score.rs)     (articulation.rs)  (synth.rs)    (glottis.rs)  (tract.rs)
```

| Stage | Model | References |
|---|---|---|
| Text → phonemes | CMU Pronouncing Dictionary + letter-to-sound rules (English); kana/romaji rules with ん assimilation, っ gemination (Japanese) | CMUdict; Elovitz et al. 1976 |
| Timing | Klatt's multiplicative duration rules (English); mora timing (Japanese); note-aligned onsets for singing | Klatt 1979 |
| Intonation | Sparse tonal targets: H\* accents with downstep, L-L% / H-H% / L-H% boundary tones; Japanese lexical pitch accent | Pierrehumbert 1980; Pierrehumbert & Beckman 1988 |
| Emotion | Offsets in F0 level/range, rate, loudness, laryngeal tension, breathiness, creak, jitter, articulatory precision, smile | Murray & Arnott 1993; Burkhardt & Sendlmeier 2000; Scherer 2003 |
| Gestures | Per-articulator tiers (tongue body, lips, tip, dorsum, root, velum, glottis) as critically damped second-order systems; vowels and consonants on separate tiers → coarticulation | Browman & Goldstein 1992; Saltzman & Munhall 1989 |
| Vocal tract shape | Stevens & House parabolic tongue model + asymmetric front channel and pharyngeal expansion; consonant constrictions overlaid. Vowel shapes **calibrated by acoustic-to-articulatory inversion** against measured formants | Stevens & House 1955; Hillenbrand et al. 1995 |
| Airflow | Quasi-steady Bernoulli flow through glottis and supraglottal constriction, oral pressure integrated through wall compliance → stop bursts, devoicing in closures, aspiration emerge physically | Stevens 1998; Maeda 1982; Birkholz 2007 |
| Turbulence | Noise sources ∝ Re² − Re_c² at the constriction and at the teeth obstacle (sibilants); release transients ∝ dU/dt | Flanagan & Cherry 1969; Sondhi & Schroeter 1987; Shadle 1985 |
| Voice source | Liljencrants–Fant glottal flow derivative via Fant's Rd parameter, with jitter, shimmer, flutter, creak, pulsatile aspiration | Fant et al. 1985; Fant 1995; Klatt & Klatt 1990 |
| Acoustics | Kelly–Lochbaum waveguide at 96 kHz (one section = 3.6 mm) with area-dependent losses, nasal branch via 3-port junction at the velum, Flanagan piston radiation load | Kelly & Lochbaum 1962; Flanagan 1972; Story 1995 |
| Singing | Note-timed vowels with consonants placed before the beat, portamento, delayed vibrato, epilarynx narrowing for the singer's formant | Sundberg 1987 |

The only things "learned from data" are the vowel tract shapes, fitted by
`examples/calibrate.rs` (Nelder–Mead over the articulatory model, using the
same waveguide that synthesizes speech). Everything else is physics or
published phonetic rules, so every parameter is interpretable and editable.

I/O uses the [oxideav](https://crates.io/crates/oxideav-core) crates: WAV
(`oxideav-basic`), FLAC (`oxideav-flac`), playback (`oxideav-sysaudio`), MIDI
input (`oxideav-midi`), spectrogram PNGs (`oxideav-png`).

## Voices

`nanami` (default; young, light, slightly breathy, 14.6 cm tract), `female`,
`male` (17.5 cm), `child` (12 cm). A voice is anatomy (tract/nasal length,
epilarynx) plus habitual source settings — see `src/voice.rs`.

## Development

```sh
cargo test                                # unit tests (LF model, IPA, markup, G2P)
cargo run --release --example calibrate > src/vowel_table.rs   # refit vowels
cargo run --release --example diag -- "/ˈasa ˈaʃa ˈapa/"       # per-segment level/centroid/aero trace
cargo run --release --example f0stats    # F0/duration per emotion
cargo run --release -- say "..." -s spec.png   # spectrogram for visual checks
```

Measured checks (nanami voice, `diag`): fricative levels relative to vowels
/s/ −16 dB, /ʃ/ −10 dB, /f/ −30 dB, /θ/ −29 dB, /h/ −22 dB; /s/ centroid ≈ 9
kHz, /ʃ/ ≈ 5 kHz; voiceless stops show burst + aspiration, voiced stops show
voicing decaying during closure. Uniform 17.5 cm tube resonates at 477 / 1433
/ 2394 / 3361 Hz.

## Status & known limitations

This is an early research prototype (v0.1):

- Front vowels (/i e ɛ æ/) reach F2 a few hundred Hz below Hillenbrand means —
  a limit of the current parametric tongue model.
- /l/ is modeled with a 1-D tube (no true lateral side channels); /ɹ/ uses
  three constrictions near F3 velocity maxima.
- English intonation is rule-based (no syntax/focus analysis beyond function
  words and `<emph>`); Japanese defaults to heiban accent unless `ꜜ` is given.
- Kanji are not converted; supply kana or IPA.
- A voice change (`<voice>`) starts a new rendering run (no cross-fade).

## Layout

```
src/
  main.rs          CLI
  lib.rs           render_markup() entry point
  markup.rs        NML parser
  g2p/             English (CMUdict + rules, numbers) and Japanese G2P
  ipa.rs           IPA → segments
  phoneme.rs       inventory, features, articulatory gestures
  utterance.rs     words/phrases/songs, styles, emotion presets
  prosody.rs       durations, intonation, breathing, singing timeline
  score.rs         gestural score → articulator trajectories
  articulation.rs  articulators → area function
  synth.rs         aerodynamics, turbulence, engine
  glottis.rs       LF voice source
  tract.rs         waveguide vocal + nasal tract
  vowel_table.rs   calibrated vowel shapes (generated)
  midi.rs          MIDI → NML
  audio_io.rs      WAV/FLAC/playback/spectrogram via oxideav
data/cmudict.dict  CMU Pronouncing Dictionary (see data/CMUDICT_LICENSE)
```
