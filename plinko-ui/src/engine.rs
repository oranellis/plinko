//! Network engine — client-side replacement for PlanEngine.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;

use plinko_shared::data::Plan;
use plinko_shared::protocol::{ClientMessage, PlanRequest, PlanResponse, ServerMessage, VERSION};

#[derive(Clone)]
pub struct PlanRequestSender {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    next_id: Arc<Mutex<u64>>,
}

impl PlanRequestSender {
    pub fn send(&self, request: PlanRequest) {
        let mut id_lock = self.next_id.lock().unwrap();
        let id = *id_lock;
        *id_lock += 1;
        drop(id_lock);

        let msg = ClientMessage::Request { id, request };
        let mut line = serde_json::to_string(&msg).unwrap();
        line.push('\n');
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(line.as_bytes());
        }
    }
}

pub struct NetworkEngine {
    plan: Plan,
    sender: PlanRequestSender,
    incoming: Arc<Mutex<VecDeque<ServerMessage>>>,
}

impl NetworkEngine {
    pub fn connect(port: u16) -> Result<Self, String> {
        let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .map_err(|e| format!("connect failed: {e}"))?;

        let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        let writer = stream;

        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let server_hello: ServerMessage =
            serde_json::from_str(line.trim()).map_err(|e| format!("parse server hello: {e}"))?;
        let ServerMessage::Hello {
            version: server_version,
        } = server_hello
        else {
            return Err("expected Hello from server".to_string());
        };
        if server_version != VERSION {
            return Err(format!(
                "version mismatch: server={server_version}, client={VERSION}"
            ));
        }

        let client_hello = ClientMessage::Hello {
            version: VERSION.to_string(),
        };
        let mut hello_line = serde_json::to_string(&client_hello).unwrap();
        hello_line.push('\n');
        let mut writer_clone = writer.try_clone().map_err(|e| e.to_string())?;
        writer_clone
            .write_all(hello_line.as_bytes())
            .map_err(|e| e.to_string())?;

        line.clear();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let plan_state: ServerMessage =
            serde_json::from_str(line.trim()).map_err(|e| format!("parse plan state: {e}"))?;
        let ServerMessage::PlanState { plan } = plan_state else {
            return Err("expected PlanState from server".to_string());
        };
        let plan = *plan;

        let incoming = Arc::new(Mutex::new(VecDeque::new()));
        let incoming_clone = Arc::clone(&incoming);
        thread::spawn(move || {
            let mut lines = reader;
            let mut line = String::new();
            loop {
                line.clear();
                match lines.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if let Ok(msg) = serde_json::from_str::<ServerMessage>(line.trim()) {
                    incoming_clone.lock().unwrap().push_back(msg);
                }
            }
        });

        let writer_arc: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(writer)));
        let sender = PlanRequestSender {
            writer: writer_arc,
            next_id: Arc::new(Mutex::new(0)),
        };

        Ok(Self {
            plan,
            sender,
            incoming,
        })
    }

    pub fn sender(&self) -> PlanRequestSender {
        self.sender.clone()
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn process_pending(&mut self) -> Vec<PlanResponse> {
        let mut responses = Vec::new();
        let msgs: Vec<ServerMessage> = {
            let mut q = self.incoming.lock().unwrap();
            q.drain(..).collect()
        };
        for msg in msgs {
            match msg {
                ServerMessage::PlanState { plan } => self.plan = *plan,
                ServerMessage::Response { response, .. } => responses.push(response),
                _ => {}
            }
        }
        responses
    }
}
