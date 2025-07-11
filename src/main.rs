#![windows_subsystem = "windows"]
mod app;
mod clipboard;
mod encode;
mod network;
mod utils;

use app::init_taskmenu;
use app::ButtonData;
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
use network::init_network_change_listener;
use network::listen_to_socket;
use network::listen_to_tcp;
use network::send_bye_packet;
use network::send_greeting_packet;
use network::send_message_to_peer;
use network::send_message_to_socket;
use network::NetworkChangeListener;
use network::NetworkError;
use network::NetworkListener;
use network::BROADCAST_ADDR;
use network::PORT;
use network::PROTOCOL_VER;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::net::TcpListener;
use std::net::UdpSocket;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use utils::attempt_get_lock;
use utils::get_pc_name;
use utils::log_into_file;

#[derive(PartialEq, Debug)]
#[allow(dead_code)]
enum SyncMessage {
    Stop,
    Discover,
    NetworkChange,
    Cmd((SocketAddr, MessageType)),
}
#[allow(unused_must_use)]
fn main() {
    log_into_file("Starting...");

    let (_c_sender, _c_receiver) = channel::<SyncMessage>();
    let arc_c_sender = Arc::new(Mutex::new(_c_sender));
    let c_sender = arc_c_sender.clone();
    let c_sender_clone = c_sender.clone();
    let network_change_cb = Box::new(move || {
        if let Ok(sender) = attempt_get_lock(&c_sender_clone) {
            let _ = sender.send(SyncMessage::NetworkChange);
        };
    });
    let listener = init_network_change_listener(Some(network_change_cb)).unwrap();

    listener.start_listen().unwrap();

    let app = Arc::new(init_taskmenu().expect("Init error"));
    let app_arc = app.clone();
    let core_thread = thread::spawn(move || core_handle(app_arc, arc_c_sender, _c_receiver));

    app.run().expect("App failed to run");

    if let Ok(sender) = attempt_get_lock(&c_sender) {
        let _ = log_into_file("Terminating the program.");
        let _ = sender.send(SyncMessage::Stop);
    };
    let res = core_thread.join();
    match res {
        Err(err) => {
            let _ = log_into_file(format!("Program finished with error: {:?}", err).as_str());
        }
        Ok(_) => {
            let _ = log_into_file("Program finished successfully");
        }
    }
}

fn core_handle(
    app_menu: Arc<impl TaskMenuOperations>,
    c_sender: Arc<Mutex<Sender<SyncMessage>>>,
    c_receiver: Receiver<SyncMessage>,
) {
    while !NetworkChangeListener::is_en0_connected() {
        let _ = log_into_file(
            "WiFi network cannot be found! Make sure you are connected to wifi router.",
        );
        thread::sleep(Duration::new(2, 0));
        let cmd_msg = c_receiver.try_recv();
        if cmd_msg.is_ok() {
            let msg = cmd_msg.unwrap();
            if msg == SyncMessage::Stop {
                return;
            }
        }
    }

    thread::sleep(Duration::new(2, 0));

    let c_sender_clone = c_sender.clone();

    let copy_event_handler = Box::new(move |e: Event| {
        if e.is_none() {
            return;
        }
        if let Some(ip_str) = &e.unwrap().attrs_str {
            let socket_addr = SocketAddr::from_str(ip_str)
                .unwrap_or(SocketAddr::new(IpAddr::from_str("0.0.0.0").unwrap(), PORT));
            if let Ok(sender) = attempt_get_lock(&c_sender) {
                let _ = sender.send(SyncMessage::Cmd((socket_addr, MessageType::Xcpy)));
            };
        }
    });

    let btn_res = app_menu.add_menu_item(
        ButtonData::from_str_static("Discover"),
        Box::new(move |_| {
            if let Ok(sender) = attempt_get_lock(&c_sender_clone) {
                let _ = sender.send(SyncMessage::Discover);
            };
        }),
    );

    if btn_res.is_err() {
        let _ = app_menu.stop();
        return;
    }

    let mut connection_map: HashMap<IpAddr, PeerData> = HashMap::new();
    let cp = new_clipboard();
    if cp.is_err() {
        let _ = app_menu.stop();
        return;
    }
    let cp = cp.unwrap();

    // getting my peer name
    let my_peer_name = get_pc_name();
    debug_println!("Name: {:?}", my_peer_name);
    let my_peer_data = encode::PeerData {
        peer_name: my_peer_name,
    };

    // bind listener
    let bind_res = bind_network();
    if bind_res.is_err() {
        let _ = log_into_file(format!("{:?}", bind_res.unwrap_err()).as_str());
        let _ = app_menu.stop();
        return;
    }

    let bound_listeners = bind_res.unwrap();
    let mut my_local_ip = bound_listeners.0;
    let mut socket: Option<UdpSocket> = Some(bound_listeners.1);
    let mut tcp_listener: Option<TcpListener> = Some(bound_listeners.2);

    // creating greeting message to send to all peers

    send_greeting_packet(socket.as_ref(), BROADCAST_ADDR, my_peer_data.clone());

    let nw_change_debounce = Duration::new(2, 0);
    let rediscover_timeframe = Duration::new(60 * 5, 0); // rediscover every 5 min
    let sleep_timeout = Duration::new(1, 0);
    let mut last_rediscover = Instant::now();
    let mut tcp_buff: Vec<u8> = Vec::with_capacity(5024);
    let mut udp_buff: [u8; 1024] = [0; 1024];
    let mut last_nw_change_time: Option<Instant> = None;
    // main listener loop
    loop {
        thread::sleep(sleep_timeout);
        if !tcp_buff.is_empty() {
            tcp_buff.clear();
        }
        if udp_buff[0] != 0 {
            udp_buff = [0; 1024];
        }

        // rebind listeners when debounce time elapses for nw change
        if last_nw_change_time.is_some() {
            let time_now = Instant::now();
            let elapsed = time_now.duration_since(last_nw_change_time.unwrap());
            if elapsed > nw_change_debounce {
                let _ = log_into_file("Network change detected, binding listeners...");
                if tcp_listener.is_some() {
                    drop(tcp_listener.unwrap());
                    tcp_listener = None;
                }
                if socket.is_some() {
                    drop(socket.unwrap());
                    socket = None;
                }

                last_nw_change_time = None;
                connection_map.clear();
                let _ = app_menu.remove_all_dyn();
                let bind_res = bind_network();
                if bind_res.is_err() {
                    let _ = log_into_file(format!("{:?}", bind_res.unwrap_err()).as_str());
                    continue;
                }
                let bind_res = bind_res.unwrap();
                my_local_ip = bind_res.0;
                socket = Some(bind_res.1);
                tcp_listener = Some(bind_res.2);
                let _ = log_into_file("Listeners recreated.");
                send_greeting_packet(socket.as_ref(), BROADCAST_ADDR, my_peer_data.clone());
            }
        }
        if Instant::now().duration_since(last_rediscover) > rediscover_timeframe {
            let _ = app_menu.remove_all_dyn();
            connection_map.clear();
            send_greeting_packet(socket.as_ref(), BROADCAST_ADDR, my_peer_data.clone());
            last_rediscover = Instant::now();
        }

        // receive SyncMessages
        let client_res = c_receiver.try_recv();
        // Listen to UDP datagrams
        let res = listen_to_socket(socket.as_ref(), &mut udp_buff);
        // Listen to TCP packets
        let tcp_res = listen_to_tcp(tcp_listener.as_ref(), &mut tcp_buff);
        // Handle message from UDP
        if res.is_some() {
            let (ip_addr, data) = res.unwrap();
            if ip_addr.ip() == my_local_ip || ip_addr.ip().is_loopback() {
                continue;
            }
            let parsed = parse_message(&data).unwrap_or_else(|err| {
                let _ = log_into_file(format!("Parsing error: {:?}", err).as_str());
                MessageType::NoMessage
            });
            match parsed {
                encode::MessageType::NoMessage => {
                    let _ = log_into_file("Skipping message. Empty message received");
                }
                encode::MessageType::Xacn(_data) => {
                    let _ = log_into_file(format!("Ack got: {:?}", _data).as_str());
                    let p_name = _data.peer_name.clone();

                    let ip = ip_addr.ip();
                    if let Entry::Vacant(_) = connection_map.entry(ip) {
                        connection_map.insert(ip, _data);
                        let mut btn_data =
                            ButtonData::from_str_dyn(&format!("cp from {:?}", p_name));
                        btn_data.attrs_str = Some(ip_addr.to_string());
                        let _ = app_menu.add_menu_item(btn_data, copy_event_handler.clone());
                    }
                }
                encode::MessageType::Xcon(_data) => {
                    let _ = log_into_file(format!("Connection got: {:?}", _data).as_str());
                    let p_name = _data.peer_name.clone();
                    // creating acknowledgment msg to response to all peers
                    let ack_msg =
                        compose_message(&MessageType::Xacn(my_peer_data.clone()), PROTOCOL_VER);
                    if let Ok(ack_msg) = ack_msg {
                        send_message_to_socket(socket.as_ref(), ip_addr, &ack_msg);
                    } else {
                        let _ = log_into_file(
                            format!("Failed to compose ack msg: {:?}", ack_msg.unwrap_err())
                                .as_str(),
                        );
                    }
                    let ip = ip_addr.ip();

                    if let Entry::Vacant(_) = connection_map.entry(ip) {
                        connection_map.insert(ip, _data);
                        let mut btn_data =
                            ButtonData::from_str_dyn(&format!("cp from {:?}", p_name));
                        btn_data.attrs_str = Some(ip_addr.to_string());
                        let _ = app_menu.add_menu_item(btn_data, copy_event_handler.clone());
                    }
                }
                encode::MessageType::Xdis => {
                    if let Some(data) = connection_map.remove(&ip_addr.ip()) {
                        let p_name = data.peer_name;
                        let mut btn_data =
                            ButtonData::from_str_dyn(&format!("cp from {:?}", p_name));
                        btn_data.attrs_str = Some(ip_addr.to_string());
                        let _ = app_menu.remove_menu_item(btn_data);
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
                                let _ = log_into_file(
                                    format!("Error sending TCP message: {:?}", err).as_str(),
                                );
                            }
                        } else {
                            let _ = log_into_file(
                                format!("ENCODE ERR: {:?}", message.unwrap_err()).as_str(),
                            );
                        }
                    } else {
                        let _ = log_into_file(
                            format!("CLIPBOARD ERR: {:?}", cp_buffer_res.unwrap_err()).as_str(),
                        );
                    }
                }
                _ => {}
            }
        }
        // Handle msg from TCP (usually data to write into CP)
        if tcp_res.is_ok() {
            let _ = tcp_res.unwrap();
            let parsed = parse_message(&tcp_buff).unwrap_or_else(|err| {
                let _ = log_into_file(format!("Parsing error: {:?}", err).as_str());
                MessageType::NoMessage
            });

            if let MessageType::Xpst(cp_data) = parsed {
                if let Err(err) = cp.write(cp_data) {
                    let _ = log_into_file(format!("CLIPBOARD ERR: {:?}", err).as_str());
                }
            }
            tcp_buff.clear();
            tcp_buff = Vec::with_capacity(5024);
        } else {
            let err = tcp_res.unwrap_err();
            if err != NetworkError::Blocked {
                debug_println!("Tcp read err: {:?}", err);
            }
        }

        // Handle SyncMessages from other parts of the app
        if client_res.is_ok() {
            let msg = client_res.unwrap();
            #[allow(clippy::collapsible_match)]
            match msg {
                SyncMessage::Cmd((target, msg_cmd)) => {
                    if let MessageType::Xcpy = msg_cmd {
                        let cpy_cmd = compose_message(&MessageType::Xcpy, PROTOCOL_VER);
                        if let Ok(data) = cpy_cmd {
                            send_message_to_socket(socket.as_ref(), target, &data);
                        } else {
                            let _ = log_into_file(
                                format!("Failed to compose message: {:?}", cpy_cmd.unwrap_err())
                                    .as_str(),
                            );
                        }
                    }
                }
                SyncMessage::Stop => {
                    break;
                }
                SyncMessage::Discover => {
                    let _ = app_menu.remove_all_dyn();
                    connection_map.clear();
                    send_greeting_packet(socket.as_ref(), BROADCAST_ADDR, my_peer_data.clone());
                    last_rediscover = Instant::now();
                }
                SyncMessage::NetworkChange => {
                    last_nw_change_time = Some(Instant::now());
                }
            };
        }
    }
    let _ = app_menu.remove_menu_item(ButtonData::from_str_static("Discover"));
    send_bye_packet(socket.as_ref(), BROADCAST_ADDR);
}

fn bind_network() -> Result<(IpAddr, UdpSocket, TcpListener), NetworkError> {
    let _ = log_into_file("Binding listeners...");
    let my_local_ip = local_ip()
        .map_err(|err| NetworkError::Unexpected(format!("Could not get ip: {:?}", err)))?;
    let _ = log_into_file(format!("This is my local IP address: {:?}", my_local_ip).as_str());
    // bind listener
    let (socket, tcp) = init_listeners("0.0.0.0".parse().unwrap())?;
    Ok((my_local_ip, socket, tcp))
}
