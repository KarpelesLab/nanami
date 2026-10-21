# NML — NANAMI Markup Language

NML is how a language model (or a person) tells NANAMI *what* to say and *how*
to say it. It is plain text with optional SSML-like tags, inline IPA, and a
compact note syntax for singing. Everything is forgiving: unknown tags are
ignored with a warning, attribute quotes are optional, and plain text alone is
valid NML.

This page is written so it can be pasted into an LLM system prompt.

---

## 1. Text

```
Hello, world! How are you today?
```

- Punctuation drives phrasing: `,` `;` `:` `—` → continuation (slight rise, short
  pause); `.` → falling statement; `?` → rising yes/no question (falling for
  wh-questions); `!` → emphatic fall; `…` → hesitation pause.
- Numbers, dates, currency and common abbreviations are expanded (English).
- Japanese: write **kana** (ひらがな/カタカナ) or romaji. Kanji cannot be
  pronounced — convert them to kana or IPA yourself.

## 2. Phonemes (IPA)

Anything between slashes is IPA and bypasses spelling-to-sound:

```
My name is /ˈnɑnəmi/. I say /təˈmɑːtoʊ/, not /təˈmeɪtoʊ/.
```

Or `<ipa>…</ipa>`, or SSML `<phoneme ph="…">word</phoneme>`.

Supported symbols:

| Kind | Symbols |
|---|---|
| Vowels | i ɪ e ɛ æ a ɑ ɒ ɔ o ʊ u ʌ ə ɜ ɐ ɯ ɤ ɨ ʉ y ø œ ɚ ɝ |
| Stops | p b t d ʈ ɖ c ɟ k g q ʔ |
| Nasals | m n ɳ ɲ ŋ ɴ |
| Fricatives | ɸ β f v θ ð s z ʃ ʒ ɕ ʑ ʂ ç x ɣ χ ʁ h ɦ |
| Affricates | ts dz tʃ dʒ tɕ dʑ (ligatures ʦ ʧ ʤ ʨ and tie bars OK) |
| Approximants | j w ʍ ɰ ɥ ɹ ɻ l ɫ ʎ ɾ (tap) r (trill) |

| Mark | Meaning |
|---|---|
| `ˈ` or `'` | primary stress (before the syllable) |
| `ˌ` | secondary stress |
| `ː` or `:` | long vowel / geminate consonant |
| `̃` (combining tilde) | nasalized vowel |
| `ʰ` | aspirated |
| `̥` | devoiced |
| `̩` | syllabic consonant |
| `ꜜ` | Japanese accent fall after this vowel (inside `<ja>`, e.g. `<ja>/haꜜɕi/</ja>` 箸) |
| `ꜛ` | step up |
| `↗` `↘` | rising / falling pitch on this syllable |
| `.` | syllable break |

Diphthongs are written as two vowels (`aɪ aʊ ɔɪ eɪ oʊ`).

## 3. Tags

| Tag | Effect |
|---|---|
| `<voice name="nanami">` | Speaker: `nanami`, `female`, `male`, `child` |
| `<emotion name="happy" amount="0.7">` | Emotion; `amount` 0–1 (>1 exaggerates) |
| `<happy>` `<sad 0.5>` … | Shorthand for any emotion name |
| `<prosody rate="1.2" pitch="+2" range="1.5" volume="loud">` | rate multiplier or `slow`/`fast`; pitch in semitones; range multiplier; volume multiplier or `soft`/`loud` |
| `<emph>` | Contrastive emphasis (pitch accent, length, loudness) |
| `<whisper>` | Whispered (no voicing) |
| `<quality breath="0.5" creak="0.3" tension="0.6" tremor="0.2" smile="0.8" precision="1.2">` | Direct voice-quality control |
| `<break time="500ms"/>` | Pause (`ms`, `s`, or `weak`/`medium`/`strong`) |
| `<lang ja>` / `<ja>` / `<en>` | Language for plain text |

Emotions: `neutral`, `happy`, `excited`, `sad`, `angry`, `fear`,
`surprise`, `calm`, `bored`, `disgust`, `serious`, `playful` (plus synonyms
such as joy, scared, gentle, tired…). Tags nest; closing a tag restores the
previous style.

## 4. Singing

```
<sing tempo="96" style="pop" vibrato="0.35">
  twinkle@C4,C4 twinkle@G4,G4 little@A4,A4 star@G4:2
  how@F4 I@F4 won@E4 der@E4 what@D4 you@D4 are@C4:2
</sing>
```

Each token is `lyric@note[:beats][,note[:beats]…]`:

- **lyric**: a word or syllable (spelled), or `/ipa/`. A multi-syllable word
  with several notes is split one syllable per note (`twinkle@C4,C4`). If
  there are more notes than syllables, the last syllable is held across them
  (melisma). `-@D4` continues the previous syllable on a new pitch.
- **note**: `C4`, `F#3`, `Bb5`, MIDI number `60`, or `440hz`.
- **beats**: `1` (default), `2`, `0.5`, `1/2`, `3/4`, or `w h q e s` with `.`
  for dotted. Durations do **not** carry over: omitted = 1 beat.
- **rest**: `_` (1 beat) or `_:2`.
- **legato**: suffix the lyric with `~` to slur into the next note.

`<sing>` attributes: `tempo` (BPM), `transpose` (semitones), `vibrato`
(± semitones), `vibrato-rate` (Hz), `portamento` (glide time constant, s),
`style` (`pop` | `classical` — classical narrows the epilarynx to produce the
singer's formant), `lang`. Emotion and prosody tags around `<sing>` apply.

Japanese singing works the same way with kana: `<sing tempo="80">さ@C4 く@C4 ら@D4:2</sing>`.

## 5. Tips for language models

- Prefer **IPA** for names, rare words, and anything ambiguous — it is exactly
  what will be spoken.
- Use punctuation generously; it is your main intonation tool.
- Keep emotions moderate (`amount` 0.4–0.8) for natural results.
- For songs, give one token per sung syllable; put consonant clusters in the
  syllable they belong to (`/stɑɹ/@G4:2`).
