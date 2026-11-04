# Audio I/O with cpal 0.18

## Breaking changes vs 0.15

- `StreamTrait::play()` → `start()` (`play` deprecated).
- `InputCallbackInfo`/`OutputCallbackInfo` → `CallbackInfo`.
- `DeviceTrait`/`StreamTrait` require `Send + Sync`.
- Buffer over/underruns via `CallbackInfo::xrun()`.
- ALSA uses the `alsa` 0.12 crate.

## Enumerate devices

```rust
use cpal::traits::{DeviceTrait, HostTrait};

let host = cpal::default_host();
for device in host.input_devices()? {
    if let Ok(name) = device.name() {
        println!("in: {name}");
    }
}
for device in host.output_devices()? {
    if let Ok(name) = device.name() {
        println!("out: {name}");
    }
}
```

Device names are not stable across replugs. Persist the name but fall back to
the default device if it disappears.

## Capture (f32, device-native rate)

```rust
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

let device = host.default_input_device().ok_or(AudioError::NoInput)?;
let supported = device.default_input_config()?;
let sample_format = supported.sample_format();
let config: StreamConfig = supported.config();

let stream = match sample_format {
    SampleFormat::F32 => device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::CallbackInfo| { /* push */ },
        move |err| eprintln!("input error: {err}"),
        None,
    )?,
    SampleFormat::I16 => device.build_input_stream(
        &config,
        move |data: &[i16], _: &cpal::CallbackInfo| { /* convert */ },
        move |err| eprintln!("input error: {err}"),
        None,
    )?,
    SampleFormat::U16 => /* same with u16 */,
    other => return Err(AudioError::UnsupportedFormat(other)),
};
stream.start()?;
```

Generic conversion:

```rust
use cpal::{FromSample, Sample, SizedSample};

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut on_frame: impl FnMut(Vec<f32>) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::CallbackInfo| {
            let mut mono = Vec::with_capacity(data.len() / channels);
            for frame in data.chunks_exact(channels) {
                let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
                mono.push(sum / channels as f32);
            }
            on_frame(mono);
        },
        |err| eprintln!("input error: {err}"),
        None,
    )
}
```

`Vec` allocation in the callback is acceptable for a first version but not
ideal; prefer a preallocated scratch buffer owned by the closure and send
fixed-size chunks.

## Playback

```rust
let device = host.default_output_device().ok_or(AudioError::NoOutput)?;
let supported = device.default_output_config()?;
let config = supported.config();

let queue = Arc::new(Mutex::new(VecDeque::<f32>::new()));
let q = queue.clone();

let stream = device.build_output_stream(
    &config,
    move |out: &mut [f32], _: &cpal::CallbackInfo| {
        let mut q = q.lock().unwrap();
        for frame in out.chunks_mut(config.channels as usize) {
            let sample = q.pop_front().unwrap_or(0.0);
            for s in frame { *s = sample; }
        }
    },
    |err| eprintln!("output error: {err}"),
    None,
)?;
stream.start()?;
```

For i16/u16 output, use `T: SizedSample + FromSample<f32>` and
`T::from_sample(sample)`.

The mutex in the audio callback is a pragmatic v1 choice; a lock-free ring
buffer (e.g., `rtrb`) is the upgrade if you hear glitches.

## Resampling (linear)

```rust
pub struct LinearResampler {
    step: f64,      // from / to
    pos: f64,
    prev: f32,
    primed: bool,
}

impl LinearResampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self { step: from as f64 / to as f64, pos: 0.0, prev: 0.0, primed: false }
    }

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
```

- 48 kHz → 16 kHz: `step = 3.0`; output length ≈ input/3.
- Linear interpolation is fine for speech VAD/STT; use `rubato` if you need
  high-quality music or anti-aliasing.
- Reset the resampler when the device changes; stale `prev` causes a click.

## Device changes and recovery

cpal reports a stream error when the device disappears. Keep the
`Stream` in a guard; on error, drop it, re-enumerate, and rebuild after a short
delay. Do not rebuild inside the error callback.

## Linux notes

- ALSA is the backend; PipeWire is reached through `pipewire-alsa`/`pulseaudio-alsa`.
- `libasound2-dev` is required to build.
- `default_input_config()` may pick 48 kHz stereo even for a mono headset;
  resampling + downmix handles it.
- Bluetooth devices often need a moment after connection before they expose a
  config; retry once.

## Windows/macOS notes

- WASAPI/CoreAudio handle format conversion internally, but cpal still gives
  you the device format; convert explicitly.
- On macOS, microphone permission is per-app bundle; `cargo run` may not have
  `NSMicrophoneUsageDescription`. Test with a bundle for microphone access.
