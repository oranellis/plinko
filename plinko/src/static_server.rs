use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

pub fn run_static_server(dist_dir: PathBuf, port: u16) {
    let listener = match TcpListener::bind(format!("127.0.0.1:{port}")) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("static server: bind failed on port {port}: {e}");
            return;
        }
    };
    eprintln!("plinko static server listening on http://127.0.0.1:{port}");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let dir = dist_dir.clone();
                std::thread::spawn(move || handle_static(s, &dir));
            }
            Err(e) => eprintln!("static server: accept error: {e}"),
        }
    }
}

fn handle_static(mut stream: TcpStream, dist_dir: &Path) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    // Drain request headers (we only care about the path).
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => {}
        }
    }

    // Parse "GET /path HTTP/1.1"
    let parts: Vec<&str> = request_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return;
    }
    let raw_path = parts[1];
    // Strip query string.
    let path = raw_path.split('?').next().unwrap_or("/");

    // Map to file path.
    let rel = path.trim_start_matches('/');
    let file_path = if rel.is_empty() {
        dist_dir.join("index.html")
    } else {
        dist_dir.join(rel)
    };

    let (status, body, content_type) = if file_path.is_file() {
        match fs::read(&file_path) {
            Ok(bytes) => {
                let ct = content_type_for(&file_path);
                ("200 OK", bytes, ct)
            }
            Err(_) => (
                "500 Internal Server Error",
                b"Internal Server Error".to_vec(),
                "text/plain",
            ),
        }
    } else {
        // SPA fallback: serve index.html for unknown paths so React routing works.
        match fs::read(dist_dir.join("index.html")) {
            Ok(bytes) => ("200 OK", bytes, "text/html; charset=utf-8"),
            Err(_) => ("404 Not Found", b"404 Not Found".to_vec(), "text/plain"),
        }
    };

    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&body);
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}
