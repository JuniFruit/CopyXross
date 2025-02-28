mod clipboard;
mod encode;
mod network;
mod utils;

use clipboard::new_clipboard;
use clipboard::Clipboard;
use encode::compose_message;
use encode::parse_message;
use encode::MessageType;
use encode::PeerData;
use local_ip_address::local_ip;
use network::init_listeners;
use network::listen_to_socket;
use network::listen_to_tcp;
use network::send_message_to_peer;
use network::send_message_to_socket;
use network::NetworkError;
use network::PROTOCOL_VER;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::mpsc::channel;
use std::sync::MutexGuard;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PORT: u16 = 53300;

#[derive(PartialEq, Eq, Debug)]
#[allow(dead_code)]
enum SyncMessage {
    Stop,
}

fn main() {
    println!("Starting...");
    println!("Scanning network...");
    let my_local_ip = local_ip().expect("Could not determine my ip");
    println!("This is my local IP address: {:?}", my_local_ip);
    let (_c_sender, _c_receiver) = channel::<SyncMessage>();
    let connection_map: Arc<Mutex<HashMap<IpAddr, PeerData>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let connection_map_clone = connection_map.clone();
    let cp = new_clipboard().unwrap();

    // getting my peer name
    let my_peer_name = format!("PC_num-{}", 42);
    let my_peer_data = encode::PeerData {
        peer_name: my_peer_name,
    };
    // creating greeting message to send to all peers
    let greeting_message = compose_message(&MessageType::Xcon(my_peer_data.clone()), PROTOCOL_VER)
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

    // bind listener
    let broadcast_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)), PORT);
    let (socket, tcp_listener) = init_listeners(my_local_ip).unwrap();

    let socket_clone = socket.try_clone().expect("Could not close socket!");

    send_message_to_socket(&socket, broadcast_addr, &greeting_message);

    let client_handler = thread::spawn(move || {
        thread::sleep(Duration::new(2, 0));
        interact(connection_map_clone, &socket_clone);
    });

    let mut tcp_buff = Vec::with_capacity(5024);
    // main listener loop
    loop {
        if client_handler.is_finished() {
            let c_res = client_handler.join();

            if c_res.is_err() {
                println!("Program finished with error: {:?}", c_res.unwrap_err());
            }

            break;
        }
        let res = listen_to_socket(&socket);
        let tcp_res = listen_to_tcp(&tcp_listener, &mut tcp_buff);
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
                    attempt_write_lock(&connection_map, |mut m| {
                        m.insert(ip_addr.ip(), _data);
                    });
                }
                encode::MessageType::Xcon(_data) => {
                    if ip_addr.ip() != my_local_ip {
                        println!("Connection got: {:?}", _data);
                        attempt_write_lock(&connection_map, |mut m| {
                            m.insert(ip_addr.ip(), _data);
                        });
                        send_message_to_socket(&socket, ip_addr, &ack_msg);
                    }
                }
                encode::MessageType::Xdis => {
                    attempt_write_lock(&connection_map, |mut m| {
                        m.remove(&ip_addr.ip());
                    });
                }
                encode::MessageType::Xcpy => {
                    let cp_buffer_res = cp.read();

                    if let Ok(cp_buffer_res) = cp_buffer_res {
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
                    } else {
                        println!("CLIPBOARD ERR: {:?}", cp_buffer_res.unwrap_err());
                    }
                }
                _ => {}
            }
        }
        if tcp_res.is_ok() {
            tcp_buff.clear();
            let _ = tcp_res.unwrap();
            let parsed = parse_message(&tcp_buff).unwrap_or_else(|err| {
                println!("Parsing error: {:?}", err);
                MessageType::NoMessage
            });
            if let MessageType::Xpst(cp_data) = parsed {
                if let Err(err) = cp.write(cp_data) {
                    println!("CLIPBOARD ERR: {:?}", err);
                }
            }
        } else {
            let err = tcp_res.unwrap_err();
            if err != NetworkError::Blocked {
                debug_println!("Read error: {:?}", err);
            }
        }
    }
    tcp_buff.clear();

    let disconnect_msg = compose_message(&MessageType::Xdis, PROTOCOL_VER).unwrap();
    send_message_to_socket(&socket, broadcast_addr, &disconnect_msg);
    println!("Program finished successfully");
}

fn interact(connection_map: Arc<Mutex<HashMap<IpAddr, PeerData>>>, socket: &UdpSocket) {
    let mut input = String::new();
    let usage_str = "Usage: cp <ip_addr>. Ex: cp 192.168.178.1";
    let delim = "-".repeat(50);

    println!("{}", delim);
    println!("Copy from local machine (type 'exit' to quit):");
    println!("{}", usage_str);
    println!("Any input to update peers");
    println!("{}", delim);

    loop {
        input.clear(); // Clear previous input
        let mut keys: Vec<String> = vec![];
        attempt_write_lock(&connection_map, |m| {
            keys = m.keys().map(|ip| ip.to_string()).collect();
        });

        println!("Available peers: {:?}\r", keys);

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
            continue;
        }
        let cmd = args[0];
        let ip: Vec<u8> = args[1]
            .split(".")
            .map(|val| val.parse::<u8>().unwrap_or(0))
            .collect();
        if cmd != "cp" {
            continue;
        } else {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])), PORT);

            println!("Sending copy cmd to {:?}", addr);
            let message = compose_message(&MessageType::Xcpy, PROTOCOL_VER).unwrap();
            send_message_to_socket(socket, addr, &message);
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
