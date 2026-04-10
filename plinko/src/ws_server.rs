use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use tungstenite::Message as WsMessage;
use tungstenite::accept;

use plinko_shared::protocol::ServerMessage;

use crate::auth::AuthDb;
use crate::engine::PlanEngine;
use crate::server::handle_protocol;

use plinko_shared::data::Storage;

pub fn run_ws_server(
    engine: Arc<Mutex<PlanEngine>>,
    storage: Arc<Mutex<Storage>>,
    auth_db: Arc<AuthDb>,
    port: u16,
) {
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).expect("ws bind failed");
    eprintln!("plinko WebSocket server listening on 0.0.0.0:{port}");
    for stream in listener.incoming() {
        match stream {
            Ok(tcp) => {
                let peer = tcp
                    .peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                let engine = Arc::clone(&engine);
                let storage = Arc::clone(&storage);
                let auth_db = Arc::clone(&auth_db);
                std::thread::spawn(move || {
                    handle_ws_connection(tcp, peer, engine, storage, auth_db);
                });
            }
            Err(e) => eprintln!("[ws] accept error: {e}"),
        }
    }
}

fn handle_ws_connection(
    stream: std::net::TcpStream,
    peer: String,
    engine: Arc<Mutex<PlanEngine>>,
    storage: Arc<Mutex<Storage>>,
    auth_db: Arc<AuthDb>,
) {
    let mut ws = match accept(stream) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[ws] {peer}: handshake error: {e}");
            return;
        }
    };

    let (out_tx, out_rx) = std::sync::mpsc::channel::<ServerMessage>();
    let peer_send = peer.clone();
    let (in_tx, in_rx) = std::sync::mpsc::channel::<String>();

    let out_tx_clone = out_tx.clone();
    let engine_clone = Arc::clone(&engine);
    let storage_clone = Arc::clone(&storage);
    let peer_clone = peer.clone();

    let protocol_thread = std::thread::spawn(move || {
        handle_protocol(
            peer_clone,
            move |msg| {
                out_tx_clone
                    .send(msg.clone())
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e))
            },
            move |buf| match in_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(line) => {
                    *buf = line;
                    Ok(true)
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Ok(false),
            },
            engine_clone,
            storage_clone,
            auth_db,
        );
    });

    // Main loop: read WS frames → in_tx, drain out_rx → write WS frames.
    loop {
        // Drain all pending outbound messages first (non-blocking).
        loop {
            match out_rx.try_recv() {
                Ok(msg) => {
                    let json = serde_json::to_string(&msg).unwrap();
                    if ws.send(WsMessage::Text(json.into())).is_err() {
                        eprintln!("[ws] {peer_send}: write error, closing");
                        return;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    return;
                }
            }
        }

        if protocol_thread.is_finished() {
            let _ = ws.close(None);
            return;
        }

        ws.get_mut()
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .ok();
        match ws.read() {
            Ok(WsMessage::Text(text)) => {
                if in_tx.send(text.to_string()).is_err() {
                    return;
                }
            }
            Ok(WsMessage::Close(_)) => {
                eprintln!("[ws] {peer_send}: client closed connection");
                return;
            }
            Ok(WsMessage::Ping(data)) => {
                let _ = ws.send(WsMessage::Pong(data));
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // No data yet — loop back to drain out_rx.
            }
            Err(e) => {
                eprintln!("[ws] {peer_send}: read error: {e}");
                return;
            }
        }
    }
}
