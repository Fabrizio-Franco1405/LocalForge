//! Minimal OpenAI-compatible HTTP server used by controller tests.
//!
//! It binds an ephemeral port on loopback, answers `/v1/models` with one model
//! and `/v1/chat/completions` with a fixed SSE stream, and records the last
//! chat request body so tests can assert on the payload.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

/// Loopback server that mimics llama-server / Ollama for tests.
pub struct MockServer {
    port: u16,
    stop: std::sync::Arc<AtomicBool>,
    last_body: std::sync::Arc<Mutex<String>>,
    join: Option<JoinHandle<()>>,
}

impl MockServer {
    /// Starts the server on `127.0.0.1:0` and returns it with the chosen port.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("addr").port();
        listener
            .set_nonblocking(true)
            .expect("non blocking listener");

        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let last_body = std::sync::Arc::new(Mutex::new(String::new()));
        let stop_thread = std::sync::Arc::clone(&stop);
        let body_thread = std::sync::Arc::clone(&last_body);

        let join = std::thread::spawn(move || {
            while !stop_thread.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => handle(stream, &body_thread),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            port,
            stop,
            last_body,
            join: Some(join),
        }
    }

    /// Port the server is listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Body of the last `/v1/chat/completions` request.
    pub fn body(&self) -> String {
        self.last_body.lock().expect("body lock").clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Despierta el accept loop para que el hilo pueda salir.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn handle(mut stream: TcpStream, last_body: &std::sync::Arc<Mutex<String>>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");

    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end = loop {
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            Err(_) => return,
        }

        if let Some(position) = find_bytes(&buffer, b"\r\n\r\n") {
            break position + 4;
        }
    };

    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let path = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_owned();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    while buffer.len() < header_end + content_length {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            Err(_) => break,
        }
    }

    let body = String::from_utf8_lossy(&buffer[header_end..]).to_string();
    if path.contains("chat/completions") {
        *last_body.lock().expect("body lock") = body;
    }

    let response = if path.ends_with("/v1/models") || path == "/models" {
        let body = r#"{"object":"list","data":[{"id":"mock-qwen","object":"model"}]}"#;
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    } else if path.contains("chat/completions") {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hola\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" mundo\"}}]}\n\n",
            "data: [DONE]\n\n"
        );
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    } else {
        let body = r#"{"error":"not found"}"#;
        format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    };

    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
