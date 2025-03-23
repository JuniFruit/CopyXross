mod app;
mod clipboard;
mod encode;
mod network;
mod utils;

use app::init_taskmenu;
use app::Event;
use app::TaskMenuOperations;
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
use network::send_bye_packet;
use network::send_message_to_peer;
use network::send_message_to_socket;
use network::NetworkError;
use network::BROADCAST_ADDR;
use network::PORT;
use network::PROTOCOL_VER;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use utils::attempt_get_lock;
use utils::format_copy_button_title;

#[derive(PartialEq, Debug)]
#[allow(dead_code)]
enum SyncMessage {
    Stop,
    Cmd((SocketAddr, MessageType)),
}

fn main() {
    println!("Starting...");
    let (_c_sender, _c_receiver) = channel::<SyncMessage>();
    let arc_c_sender = Arc::new(Mutex::new(_c_sender));
    let c_sender = arc_c_sender.clone();

    let app = Arc::new(init_taskmenu().unwrap());
    let app_arc = app.clone();
    let core_thread = thread::spawn(move || core_handle(app_arc, arc_c_sender, _c_receiver));

    if core_thread.is_finished() {
        panic!("Program failed to start successfully");
    }

    let res = app.run();

    if res.is_err() {
        println!("Error: {:?}", res.unwrap_err());
    }

    attempt_get_lock(&c_sender, |sender| {
        println!("Terminating the program.");
        let _ = sender.send(SyncMessage::Stop);
    });
    let res = core_thread.join();
    match res {
        Err(err) => {
            println!("Program finished with error: {:?}", err);
        }
        Ok(_) => {
            println!("Program finished successfully");
        }
    }
}

fn core_handle(
    app_menu: Arc<impl TaskMenuOperations>,
    c_sender: Arc<Mutex<Sender<SyncMessage>>>,
    c_receiver: Receiver<SyncMessage>,
) {
    println!("Scanning network...");
    thread::sleep(Duration::new(10, 0));
    let copy_event_handler = Box::new(move |e: Event| {
        if e.is_none() {
            return;
        }
        let ip_str = e.unwrap();
        let socket_addr = SocketAddr::new(
            IpAddr::from_str(&ip_str).unwrap_or(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))),
            PORT,
        );
        attempt_get_lock(&c_sender, |sender| {
            let _ = sender.send(SyncMessage::Cmd((socket_addr, MessageType::Xcpy)));
        });
    });
    let my_local_ip = local_ip().expect("Could not determine my ip");
    println!("This is my local IP address: {:?}", my_local_ip);
    let mut connection_map: HashMap<IpAddr, PeerData> = HashMap::new();
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
    let (socket, tcp_listener) = init_listeners(my_local_ip).unwrap();

    send_message_to_socket(&socket, BROADCAST_ADDR, &greeting_message);

    let mut tcp_buff: Vec<u8> = Vec::with_capacity(5024);
    // main listener loop
    loop {
        if !tcp_buff.is_empty() {
            tcp_buff.clear();
        }
        let client_res = c_receiver.try_recv();
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
                    let p_name = _data.peer_name.clone();

                    connection_map.insert(ip_addr.ip(), _data);
                    let _ = app_menu.add_menu_item(
                        format_copy_button_title(&p_name, &ip_addr.to_string()),
                        copy_event_handler.clone(),
                    );
                }
                encode::MessageType::Xcon(_data) => {
                    if ip_addr.ip() != my_local_ip {
                        println!("Connection got: {:?}", _data);
                        let p_name = _data.peer_name.clone();

                        connection_map.insert(ip_addr.ip(), _data);
                        send_message_to_socket(&socket, ip_addr, &ack_msg);
                        let _ = app_menu.add_menu_item(
                            format_copy_button_title(&p_name, &ip_addr.to_string()),
                            copy_event_handler.clone(),
                        );
                    }
                }
                encode::MessageType::Xdis => {
                    if let Some(data) = connection_map.remove(&ip_addr.ip()) {
                        let _ = app_menu.remove_menu_item(format_copy_button_title(
                            &data.peer_name,
                            &ip_addr.to_string(),
                        ));
                    }
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
            tcp_buff.clear();
            tcp_buff = Vec::with_capacity(5024);
        } else {
            let err = tcp_res.unwrap_err();
            if err != NetworkError::Blocked {
                debug_println!("Read error: {:?}", err);
            }
        }

        if client_res.is_ok() {
            let msg = client_res.unwrap();
            #[allow(clippy::collapsible_match)]
            match msg {
                SyncMessage::Cmd((target, msg_cmd)) => {
                    if let MessageType::Xcpy = msg_cmd {
                        let cpy_cmd = compose_message(&MessageType::Xcpy, PROTOCOL_VER);
                        if let Ok(data) = cpy_cmd {
                            send_message_to_socket(&socket, target, &data);
                        } else {
                            println!("Failed to compose message: {:?}", cpy_cmd.unwrap_err());
                        }
                    }
                }
                SyncMessage::Stop => {
                    break;
                }
            };
        }
    }
    send_bye_packet(&socket, BROADCAST_ADDR);
}
