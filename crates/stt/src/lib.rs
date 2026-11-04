//! Speech-to-text wrapper for `whisper-cli`.
//!
//! Writes the utterance as a temporary PCM16 mono 16 kHz WAV, runs
//! `whisper-cli -m <model> -f <wav> -l <lang> -t <threads> -np -nt` with a
//! timeout, and parses both timestamped and plain output.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use shared::SttConfig;

/// Result of a transcription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    /// Recognized text (may be empty for silence).
    pub text: String,
}

/// Errors returned by the STT wrapper.
#[derive(Debug, thiserror::Error)]
pub enum SttError {
    /// No `ggml-*.bin` model configured.
    #[error("no hay modelo de Whisper configurado (Ajustes → Motor Audio)")]
    MissingModel,
    /// The binary could not be spawned.
    #[error("no se pudo ejecutar whisper-cli: {0}")]
    Spawn(String),
    /// Writing the temporary WAV failed.
    #[error("no se pudo escribir el WAV temporal: {0}")]
    Wav(String),
    /// The process exceeded the timeout.
    #[error("whisper-cli tardó más de {0} s")]
    Timeout(u64),
    /// Non-zero exit code.
    #[error("whisper-cli terminó con código {code:?}: {stderr}")]
    Exited { code: Option<i32>, stderr: String },
}

/// Blocking wrapper around the configured `whisper-cli` binary.
pub struct WhisperCli {
    config: SttConfig,
}

impl WhisperCli {
    /// Creates the wrapper.
    pub fn new(config: &SttConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Transcribes 16 kHz mono `f32` samples.
    pub fn transcribe(&self, samples: &[f32]) -> Result<Transcript, SttError> {
        let model = self.config.model.as_ref().ok_or(SttError::MissingModel)?;
        let wav = TempWav::write(samples)?;

        let mut command = Command::new(&self.config.binary);
        command
            .arg("-m")
            .arg(model)
            .arg("-f")
            .arg(wav.path())
            .arg("-l")
            .arg(&self.config.language)
            .arg("-t")
            .arg(self.config.threads.to_string())
            .arg("-np")
            .arg("-nt")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if self.config.translate {
            command.arg("-tr");
        }
        command.args(&self.config.extra_args);

        let mut child = command
            .spawn()
            .map_err(|error| SttError::Spawn(error.to_string()))?;
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();

        let stdout_reader = stdout.take().map(|mut pipe| {
            std::thread::spawn(move || {
                let mut text = String::new();
                let _ = pipe.read_to_string(&mut text);
                text
            })
        });
        let stderr_reader = stderr.take().map(|mut pipe| {
            std::thread::spawn(move || {
                let mut text = String::new();
                let _ = pipe.read_to_string(&mut text);
                text
            })
        });

        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(SttError::Timeout(60));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(error) => return Err(SttError::Spawn(error.to_string())),
            }
        };

        let stdout = stdout_reader
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default();
        let stderr = stderr_reader
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default();

        if !status.success() {
            return Err(SttError::Exited {
                code: status.code(),
                stderr: tail(&stderr, 12),
            });
        }

        Ok(Transcript {
            text: parse_output(&stdout),
        })
    }
}

/// Extracts text from `whisper-cli` stdout.
///
/// Accepts timestamped lines (`[00:00:00.000 --> ...] text`), plain lines and
/// drops markers such as `[BLANK_AUDIO]` or `[MUSIC]`.
pub fn parse_output(stdout: &str) -> String {
    let mut parts = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || is_marker(line) {
            continue;
        }

        let text = if line.starts_with('[') {
            line.split_once(']')
                .map(|(_, rest)| rest.trim())
                .unwrap_or(line)
        } else {
            line
        };

        let text = text.trim().trim_start_matches([':', ']']).trim();
        if !text.is_empty() && !is_marker(text) {
            parts.push(text.to_owned());
        }
    }

    parts.join(" ")
}

fn is_marker(text: &str) -> bool {
    let upper = text.to_uppercase();
    upper.contains("[BLANK_AUDIO]")
        || upper.contains("[MUSIC]")
        || upper.contains("[NOISE]")
        || upper.contains("(SILENCIO)")
        || upper.contains("[SILENCE]")
}

fn tail(text: &str, lines: usize) -> String {
    let mut tail: Vec<&str> = text.lines().rev().take(lines).collect();
    tail.reverse();
    tail.join("\n")
}

struct TempWav {
    path: PathBuf,
}

impl TempWav {
    fn write(samples: &[f32]) -> Result<Self, SttError> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "localforge-stt-{}-{unique}.wav",
            std::process::id()
        ));

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec)
            .map_err(|error| SttError::Wav(error.to_string()))?;
        for sample in samples {
            let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer
                .write_sample(value)
                .map_err(|error| SttError::Wav(error.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|error| SttError::Wav(error.to_string()))?;

        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempWav {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
