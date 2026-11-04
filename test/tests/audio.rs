//! Tests for the audio resampler and the whisper.cpp output parser.

use audio_engine::LinearResampler;
use stt::parse_output;

#[test]
fn resamples_48k_to_16k_third_length() {
    let mut resampler = LinearResampler::new(48_000, 16_000);
    let input = vec![0.25_f32; 4_800];
    let mut output = Vec::new();

    resampler.process(&input, &mut output);

    assert!(
        (output.len() as i64 - 1_600).abs() <= 3,
        "len={}",
        output.len()
    );
}

#[test]
fn parses_timestamped_whisper_output() {
    let stdout = "\
[00:00:00.000 --> 00:00:02.000]   Hola, ¿qué tal?
[00:00:02.000 --> 00:00:04.000]   Esto es una prueba.
";

    assert_eq!(parse_output(stdout), "Hola, ¿qué tal? Esto es una prueba.");
}

#[test]
fn parses_plain_output_and_drops_markers() {
    let stdout = "\
Hola mundo
[BLANK_AUDIO]
[MUSIC]
";

    assert_eq!(parse_output(stdout), "Hola mundo");
}

#[test]
fn reads_pcm16_wav() {
    let path = std::env::temp_dir().join(format!("localforge-tts-test-{}.wav", std::process::id()));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 24_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).expect("wav");
    for sample in [0_i16, 16_384, -16_384] {
        writer.write_sample(sample).expect("sample");
    }
    writer.finalize().expect("finalize");

    let buffer = tts::read_wav(&path).expect("read");

    assert_eq!(buffer.sample_rate, 24_000);
    assert_eq!(buffer.samples.len(), 3);
    assert!(buffer.samples[1] > 0.4 && buffer.samples[1] < 0.6);

    std::fs::remove_file(&path).ok();
}

#[cfg(unix)]
#[test]
fn command_tts_writes_and_reads_wav() {
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!("localforge-tts-cmd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let script = dir.join("fake_tts.sh");
    std::fs::write(
        &script,
        r#"#!/bin/sh
cat > /dev/null
python3 - "$1" <<'PY'
import sys, wave
w = wave.open(sys.argv[1], 'wb')
w.setnchannels(1); w.setsampwidth(2); w.setframerate(24000)
w.writeframes(b'\x00\x10' * 2400)
w.close()
PY
"#,
    )
    .expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let config = shared::TtsConfig {
        backend: shared::TtsBackend::Command,
        command: format!("{} {{out}}", script.display()),
        voice: "ef_dora".to_owned(),
        speed: 1.0,
        ..shared::TtsConfig::default()
    };

    let audio = tts::synthesize(&config, "Hola **mundo**").expect("tts");

    assert_eq!(audio.sample_rate, 24_000);
    assert_eq!(audio.samples.len(), 2_400);

    std::fs::remove_dir_all(&dir).ok();
}
