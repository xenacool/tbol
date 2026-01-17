use godot::classes::enet_connection::CompressionMode;
use godot::classes::object::ConnectFlags;
use godot::classes::{
    Button, Engine, IPanel, Label, LineEdit, LinkButton, Os, Panel, ProjectSettings,
};
use godot::global::Error;
use godot::prelude::*;
use log::warn;
use std::sync::{Arc, Mutex};
use std::{future::Future, rc::Rc};
use tokio::io::{AsyncBufReadExt, BufReader, stdin};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::{
    runtime::{self, Runtime},
    task::JoinHandle,
};
use veilnet::datagram::Dialer;
use veilnet::{Connection, DHTAddr};
use veilnet::{connection::Veilid, datagram::socket::Socket};

const DEFAULT_PORT: i32 = 8910;

// adapted from MIT licensed https://github.com/2-3-5-41/godot_tokio/tree/master
#[derive(GodotClass)]
#[class(base=Object)]
pub struct TokioRuntime {
    base: Base<Object>,
    runtime: Rc<Runtime>,
}

#[godot_api]
impl IObject for TokioRuntime {
    fn init(base: Base<Object>) -> Self {
        let runtime = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self {
            base,
            runtime: Rc::new(runtime),
        }
    }
}

#[godot_api]
impl TokioRuntime {
    pub const SINGLETON: &'static str = "TokioRuntime";

    fn singleton() -> Option<Gd<TokioRuntime>> {
        match Engine::singleton().get_singleton(Self::SINGLETON) {
            Some(singleton) => Some(singleton.cast::<Self>()),
            None => {
                panic!("Failed to get singleton");
            }
        }
    }

    pub fn runtime() -> Rc<Runtime> {
        match Self::singleton() {
            Some(singleton) => {
                let bind = singleton.bind();
                Rc::clone(&bind.runtime)
            }
            None => {
                panic!("Failed to get singleton");
            }
        }
    }

    /// A wrapper function for the [`tokio::spawn`] function.
    pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        Self::runtime().spawn(future)
    }

    /// A wrapper function for the [`tokio::block_on`] function.
    pub fn block_on<F>(future: F) -> F::Output
    where
        F: Future,
    {
        Self::runtime().block_on(future)
    }

    /// A wrapper function for the [`tokio::spawn_blocking`] function.
    pub fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        Self::runtime().spawn_blocking(func)
    }
}

#[derive(GodotClass)]
#[class(init, base=Panel)]
pub struct IslandMultiplayerWizard {
    #[export]
    host_button: OnEditor<Gd<Button>>,
    #[export]
    join_button: OnEditor<Gd<Button>>,
    #[export]
    status_ok: OnEditor<Gd<Label>>,
    #[export]
    status_fail: OnEditor<Gd<Label>>,
    #[export]
    port_forward_label: OnEditor<Gd<Label>>,
    #[export]
    find_public_ip_button: OnEditor<Gd<LinkButton>>,
    #[export]
    dht_address: OnEditor<Gd<Label>>,
    peer: Option<String>,
    base: Base<Panel>,
    socket_handle: Option<JoinHandle<()>>,
    tx: Option<Sender<IslandMultiplayerEvent>>,
    rx: Option<Receiver<IslandMultiplayerEvent>>,
}

pub enum IslandMultiplayerEvent {
    Message(String),
    Error(String),
    LogEntry(IslandReplicationLogEntry),
}

pub struct IslandReplicationLogEntry {
    entry: u64,
    value: Vec<u8>,
}

#[godot_api]
impl IPanel for IslandMultiplayerWizard {
    fn ready(&mut self) {
        self.base_mut().set_process(true);
        let (tx, rx) = tokio::sync::mpsc::channel::<IslandMultiplayerEvent>(10_000);
        self.tx = Some(tx);
        self.rx = Some(rx);
        /*
        # Connect all the callbacks related to networking.
        multiplayer.peer_connected.connect(_player_connected)
        multiplayer.peer_disconnected.connect(_player_disconnected)
        multiplayer.connected_to_server.connect(_connected_ok)
        multiplayer.connection_failed.connect(_connected_fail)
        multiplayer.server_disconnected.connect(_server_disconnected)
        */

        // let multiplayer = self.base().get_multiplayer().unwrap();
        let gd_ref = self.to_gd();
        // multiplayer
        //     .signals()
        //     .peer_connected()
        //     .builder()
        //     .connect_other_gd(&gd_ref, |mut this: Gd<Self>, _id: i64| {
        //         godot_print!("Someone connected, start the game!");
        //         let pong = load::<PackedScene>("res://pong.tscn").instantiate_as::<Pong>();
        //         // Connect deferred so we can safely erase it from the callback.
        //         pong.signals()
        //             .game_finished()
        //             .builder()
        //             .flags(ConnectFlags::DEFERRED)
        //             .connect_other_mut(&this, |this| {
        //                 this.end_game("Client disconnected.");
        //             });
        //
        //         this.bind_mut()
        //             .base_mut()
        //             .get_tree()
        //             .get_root()
        //             .unwrap()
        //             .add_child(&pong);
        //         this.hide();
        //     });
        // multiplayer
        //     .signals()
        //     .peer_disconnected()
        //     .builder()
        //     .connect_other_mut(&gd_ref, |this, _id: i64| {
        //         if this.base().get_multiplayer().unwrap().is_server() {
        //             this.end_game("Client disconnected.");
        //         } else {
        //             this.end_game("Server disconnected.");
        //         }
        //     });
        // multiplayer
        //     .signals()
        //     .connection_failed()
        //     .builder()
        //     .connect_other_mut(&gd_ref, |this| {
        //         this.set_status("Couldn't connect.", false);
        //         let mut multiplayer = this.base().get_multiplayer().unwrap();
        //         multiplayer.set_multiplayer_peer(Gd::null_arg()); // Remove peer.
        //         this.host_button.set_disabled(false);
        //         this.join_button.set_disabled(false);
        //     });
        // multiplayer
        //     .signals()
        //     .server_disconnected()
        //     .builder()
        //     .connect_other_mut(&gd_ref, |this| {
        //         this.end_game("Server disconnected.");
        //     });
        //
        self.host_button
            .signals()
            .pressed()
            .builder()
            .connect_other_mut(&gd_ref, |this| {
                this.on_host_pressed();
            });
        //
        self.join_button
            .signals()
            .pressed()
            .builder()
            .connect_other_mut(&gd_ref, |this| {
                this.on_join_pressed();
            });
    }

    fn process(&mut self, _delta: f64) {
        let event = self.rx.as_mut().unwrap().try_recv();
        if let Ok(message) = event {
            match message {
                IslandMultiplayerEvent::Message(msg) => {
                    warn!("Received message: {}", msg);
                    self.dht_address.set_text(msg.as_str());
                    self.set_status(&msg, true);
                }
                IslandMultiplayerEvent::Error(err) => {
                    warn!("Received error: {}", err);
                    self.set_status(&err, false);
                    self.host_button.set_disabled(false);
                    self.join_button.set_disabled(false);
                }
                IslandMultiplayerEvent::LogEntry(_entry) => {
                    warn!("Received log entry");
                }
            }
        }
    }
}

#[godot_api]
impl IslandMultiplayerWizard {
    fn set_status(&mut self, text: &str, is_ok: bool) {
        // Simple way to show status.
        if is_ok {
            self.status_ok.set_text(text);
            self.status_fail.set_text("");
        } else {
            self.status_ok.set_text("");
            self.status_fail.set_text(text);
        }
    }

    fn end_game(&mut self, with_error: &str) {
        if self.base().has_node("/root/Pong") {
            // Erase immediately, otherwise network might show
            // errors (this is why we connected deferred above).
            self.base().get_node_as::<Node>("/root/Pong").free();
            self.base_mut().show();
        }

        self.host_button.set_disabled(false);
        self.join_button.set_disabled(false);

        self.set_status(with_error, false);
    }

    fn on_host_pressed(&mut self) {
        let tx = self.tx.clone().unwrap();
        let socket_handle = TokioRuntime::spawn(async move {
            let mut conn = match Veilid::new().await {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx
                        .send(IslandMultiplayerEvent::Error(format!(
                            "Veilid init failed: {}",
                            e
                        )))
                        .await;
                    return;
                }
            };
            if let Err(e) = conn.require_attachment().await {
                let _ = tx
                    .send(IslandMultiplayerEvent::Error(format!(
                        "Veilid attachment failed: {}",
                        e
                    )))
                    .await;
                return;
            }
            let mut sock = match Socket::new(conn, None, 0).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx
                        .send(IslandMultiplayerEvent::Error(format!(
                            "Socket bind failed: {}",
                            e
                        )))
                        .await;
                    return;
                }
            };

            let message = format!("{}", sock.addr());
            let _ = tx.send(IslandMultiplayerEvent::Message(message)).await;

            loop {
                match sock.recv_from().await {
                    Ok((addr, dgram)) => {
                        warn!(
                            "{} {}",
                            addr,
                            str::from_utf8(dgram.as_slice()).unwrap_or("???")
                        );
                    }
                    Err(err) => {
                        warn!("error {}", err);
                    }
                }
            }
        });
        self.socket_handle = Some(socket_handle);

        self.host_button.set_disabled(true);
        self.join_button.set_disabled(true);
        self.set_status("Establishing connection...", true);
    }

    fn on_join_pressed(&mut self) {
        let addr_str = "127.0.0.1".to_string();
        let addr: DHTAddr = match addr_str.parse() {
            Ok(a) => a,
            Err(_) => {
                self.set_status("Invalid DHT address.", false);
                return;
            }
        };

        let tx = self.tx.clone().unwrap();
        let socket_handle = TokioRuntime::spawn(async move {
            let mut conn = match Veilid::new().await {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx
                        .send(IslandMultiplayerEvent::Error(format!(
                            "Veilid init failed: {}",
                            e
                        )))
                        .await;
                    return;
                }
            };
            if let Err(e) = conn.require_attachment().await {
                let _ = tx
                    .send(IslandMultiplayerEvent::Error(format!(
                        "Veilid attachment failed: {}",
                        e
                    )))
                    .await;
                return;
            }
            // Use port 0 (or any valid subkey) for the client's ephemeral socket
            let mut sock = match Socket::new(conn, None, 0).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx
                        .send(IslandMultiplayerEvent::Error(format!(
                            "Socket init failed: {}",
                            e
                        )))
                        .await;
                    return;
                }
            };

            let _ = tx
                .send(IslandMultiplayerEvent::Message(
                    "Connected (sending ping...)".to_string(),
                ))
                .await;

            if let Err(e) = sock.send_to(&addr, b"ping").await {
                let _ = tx
                    .send(IslandMultiplayerEvent::Error(format!("Send failed: {}", e)))
                    .await;
                return;
            }

            loop {
                match sock.recv_from().await {
                    Ok((addr, dgram)) => {
                        warn!(
                            "{} {}",
                            addr,
                            str::from_utf8(dgram.as_slice()).unwrap_or("???")
                        );
                    }
                    Err(err) => {
                        warn!("error {}", err);
                    }
                }
            }
        });
        self.socket_handle = Some(socket_handle);

        self.host_button.set_disabled(true);
        self.join_button.set_disabled(true);
        self.set_status("Connecting...", true);
    }

    fn _on_find_public_ip_pressed(&mut self) {
        let mut os = Os::singleton();
        os.shell_open("https://icanhazip.com/");
    }
}
