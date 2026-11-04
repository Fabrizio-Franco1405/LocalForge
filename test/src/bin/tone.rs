fn main() {
    let samples: Vec<f32> = (0..24_000)
        .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin() * 0.08)
        .collect();
    audio_engine::play(&samples, 24_000, 0.6).expect("playback");
    println!("playback ok");
}
