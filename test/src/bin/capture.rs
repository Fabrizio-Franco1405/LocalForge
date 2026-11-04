use std::time::Duration;

fn main() {
    let config = shared::AudioConfig::default();
    let (tx, rx) = std::sync::mpsc::channel();
    let capture = audio_engine::start(&config, move |event| {
        let _ = tx.send(match event {
            audio_engine::AudioEvent::Level(level) => format!("level {level:.2}"),
            audio_engine::AudioEvent::SpeechStart => "speech start".to_owned(),
            audio_engine::AudioEvent::SpeechEnd(samples) => {
                format!("speech end {} samples", samples.len())
            }
        });
    })
    .expect("start capture");

    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    while std::time::Instant::now() < deadline {
        if let Ok(message) = rx.recv_timeout(Duration::from_millis(200)) {
            println!("{message}");
        }
    }

    drop(capture);
    println!("done");
}
