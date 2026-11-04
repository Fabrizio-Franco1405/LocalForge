//! Text-to-speech backends.
//!
//! - `KokoroCli`: runs the isolated Python worker (`tools/kokoro_worker.py`)
//!   with `kokoro-onnx` (ONNX Runtime CPU), text on stdin and a WAV path on
//!   stdout/`--out`.
//! - `Command`: any user command template with `{out}`, `{voice}`, `{speed}`;
//!   text also goes on stdin.
//!
//! Both return a 24 kHz mono [`AudioBuffer`]; playback lives in `audio_engine`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use shared::{TtsBackend, TtsConfig};

/// Decoded speech ready for playback.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioBuffer {
    /// Mono samples in `-1.0..=1.0`.
    pub samples: Vec<f32>,
    /// Native sample rate of the backend (Kokoro: 24 kHz).
    pub sample_rate: u32,
}

/// TTS errors.
#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    /// The backend is disabled.
    #[error("el motor TTS está desactivado (Ajustes → Motor Audio)")]
    Disabled,
    /// Nothing to say after cleaning Markdown.
    #[error("el texto quedó vacío tras limpiar Markdown")]
    Empty,
    /// Worker script missing.
    #[error("no se encuentra el worker: {0}")]
    MissingWorker(String),
    /// Command template empty.
    #[error("el comando TTS está vacío")]
    MissingCommand,
    /// Process failed to start.
    #[error("no se pudo ejecutar el TTS: {0}")]
    Spawn(String),
    /// Timed out.
    #[error("el TTS tardó más de {0} s")]
    Timeout(u64),
    /// Non-zero exit.
    #[error("el TTS terminó con código {code:?}: {stderr}")]
    Exited { code: Option<i32>, stderr: String },
    /// WAV decoding failed.
    #[error("no se pudo leer el WAV: {0}")]
    Wav(String),
}

/// Synthesizes `text` with the configured backend.
pub fn synthesize(config: &TtsConfig, text: &str) -> Result<AudioBuffer, TtsError> {
    let clean = shared::text::for_speech(text);
    if clean.trim().is_empty() {
        return Err(TtsError::Empty);
    }

    match config.backend {
        TtsBackend::Disabled => Err(TtsError::Disabled),
        TtsBackend::KokoroCli => kokoro(config, &clean),
        TtsBackend::Command => external(config, &clean),
    }
}

fn kokoro(config: &TtsConfig, text: &str) -> Result<AudioBuffer, TtsError> {
    let script = config.worker_script.clone();
    if script.as_os_str().is_empty() || !script.is_file() {
        return Err(TtsError::MissingWorker(script.display().to_string()));
    }
    if config.python.as_os_str().is_empty() {
        return Err(TtsError::Spawn(
            "falta la ruta del intérprete Python (.venv-kokoro)".to_owned(),
        ));
    }

    let out = temp_wav("kokoro");
    let mut command = Command::new(&config.python);
    command
        .arg(&script)
        .arg("--model-dir")
        .arg(&config.model_dir)
        .arg("--voice")
        .arg(&config.voice)
        .arg("--speed")
        .arg(config.speed.to_string())
        .arg("--out")
        .arg(&out);

    run(command, Some(text), 60)?;
    let buffer = read_wav(&out);
    let _ = std::fs::remove_file(&out);
    buffer
}

fn external(config: &TtsConfig, text: &str) -> Result<AudioBuffer, TtsError> {
    if config.command.trim().is_empty() {
        return Err(TtsError::MissingCommand);
    }

    let out = temp_wav("command");
    let rendered = config
        .command
        .replace("{out}", &out.display().to_string())
        .replace("{voice}", &config.voice)
        .replace("{speed}", &config.speed.to_string());

    let mut parts = rendered.split_whitespace();
    let program = parts.next().ok_or(TtsError::MissingCommand)?;
    let mut command = Command::new(program);
    command.args(parts);

    run(command, Some(text), 60)?;
    let buffer = read_wav(&out);
    let _ = std::fs::remove_file(&out);
    buffer
}

fn run(mut command: Command, stdin_text: Option<&str>, seconds: u64) -> Result<(), TtsError> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| TtsError::Spawn(error.to_string()))?;

    if let Some(text) = stdin_text
        && let Some(mut stdin) = child.stdin.take()
    {
        let _ = stdin.write_all(text.as_bytes());
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = spawn_reader(stdout);
    let stderr_reader = spawn_reader(stderr);

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(TtsError::Timeout(seconds));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(error) => return Err(TtsError::Spawn(error.to_string())),
        }
    };

    let _ = stdout_reader.and_then(|reader| reader.join().ok());
    let stderr = stderr_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();

    if !status.success() {
        return Err(TtsError::Exited {
            code: status.code(),
            stderr: tail(&stderr, 10),
        });
    }

    Ok(())
}

fn spawn_reader(
    pipe: Option<impl Read + Send + 'static>,
) -> Option<std::thread::JoinHandle<String>> {
    pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    })
}

/// Reads a mono/stereo WAV into an [`AudioBuffer`].
pub fn read_wav(path: &Path) -> Result<AudioBuffer, TtsError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|error| TtsError::Wav(error.to_string()))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .filter_map(Result::ok)
            .map(|sample| sample as f32 / i16::MAX as f32)
            .collect(),
        (hound::SampleFormat::Int, 32) => reader
            .samples::<i32>()
            .filter_map(Result::ok)
            .map(|sample| sample as f32 / i32::MAX as f32)
            .collect(),
        (hound::SampleFormat::Float, _) => reader.samples::<f32>().filter_map(Result::ok).collect(),
        _ => {
            return Err(TtsError::Wav(format!(
                "formato no soportado: {:?}/{} bits",
                spec.sample_format, spec.bits_per_sample
            )));
        }
    };

    let channels = spec.channels.max(1) as usize;
    let mono = if channels > 1 {
        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect()
    } else {
        samples
    };

    Ok(AudioBuffer {
        samples: mono,
        sample_rate: spec.sample_rate,
    })
}

fn temp_wav(tag: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "localforge-tts-{tag}-{}-{unique}.wav",
        std::process::id()
    ))
}

fn tail(text: &str, lines: usize) -> String {
    let mut tail: Vec<&str> = text.lines().rev().take(lines).collect();
    tail.reverse();
    tail.join("\n")
}
