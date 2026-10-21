//! `nanami` command-line interface.

use nanami::ipa::Lang;
use nanami::voice::Voice;
use std::io::Read;
use std::path::PathBuf;

const USAGE: &str = "\
NANAMI — articulatory speech & singing synthesizer

USAGE:
    nanami say <text|->       [options]   Speak text or NML markup (- = stdin)
    nanami render <file.nml>  [options]   Render an NML file
    nanami phonemes <text>    [--lang]    Show the phonetic transcription
    nanami midi <file.mid>    [options]   Sing a MIDI melody (lyric events or --lyrics)
    nanami vowel <ipa>        [options]   Sustain a single vowel (diagnostics)
    nanami voices | emotions              List presets

OPTIONS:
    -o, --out <file>          Output .wav or .flac (default: nanami.wav)
    -v, --voice <name>        nanami | female | male | child
    -l, --lang <en|ja>        Default language for plain text (default: en)
    -p, --play                Play through the sound card
    -s, --spectrogram <png>   Also write a spectrogram image
        --f0 <hz>             Pitch for `vowel`
        --track <n>           MIDI track to sing (default: first with notes)
        --lyrics <text>       Syllables for MIDI notes (- = hold previous)
        --transpose <st>      Transpose MIDI by semitones
        --nml                 Print the generated NML instead of rendering
";

struct Opts {
    out: Option<PathBuf>,
    voice: Voice,
    lang: Lang,
    play: bool,
    spectrogram: Option<PathBuf>,
    f0: Option<f64>,
    track: Option<usize>,
    lyrics: Option<String>,
    transpose: f64,
    print_nml: bool,
    positional: Vec<String>,
}

fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts { out: None, voice: Voice::nanami(), lang: Lang::English, play: false, spectrogram: None, f0: None, track: None, lyrics: None, transpose: 0.0, print_nml: false, positional: Vec::new() };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let mut val = || -> Result<String, String> {
            i += 1;
            args.get(i).cloned().ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "-o" | "--out" => o.out = Some(PathBuf::from(val()?)),
            "-v" | "--voice" => {
                let n = val()?;
                o.voice = Voice::by_name(&n).ok_or_else(|| format!("unknown voice '{n}'"))?;
            }
            "-l" | "--lang" => {
                o.lang = match val()?.as_str() {
                    "ja" | "jp" | "japanese" => Lang::Japanese,
                    "ipa" => Lang::Generic,
                    _ => Lang::English,
                }
            }
            "-p" | "--play" => o.play = true,
            "-s" | "--spectrogram" => o.spectrogram = Some(PathBuf::from(val()?)),
            "--f0" => o.f0 = Some(val()?.parse().map_err(|_| "bad --f0")?),
            "--track" => o.track = Some(val()?.parse().map_err(|_| "bad --track")?),
            "--lyrics" => o.lyrics = Some(val()?),
            "--transpose" => o.transpose = val()?.parse().map_err(|_| "bad --transpose")?,
            "--nml" => o.print_nml = true,
            _ => o.positional.push(a.clone()),
        }
        i += 1;
    }
    Ok(o)
}

fn output(o: &Opts, audio: &[f32], rate: u32) -> Result<(), Box<dyn std::error::Error>> {
    if o.out.is_some() || !o.play {
        let path = o.out.clone().unwrap_or_else(|| PathBuf::from("nanami.wav"));
        nanami::audio_io::write_audio(&path, audio, rate)?;
        eprintln!("wrote {} ({:.2} s)", path.display(), audio.len() as f64 / rate as f64);
    }
    if let Some(png) = &o.spectrogram {
        nanami::audio_io::write_spectrogram(png, audio, rate, 8000.0)?;
        eprintln!("wrote {}", png.display());
    }
    if o.play {
        nanami::audio_io::play(audio, rate)?;
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print!("{USAGE}");
        return;
    }
    let cmd = args[0].clone();
    let o = match parse_args(&args[1..]) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(e) = run(&cmd, &o) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cmd: &str, o: &Opts) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        "say" | "speak" | "render" => {
            let src = match (cmd, o.positional.first().map(|s| s.as_str())) {
                (_, Some("-")) | (_, None) => {
                    let mut s = String::new();
                    std::io::stdin().read_to_string(&mut s)?;
                    s
                }
                ("render", Some(path)) => std::fs::read_to_string(path)?,
                (_, Some(_)) => o.positional.join(" "),
            };
            let r = nanami::render_markup(&src, &o.voice, o.lang);
            for w in &r.warnings {
                eprintln!("warning: {w}");
            }
            output(o, &r.audio, r.sample_rate)
        }
        "phonemes" | "ipa" => {
            let text = o.positional.join(" ");
            let doc = nanami::markup::parse(&text, &o.voice, o.lang);
            for (_, blocks) in &doc.runs {
                for b in blocks {
                    match b {
                        nanami::utterance::Block::Speech(p) => {
                            let words: Vec<String> = p.words.iter().map(|w| nanami::ipa::to_string(&w.segs)).collect();
                            println!("{} {:?}", words.join(" "), p.boundary);
                        }
                        nanami::utterance::Block::Pause(d) => println!("<pause {d:.2}s>"),
                        nanami::utterance::Block::Song(s) => {
                            for it in &s.items {
                                match it {
                                    nanami::utterance::SungItem::Note(n) => println!("♪ {} {:?}", nanami::ipa::to_string(&n.segs), n.pitches),
                                    nanami::utterance::SungItem::Rest(b) => println!("♪ rest {b}"),
                                }
                            }
                        }
                    }
                }
            }
            for w in &doc.warnings {
                eprintln!("warning: {w}");
            }
            Ok(())
        }
        "midi" | "sing" => {
            let path = o.positional.first().ok_or("midi needs a file")?;
            let bytes = std::fs::read(path)?;
            let opts = nanami::midi::MidiOptions { track: o.track, lyrics: o.lyrics.clone(), fallback: "la".into(), transpose: o.transpose };
            let nml = nanami::midi::midi_to_nml(&bytes, &opts)?;
            if o.print_nml {
                println!("{nml}");
                return Ok(());
            }
            let r = nanami::render_markup(&nml, &o.voice, o.lang);
            for w in &r.warnings {
                eprintln!("warning: {w}");
            }
            output(o, &r.audio, r.sample_rate)
        }
        "vowel" => {
            let ipa = o.positional.first().map(|s| s.as_str()).unwrap_or("a");
            let audio = sustained_vowel(&o.voice, ipa, o.f0.unwrap_or(o.voice.f0))?;
            output(o, &audio, nanami::synth::OUTPUT_RATE)
        }
        "voices" => {
            for n in Voice::names() {
                let v = Voice::by_name(n).unwrap();
                println!("{n:8} tract {:.1} cm, f0 {:.0} Hz", v.anatomy.length_cm, v.f0);
            }
            Ok(())
        }
        "emotions" => {
            println!("{}", nanami::utterance::emotion_names().join("\n"));
            Ok(())
        }
        other => Err(format!("unknown command '{other}'\n\n{USAGE}").into()),
    }
}

fn sustained_vowel(voice: &Voice, ipa: &str, f0: f64) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    use nanami::articulation::Articulators;
    use nanami::synth::{CONTROL_RATE, ControlFrame};
    let shape = nanami::phoneme::VowelShape::lookup(ipa).ok_or_else(|| format!("'{ipa}' is not a vowel in the inventory"))?;
    let mut art = Articulators { glottis: 0.0, ..Default::default() };
    shape.apply(&mut art);
    let n = (CONTROL_RATE * 1.2) as usize;
    let frames: Vec<ControlFrame> = (0..n)
        .map(|i| {
            let t = i as f64 / CONTROL_RATE;
            art.lung = voice.lung * (t / 0.06).min(1.0).min((1.2 - t) / 0.08).max(0.0);
            ControlFrame { art: art.clone(), f0: f0 * (1.0 - 0.03 * t), rd: voice.rd, breath: 0.0, creak: 0.0, perturbation: 1.0 }
        })
        .collect();
    Ok(nanami::synth::render(voice, &frames, 7, 0.89))
}
