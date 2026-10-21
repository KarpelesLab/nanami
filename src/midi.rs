//! Standard MIDI File → song, via oxideav-midi. Takes one monophonic melody
//! line and attaches lyric meta-events (or a fallback syllable) to its notes.

pub struct MidiOptions {
    /// Track index to sing; None = first track that has notes.
    pub track: Option<usize>,
    /// Lyrics overriding the file's (whitespace-separated syllables, `-` = melisma).
    pub lyrics: Option<String>,
    /// Syllable to use when there are no lyrics.
    pub fallback: String,
    pub transpose: f64,
}

/// Convert a MIDI file into NML `<sing>` markup at tempo 60 (1 beat = 1 s),
/// so the result can be inspected, edited, and rendered like any other song.
pub fn midi_to_nml(bytes: &[u8], opts: &MidiOptions) -> Result<String, String> {
    let f = oxideav_midi::smf::parse(bytes).map_err(|e| format!("MIDI parse error: {e}"))?;
    let mut notes = f.notes();
    let track = match opts.track {
        Some(t) => t,
        None => notes.iter().map(|n| n.track).min().ok_or("MIDI file has no notes")?,
    };
    notes.retain(|n| n.track == track && n.channel != 9);
    notes.sort_by_key(|n| (n.start_tick, std::cmp::Reverse(n.key)));
    // Monophonic reduction: keep the top note at each onset, trim overlaps.
    let mut mono: Vec<(f64, f64, u8)> = Vec::new();
    let mut last_start = u64::MAX;
    for n in &notes {
        if n.start_tick == last_start {
            continue;
        }
        last_start = n.start_tick;
        let (t0, t1) = (f.tick_to_seconds(n.start_tick), f.tick_to_seconds(n.end_tick));
        if let Some(prev) = mono.last_mut() {
            prev.1 = prev.1.min(t0);
        }
        mono.push((t0, t1, n.key));
    }
    if mono.is_empty() {
        return Err(format!("track {track} has no notes"));
    }
    // Lyrics: from options, else lyric events nearest each note onset.
    let mut syllables: Vec<String> = match &opts.lyrics {
        Some(l) => l.split_whitespace().map(String::from).collect(),
        None => Vec::new(),
    };
    if syllables.is_empty() {
        let lyr = f.lyrics();
        if !lyr.is_empty() {
            for (t0, _, _) in &mono {
                let hit = lyr.iter().find(|l| (f.tick_to_seconds(l.tick) - t0).abs() < 0.02);
                let s = hit.map(|l| clean_lyric(&l.text_lossy())).unwrap_or_else(|| "-".into());
                syllables.push(if s.is_empty() { "-".into() } else { s });
            }
        }
    }
    let mut out = String::from("<sing tempo=\"60\"");
    if opts.transpose != 0.0 {
        out.push_str(&format!(" transpose=\"{}\"", opts.transpose));
    }
    out.push('>');
    let mut cursor = mono[0].0;
    for (i, (t0, t1, key)) in mono.iter().enumerate() {
        let gap = t0 - cursor;
        if gap > 0.03 {
            out.push_str(&format!(" _:{gap:.3}"));
        }
        let syl = syllables.get(i).cloned().unwrap_or_else(|| opts.fallback.clone());
        out.push_str(&format!(" {syl}@{key}:{:.3}", (t1 - t0).max(0.05)));
        cursor = *t1;
    }
    out.push_str(" </sing>");
    Ok(out)
}

fn clean_lyric(s: &str) -> String {
    s.trim().trim_end_matches(['-', '/', '\\', '\r', '\n']).chars().filter(|c| !c.is_whitespace() && *c != '@').collect()
}
