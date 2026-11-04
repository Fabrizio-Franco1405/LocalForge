//! Microphone capture, voice activity detection and endpointing.
//!
//! cpal captures at the device rate/format. The callback only downmixes to mono
//! and resamples to 16 kHz, then hands chunks to a worker thread; all VAD and
//! endpointing happen there, so the realtime callback never blocks or locks.
//! The worker emits [`AudioEvent`]s with level meters, speech start and the
//! finished utterance (16 kHz mono f32).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use shared::AudioConfig;
use webrtc_vad::{Vad, VadMode};

const TARGET_RATE: u32 = 16_000;
const FRAME_SAMPLES: usize = 320; // 20 ms at 16 kHz
const PRE_ROLL_FRAMES: usize = 15; // 300 ms

/// Events produced by the capture worker.
pub enum AudioEvent {
    /// Input level `0.0..=1.0`.
    Level(f32),
    /// The user started speaking.
    SpeechStart,
    /// End of utterance, 16 kHz mono f32.
    SpeechEnd(Vec<f32>),
}

/// Input/output devices detected on the host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceList {
    /// Microphone names, in host order.
    pub inputs: Vec<String>,
    /// Speaker names, in host order.
    pub outputs: Vec<String>,
}

/// Enumerates input and output devices without opening a stream.
pub fn scan_devices() -> DeviceList {
    let host = cpal::default_host();
    let inputs = host
        .input_devices()
        .map(|devices| devices.map(|device| device.to_string()).collect())
        .unwrap_or_default();
    let outputs = host
        .output_devices()
        .map(|devices| devices.map(|device| device.to_string()).collect())
        .unwrap_or_default();

    DeviceList { inputs, outputs }
}

/// Errors from capture setup.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// No input device.
    #[error("no hay micrófono disponible")]
    NoInput,
    /// The configured device name was not found.
    #[error("no se encontró el micrófono «{0}»")]
    DeviceNotFound(String),
    /// Unsupported sample format.
    #[error("formato de audio no soportado: {0:?}")]
    UnsupportedFormat(SampleFormat),
    /// cpal error.
    #[error("error de audio: {0}")]
    Cpal(String),
    /// The capture thread could not start.
    #[error("no se pudo iniciar el hilo de audio: {0}")]
    Thread(String),
    /// No output device.
    #[error("no hay altavoz disponible")]
    NoOutput,
}

/// Running capture. Dropping it stops the microphone.
pub struct AudioCapture {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Starts capture. `on_event` runs on a worker thread, never on the realtime
/// callback.
pub fn start(
    config: &AudioConfig,
    on_event: impl FnMut(AudioEvent) + Send + 'static,
) -> Result<AudioCapture, AudioError> {
    let (setup_tx, setup_rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let config = config.clone();

    let join = thread::Builder::new()
        .name("localforge-audio".to_owned())
        .spawn(move || {
            if let Err(error) = run_capture(&config, &stop_thread, on_event, &setup_tx) {
                let _ = setup_tx.send(Err(error));
            }
        })
        .map_err(|error| AudioError::Thread(error.to_string()))?;

    match setup_rx.recv() {
        Ok(Ok(())) => Ok(AudioCapture {
            stop,
            join: Some(join),
        }),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(AudioError::Thread("el hilo de audio terminó".to_owned())),
    }
}

fn run_capture<F>(
    config: &AudioConfig,
    stop: &AtomicBool,
    on_event: F,
    setup_tx: &mpsc::Sender<Result<(), AudioError>>,
) -> Result<(), AudioError>
where
    F: FnMut(AudioEvent) + Send + 'static,
{
    let host = cpal::default_host();
    let device = pick_device(&host, &config.input_device)?;
    let supported = device
        .default_input_config()
        .map_err(|error| AudioError::Cpal(error.to_string()))?;
    let sample_format = supported.sample_format();
    let stream_config: cpal::StreamConfig = supported.config();

    let (sample_tx, sample_rx) = mpsc::sync_channel::<Vec<f32>>(64);
    let worker_config = config.clone();
    let worker = thread::Builder::new()
        .name("localforge-vad".to_owned())
        .spawn(move || {
            let mut processor = Processor::new(&worker_config, on_event);
            while let Ok(chunk) = sample_rx.recv() {
                processor.push(&chunk);
            }
        })
        .map_err(|error| AudioError::Thread(error.to_string()))?;

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, sample_tx),
        SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, sample_tx),
        SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, sample_tx),
        other => return Err(AudioError::UnsupportedFormat(other)),
    }?;

    stream
        .play()
        .map_err(|error| AudioError::Cpal(error.to_string()))?;
    let _ = setup_tx.send(Ok(()));

    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
    }

    drop(stream);
    let _ = worker.join();
    Ok(())
}

fn pick_device(host: &cpal::Host, wanted: &str) -> Result<cpal::Device, AudioError> {
    if wanted.trim().is_empty() {
        return host.default_input_device().ok_or(AudioError::NoInput);
    }

    let mut fallback = None;
    if let Ok(devices) = host.input_devices() {
        for device in devices {
            if device.to_string() == wanted {
                return Ok(device);
            }
            if fallback.is_none() {
                fallback = Some(device);
            }
        }
    }

    fallback.ok_or_else(|| AudioError::DeviceNotFound(wanted.to_owned()))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_tx: SyncSender<Vec<f32>>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let mut resampler = LinearResampler::new(config.sample_rate, TARGET_RATE);

    device
        .build_input_stream(
            *config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                for frame in data.chunks_exact(channels.max(1)) {
                    let sum: f32 = frame.iter().map(|sample| f32::from_sample(*sample)).sum();
                    mono.push(sum / channels.max(1) as f32);
                }

                let mut resampled = Vec::with_capacity(mono.len() / 3 + 2);
                resampler.process(&mono, &mut resampled);
                if !resampled.is_empty() {
                    // Never block the audio callback: drop a chunk if the
                    // worker is behind.
                    let _ = sample_tx.try_send(resampled);
                }
            },
            |error| tracing::warn!(%error, "error de captura"),
            None,
        )
        .map_err(|error| AudioError::Cpal(error.to_string()))
}

struct Processor {
    vad: Vad,
    min_speech_ms: u32,
    min_silence_ms: u32,
    max_samples: usize,
    frame: Vec<f32>,
    pre_roll: VecDeque<f32>,
    utterance: Vec<f32>,
    voiced_ms: u32,
    silence_ms: u32,
    in_speech: bool,
    level: f32,
    last_level_at: Instant,
    on_event: Box<dyn FnMut(AudioEvent) + Send>,
}

impl Processor {
    fn new<F>(config: &AudioConfig, on_event: F) -> Self
    where
        F: FnMut(AudioEvent) + Send + 'static,
    {
        let mut vad = Vad::new_with_rate(webrtc_vad::SampleRate::Rate16kHz);
        vad.set_mode(vad_mode(&config.vad.mode));

        Self {
            vad,
            min_speech_ms: config.vad.min_speech_ms.max(60),
            min_silence_ms: config.vad.min_silence_ms.max(100),
            max_samples: (config.vad.max_utterance_ms as usize * TARGET_RATE as usize) / 1000,
            frame: Vec::with_capacity(FRAME_SAMPLES),
            pre_roll: VecDeque::with_capacity(PRE_ROLL_FRAMES * FRAME_SAMPLES),
            utterance: Vec::new(),
            voiced_ms: 0,
            silence_ms: 0,
            in_speech: false,
            level: 0.0,
            last_level_at: Instant::now(),
            on_event: Box::new(on_event),
        }
    }

    fn push(&mut self, samples: &[f32]) {
        let energy = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        self.level = (self.level * 0.7 + energy * 0.3).clamp(0.0, 1.0);
        if self.last_level_at.elapsed() >= Duration::from_millis(50) {
            self.last_level_at = Instant::now();
            (self.on_event)(AudioEvent::Level(self.level));
        }

        self.frame.extend_from_slice(samples);
        while self.frame.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = self.frame.drain(..FRAME_SAMPLES).collect();
            self.process_frame(&frame);
        }
    }

    fn process_frame(&mut self, frame: &[f32]) {
        let pcm: Vec<i16> = frame
            .iter()
            .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();
        let voice = self.vad.is_voice_segment(&pcm).unwrap_or(false);

        self.pre_roll.extend(frame.iter().copied());
        while self.pre_roll.len() > PRE_ROLL_FRAMES * FRAME_SAMPLES {
            self.pre_roll.pop_front();
        }

        if voice {
            self.voiced_ms += 20;
            self.silence_ms = 0;
            if !self.in_speech && self.voiced_ms >= self.min_speech_ms {
                self.in_speech = true;
                self.utterance = self.pre_roll.iter().copied().collect();
                (self.on_event)(AudioEvent::SpeechStart);
            }
            if self.in_speech {
                self.utterance.extend_from_slice(frame);
            }
        } else {
            self.voiced_ms = 0;
            if self.in_speech {
                self.silence_ms += 20;
                self.utterance.extend_from_slice(frame);
                if self.silence_ms >= self.min_silence_ms {
                    self.finish_utterance();
                }
            }
        }

        if self.in_speech && self.utterance.len() >= self.max_samples {
            self.finish_utterance();
        }
    }

    fn finish_utterance(&mut self) {
        let samples = std::mem::take(&mut self.utterance);
        self.in_speech = false;
        self.silence_ms = 0;
        self.voiced_ms = 0;
        self.pre_roll.clear();
        if !samples.is_empty() {
            (self.on_event)(AudioEvent::SpeechEnd(samples));
        }
    }
}

fn vad_mode(mode: &str) -> VadMode {
    match mode {
        "low" => VadMode::LowBitrate,
        "aggressive" => VadMode::Aggressive,
        _ => VadMode::Quality,
    }
}

/// Linear resampler used at the device boundary.
pub struct LinearResampler {
    step: f64,
    pos: f64,
    prev: f32,
    primed: bool,
}

impl LinearResampler {
    /// Creates a resampler from `from` Hz to `to` Hz.
    pub fn new(from: u32, to: u32) -> Self {
        Self {
            step: from as f64 / to as f64,
            pos: 0.0,
            prev: 0.0,
            primed: false,
        }
    }

    /// Resamples `input`, appending to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for &sample in input {
            if !self.primed {
                self.prev = sample;
                self.primed = true;
                continue;
            }
            while self.pos < 1.0 {
                out.push(self.prev + (sample - self.prev) * self.pos as f32);
                self.pos += self.step;
            }
            self.pos -= 1.0;
            self.prev = sample;
        }
    }
}

/// Plays mono `samples` at `sample_rate` on the default output device.
///
/// Blocks until the queue is drained (plus a small tail), so the caller can
/// keep `Speaking(true)` until playback actually ends. Resamples to the device
/// rate and downmixes/duplicates to its channel count.
pub fn play(samples: &[f32], sample_rate: u32, volume: f32) -> Result<(), AudioError> {
    use std::sync::Mutex;

    let host = cpal::default_host();
    let device = host.default_output_device().ok_or(AudioError::NoOutput)?;
    let supported = device
        .default_output_config()
        .map_err(|error| AudioError::Cpal(error.to_string()))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();
    let channels = config.channels as usize;

    let mut resampler = LinearResampler::new(sample_rate, config.sample_rate);
    let mut converted = Vec::with_capacity(samples.len());
    resampler.process(samples, &mut converted);

    let queue = Arc::new(Mutex::new(VecDeque::<f32>::from(converted)));
    let volume = volume.clamp(0.0, 1.0);

    let stream = match format {
        SampleFormat::F32 => {
            build_output::<f32>(&device, &config, Arc::clone(&queue), volume, channels)
        }
        SampleFormat::I16 => {
            build_output::<i16>(&device, &config, Arc::clone(&queue), volume, channels)
        }
        SampleFormat::U16 => {
            build_output::<u16>(&device, &config, Arc::clone(&queue), volume, channels)
        }
        other => return Err(AudioError::UnsupportedFormat(other)),
    }?;

    stream
        .play()
        .map_err(|error| AudioError::Cpal(error.to_string()))?;

    loop {
        let empty = queue.lock().map(|queue| queue.is_empty()).unwrap_or(true);
        if empty {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    thread::sleep(Duration::from_millis(150));
    drop(stream);
    Ok(())
}

fn build_output<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<std::sync::Mutex<VecDeque<f32>>>,
    volume: f32,
    channels: usize,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    device
        .build_output_stream(
            *config,
            move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
                let mut queue = queue.lock().expect("audio queue");
                for frame in out.chunks_mut(channels) {
                    let sample = queue.pop_front().unwrap_or(0.0) * volume;
                    let value = T::from_sample(sample);
                    for slot in frame {
                        *slot = value;
                    }
                }
            },
            |error| tracing::warn!(%error, "error de reproducción"),
            None,
        )
        .map_err(|error| AudioError::Cpal(error.to_string()))
}
