//! Audio output through the oxideav crates: WAV / FLAC files, live playback,
//! and spectrogram images (PNG) for visual inspection.

use oxideav_core::{AudioFrame, CodecId, CodecParameters, Error, Frame, Packet, SampleFormat, StreamInfo, TimeBase, WriteSeek};
use std::path::Path;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn stream_info(params: CodecParameters, rate: u32) -> StreamInfo {
    StreamInfo { index: 0, time_base: TimeBase::new(1, rate as i64), duration: None, start_time: Some(0), params }
}

fn to_i16(pcm: &[f32]) -> Vec<i16> {
    pcm.iter().map(|&s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16).collect()
}

/// Write mono 16-bit PCM WAV.
pub fn write_wav(path: &Path, pcm: &[f32], rate: u32) -> Result<()> {
    let mut params = CodecParameters::audio(CodecId::new("pcm_s16le"));
    params.channels = Some(1);
    params.sample_rate = Some(rate);
    params.sample_format = Some(SampleFormat::S16);
    let stream = stream_info(params, rate);
    let out: Box<dyn WriteSeek> = Box::new(std::fs::File::create(path)?);
    let mut mux = oxideav_basic::wav::open_muxer_with(out, std::slice::from_ref(&stream), Default::default())?;
    mux.write_header()?;
    let bytes: Vec<u8> = to_i16(pcm).iter().flat_map(|s| s.to_le_bytes()).collect();
    mux.write_packet(&Packet::new(0, stream.time_base, bytes))?;
    mux.write_trailer()?;
    Ok(())
}

/// Write mono 16-bit FLAC.
pub fn write_flac(path: &Path, pcm: &[f32], rate: u32) -> Result<()> {
    let mut p = CodecParameters::audio(CodecId::new("flac"));
    p.channels = Some(1);
    p.sample_rate = Some(rate);
    p.sample_format = Some(SampleFormat::S16);
    let mut enc = oxideav_flac::encoder::make_encoder(&p)?;
    let samples = to_i16(pcm);
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    enc.send_frame(&Frame::Audio(AudioFrame { samples: samples.len() as u32, pts: Some(0), data: vec![bytes] }))?;
    enc.flush()?;
    let mut packets = Vec::new();
    loop {
        match enc.receive_packet() {
            Ok(pkt) => packets.push(pkt),
            Err(Error::NeedMore) | Err(Error::Eof) => break,
            Err(e) => return Err(e.into()),
        }
    }
    // Native FLAC stream: magic + metadata blocks (extradata) + frames.
    let mut file = b"fLaC".to_vec();
    file.extend_from_slice(&enc.output_params().extradata);
    for pkt in &packets {
        file.extend_from_slice(&pkt.data);
    }
    std::fs::write(path, file)?;
    Ok(())
}

/// Write audio, choosing the format from the file extension (.wav / .flac).
pub fn write_audio(path: &Path, pcm: &[f32], rate: u32) -> Result<()> {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("flac") => write_flac(path, pcm, rate),
        _ => write_wav(path, pcm, rate),
    }
}

/// Play audio on the default sound device and block until done.
pub fn play(pcm: &[f32], rate: u32) -> Result<()> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let data: Arc<Vec<f32>> = Arc::new(pcm.to_vec());
    let pos = Arc::new(AtomicUsize::new(0));
    let (d, p) = (data.clone(), pos.clone());
    let req = oxideav_sysaudio::StreamRequest::new(rate, 1);
    let stream = oxideav_sysaudio::open_default(req, move |out: &mut [f32], _info| {
        let start = p.load(Ordering::Relaxed);
        for (i, s) in out.iter_mut().enumerate() {
            *s = d.get(start + i).copied().unwrap_or(0.0);
        }
        p.store(start + out.len(), Ordering::Relaxed);
    })?;
    let fmt = stream.format();
    if fmt.sample_rate != rate || fmt.channels != 1 {
        eprintln!("note: device opened at {} Hz / {} ch; playback may be off-speed", fmt.sample_rate, fmt.channels);
    }
    while pos.load(Ordering::Relaxed) < data.len() + rate as usize / 5 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    drop(stream);
    Ok(())
}

/// Render a log-magnitude spectrogram (0..max_hz) as PNG.
pub fn write_spectrogram(path: &Path, pcm: &[f32], rate: u32, max_hz: f64) -> Result<()> {
    let nfft = 1024;
    let hop = (rate as usize / 400).max(1); // 2.5 ms columns
    let height = 400usize;
    let cols = (pcm.len().saturating_sub(nfft)) / hop + 1;
    let width = cols.clamp(1, 4000);
    let hop = if cols > 4000 { (pcm.len() - nfft) / 4000 } else { hop };
    let win: Vec<f64> = (0..nfft).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / nfft as f64).cos()).collect();
    let mut mags = vec![0.0f64; width * height];
    let mut vmax = -1e9f64;
    let max_bin = (max_hz / rate as f64 * nfft as f64).min(nfft as f64 / 2.0 - 1.0);
    for c in 0..width {
        let start = c * hop;
        let mut re: Vec<f64> = (0..nfft).map(|i| pcm.get(start + i).copied().unwrap_or(0.0) as f64 * win[i]).collect();
        // Pre-emphasis-like tilt correction makes formants more visible.
        let mut im = vec![0.0; nfft];
        crate::dsp::fft(&mut re, &mut im);
        for r in 0..height {
            let bin = (r as f64 / height as f64 * max_bin) as usize;
            let tilt = 1.0 + bin as f64 * rate as f64 / nfft as f64 / 1000.0;
            let p = 10.0 * ((re[bin].powi(2) + im[bin].powi(2)) * tilt + 1e-20).log10();
            mags[(height - 1 - r) * width + c] = p;
            vmax = vmax.max(p);
        }
    }
    let mut rgb = Vec::with_capacity(width * height * 3);
    for &p in &mags {
        let t = ((p - (vmax - 70.0)) / 70.0).clamp(0.0, 1.0);
        rgb.extend_from_slice(&heat(t));
    }
    let png = oxideav_png::encode_rgb8(width as u32, height as u32, &rgb, &oxideav_png::EncodeOptions::default())?;
    std::fs::write(path, png)?;
    Ok(())
}

fn heat(t: f64) -> [u8; 3] {
    // Black → blue → magenta → orange → white.
    let stops = [(0.0, [0.0, 0.0, 0.0]), (0.3, [0.1, 0.05, 0.45]), (0.55, [0.7, 0.1, 0.5]), (0.8, [1.0, 0.6, 0.1]), (1.0, [1.0, 1.0, 0.9])];
    for w in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (w[0], w[1]);
        if t <= t1 {
            let f = (t - t0) / (t1 - t0);
            return [0, 1, 2].map(|i| ((c0[i] + (c1[i] - c0[i]) * f) * 255.0) as u8);
        }
    }
    [255, 255, 230]
}
