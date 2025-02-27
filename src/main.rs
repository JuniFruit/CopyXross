mod clipboard;
mod encode;
mod network;
mod utils;

use clipboard::new_clipboard;
use clipboard::Clipboard;
use clipboard::ClipboardData;
use clipboard::ClipboardError;
use encode::compose_message;
use encode::parse_message;
use encode::MessageType;
use encode::PeerData;
use local_ip_address::local_ip;
use network::listen_to_socket;
use network::send_message_to_peer;
use network::send_message_to_socket;
use network::socket as socket_bind;
use network::PROTOCOL_VER;
use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::sync::mpsc::channel;
use std::sync::MutexGuard;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PORT: u16 = 53300;

#[derive(PartialEq, Eq, Debug)]
enum SyncMessage {
    Stop,
}

fn main() {
    println!("Starting...");
    println!("Scanning network...");
    let my_local_ip = local_ip().expect("Could not determine my ip");
    println!("This is my local IP address: {:?}", my_local_ip);
    let (s_sender, s_receiver) = channel::<SyncMessage>();
    let (t_sender, t_receiver) = channel::<SyncMessage>();
    let (c_sender, c_receiver) = channel::<SyncMessage>();
    let connection_map: Arc<Mutex<HashMap<IpAddr, PeerData>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let connection_map_clone = connection_map.clone();
    let cp = Arc::new(Mutex::new(new_clipboard().unwrap()));
    let cp_clone = cp.clone();

    // bind listener
    let broadcast_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)), PORT);
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), PORT);
    let socket = socket_bind(bind).unwrap();
    socket.set_broadcast(true).unwrap();
    // discover peers on the network
    // debug_send(&socket, &cp);

    let t_handler = thread::spawn(move || {
        let handler = TcpListener::bind(SocketAddr::new(my_local_ip, PORT)).unwrap();
        debug_println!("Tcp is bound!");
        let mut buffer = vec![];

        loop {
            if let Ok(msg) = t_receiver.try_recv() {
                if msg == SyncMessage::Stop {
                    break;
                }
            }
            if let Ok(data) = handler.accept() {
                let mut data = data.0;
                println!("Test");

                let read = data.read(&mut buffer);
                if read.is_err() {
                    println!("Failed to read TCP stream: {:?}", read.unwrap_err());
                    continue;
                };
                let message = parse_message(&buffer).unwrap_or(MessageType::NoMessage);

                if let MessageType::Xpst(cp_data) = message {
                    attempt_write_lock(&cp.clone(), |cp_l| {
                        if let Err(err) = cp_l.write(cp_data) {
                            println!("{:?}", err);
                        }
                    });
                }
                buffer.clear();
            }
        }
    });
    let socket_clone = socket.try_clone().expect("Could not close socket!");

    let client_handler = thread::spawn(move || {
        let mut input = String::new();

        loop {
            if let Ok(msg) = c_receiver.try_recv() {
                if msg == SyncMessage::Stop {
                    break;
                }
            }

            input.clear(); // Clear previous input
            println!("Copy from local machine (type 'exit' to quit):");
            let mut keys: Vec<String> = vec![];
            attempt_write_lock(&connection_map_clone, |m| {
                keys = m.keys().map(|ip| ip.to_string()).collect();
            });

            println!("Available peers: {:?}", keys);

            std::io::stdin()
                .read_line(&mut input)
                .expect("Failed to read input");

            let trimmed = input.trim();
            if trimmed.eq_ignore_ascii_case("exit") {
                println!("Goodbye!");
                break;
            }

            let args: Vec<&str> = trimmed.split_whitespace().collect();
            if args.len() != 2 {
                println!("Invalid cmd. Usage: cp <ip_addr>. Ex: cp 192.168.178.1");
                continue;
            }
            let cmd = args[0];
            let ip: Vec<u8> = args[1]
                .split(".")
                .map(|val| val.parse::<u8>().unwrap_or(0))
                .collect();
            if cmd != "cp" {
                println!("cmd. Usage: cp <ip_addr>. Ex: cp 192.168.178.1");
                continue;
            } else {
                let addr =
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])), PORT);

                println!("Sending copy cmd to {:?}", addr);
                let message = compose_message(&MessageType::Xcpy, PROTOCOL_VER).unwrap();
                send_message_to_socket(&socket_clone, addr, &message);
            }
        }
    });

    let s_handler = thread::spawn(move || {
        // getting my peer name
        let my_peer_name = format!("PC_num-{}", 42);
        let my_peer_data = encode::PeerData {
            peer_name: my_peer_name,
        };
        // creating greeting message to send to all peers
        let greeting_message =
            compose_message(&MessageType::Xcon(my_peer_data.clone()), PROTOCOL_VER)
                .map_err(|err| {
                    println!("Failed to compose a message: {:?}", err);
                })
                .unwrap();
        // creating acknowledgment msg to response to all peers
        let ack_msg = compose_message(&MessageType::Xacn(my_peer_data.clone()), PROTOCOL_VER)
            .map_err(|err| {
                println!("Failed to compose an ack message: {:?}.", err);
            })
            .unwrap();

        send_message_to_socket(&socket, broadcast_addr, &greeting_message);
        loop {
            if let Ok(msg) = s_receiver.try_recv() {
                if msg == SyncMessage::Stop {
                    break;
                }
            }
            println!("{:?}", s_receiver.try_recv());

            let res = listen_to_socket(&socket);
            if res.is_some() {
                let (ip_addr, data) = res.unwrap();
                let parsed = parse_message(&data).unwrap_or_else(|err| {
                    println!("Parsing error: {:?}", err);
                    MessageType::NoMessage
                });
                match parsed {
                    encode::MessageType::NoMessage => {
                        println!("Skipping message. Empty message received");
                    }
                    encode::MessageType::Xacn(_data) => {
                        println!("Ack got: {:?}", _data);
                        attempt_write_lock(&connection_map.clone(), |mut m| {
                            m.insert(ip_addr.ip(), _data);
                        });
                    }
                    encode::MessageType::Xcon(_data) => {
                        println!("Connection got: {:?}", _data);
                        attempt_write_lock(&connection_map.clone(), |mut m| {
                            m.insert(ip_addr.ip(), _data);
                        });
                        send_message_to_socket(&socket, ip_addr, &ack_msg);
                    }
                    encode::MessageType::Xcpy => {
                        let mut cp_buffer_res: Option<Result<ClipboardData, ClipboardError>> = None;
                        attempt_write_lock(&cp_clone.clone(), |cp| cp_buffer_res = Some(cp.read()));

                        if let Ok(cp_buffer_res) = cp_buffer_res.unwrap() {
                            let cp_buffer = cp_buffer_res;
                            let msg_type = MessageType::Xpst(cp_buffer);
                            let message = compose_message(&msg_type, PROTOCOL_VER);
                            if let Ok(data) = message {
                                if let Err(err) = send_message_to_peer(&ip_addr, &data) {
                                    println!("Error sending TCP message: {:?}", err);
                                }
                            } else {
                                println!("ENCODE ERR: {:?}", message.unwrap_err());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    if t_handler.is_finished() {
        panic!("Could not spin up Tcp listener!");
    }

    // main listener loop
    loop {
        if client_handler.is_finished() || t_handler.is_finished() || s_handler.is_finished() {
            let _ = s_sender.send(SyncMessage::Stop);
            let _ = c_sender.send(SyncMessage::Stop);
            let _ = t_sender.send(SyncMessage::Stop);
            let c_res = client_handler.join();
            let t_res = t_handler.join();
            let s_res = s_handler.join();
            if c_res.is_err() {
                println!("Program finished with error: {:?}", c_res.unwrap_err());
            } else if t_res.is_err() {
                println!("Program finished with error: {:?}", t_res.unwrap_err())
            } else if s_res.is_err() {
                println!("Program finished with error: {:?}", s_res.unwrap_err())
            } else {
                println!("Program finished successfully")
            }

            return;
        }
    }
}

fn attempt_write_lock<T, F>(p: &Arc<Mutex<T>>, op: F)
where
    F: FnOnce(MutexGuard<T>),
{
    let mut attempts = 0;
    let max_attempts = 5;

    loop {
        match p.lock() {
            Ok(p_l) => {
                op(p_l);

                break; // Success, exit loop
            }
            Err(_) => {
                attempts += 1;
                if attempts >= max_attempts {
                    debug_println!(
                        "Could not acquire lock after {} attempts. Giving up.",
                        max_attempts
                    );
                    break;
                }

                let delay = Duration::from_millis(100 * (2_u64.pow(attempts))); // Exponential backoff
                debug_println!("Data is locked. Retrying in {:?}...", delay);
                thread::sleep(delay);
            }
        }
    }
}
