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
use network::socket;
use network::PROTOCOL_VER;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PORT: u16 = 53300;

fn main() {
    println!("Starting...");
    println!("Scanning network...");
    let my_local_ip = local_ip().expect("Could not determine my ip");
    println!("This is my local IP address: {:?}", my_local_ip);
    let mut connection_map: HashMap<IpAddr, PeerData> = HashMap::new();
    let cp = Arc::new(Mutex::new(new_clipboard().unwrap()));
    let cp_clone = cp.clone();

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
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), PORT);
    let socket = socket(bind).unwrap();
    socket.set_broadcast(true).unwrap();
    // discover peers on the network
    // debug_send(&socket, &cp);

    let t_handler = thread::spawn(move || {
        let handler = TcpListener::bind(SocketAddr::new(my_local_ip, PORT)).unwrap();
        debug_println!("Tcp is bound!");
        let mut buffer = vec![];

        for data in handler.incoming() {
            match data {
                Ok(mut data) => {
                    let read = data.read(&mut buffer);
                    if read.is_err() {
                        println!("Failed to read TCP stream: {:?}", read.unwrap_err());
                        continue;
                    };
                    let message = parse_message(&buffer).unwrap_or(MessageType::NoMessage);

                    if let MessageType::Xpst(cp_data) = message {
                        attempt_clipboard_write(&cp, cp_data);
                    }
                    buffer.clear();
                }
                Err(err) => println!("Error during reading TCP stream: {:?}", err),
            }
        }
    });
    if t_handler.is_finished() {
        panic!("Could not spin up Tcp listener!")
    }

    send_message_to_socket(&socket, broadcast_addr, &greeting_message);

    // main listener loop
    loop {
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
                    connection_map.insert(ip_addr.ip(), _data);
                }
                encode::MessageType::Xcon(_data) => {
                    println!("Connection got: {:?}", _data);
                    connection_map.insert(ip_addr.ip(), _data);
                    send_message_to_socket(&socket, ip_addr, &ack_msg);
                }
                encode::MessageType::Xcpy => {
                    let cp_buffer_res = attempt_clipboard_read(&cp_clone);

                    if let Ok(cp_buffer_res) = cp_buffer_res {
                        if cp_buffer_res.is_err() {
                            println!("CLIPBOARD ERR: {:?}", cp_buffer_res.unwrap_err());
                        } else {
                            let cp_buffer = cp_buffer_res.unwrap();
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
                }
                _ => {}
            }
        }
    }
}

fn attempt_clipboard_write(clipboard: &Arc<Mutex<impl Clipboard>>, data: ClipboardData) {
    let mut attempts = 0;
    let max_attempts = 5;

    loop {
        match clipboard.lock() {
            Ok(cp) => {
                if let Err(err) = cp.write(data) {
                    println!("Failed to write to clipboard: {:?}", err);
                }

                break; // Success, exit loop
            }
            Err(_) => {
                attempts += 1;
                if attempts >= max_attempts {
                    debug_println!(
                        "Could not acquire clipboard lock after {} attempts. Giving up.",
                        max_attempts
                    );
                    break;
                }

                let delay = Duration::from_millis(100 * (2_u64.pow(attempts))); // Exponential backoff
                debug_println!("⚠️ Clipboard is locked. Retrying in {:?}...", delay);
                thread::sleep(delay);
            }
        }
    }
}

fn attempt_clipboard_read(
    clipboard: &Arc<Mutex<impl Clipboard>>,
) -> Result<Result<ClipboardData, ClipboardError>, ErrorKind> {
    let mut attempts = 0;
    let max_attempts = 5;

    loop {
        match clipboard.lock() {
            Ok(cp) => {
                return Ok(cp.read());
            }
            Err(_) => {
                attempts += 1;
                if attempts >= max_attempts {
                    debug_println!(
                        "Could not acquire clipboard lock after {} attempts. Giving up.",
                        max_attempts
                    );
                    return Err(ErrorKind::Deadlock);
                }

                let delay = Duration::from_millis(100 * (2_u64.pow(attempts))); // Exponential backoff
                debug_println!("⚠️ Clipboard is locked. Retrying in {:?}...", delay);
                thread::sleep(delay);
            }
        }
    }
}
