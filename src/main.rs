mod clipboard;
mod encode;
mod network;
mod utils;

use clipboard::{new_clipboard, Clipboard};
use encode::{compose_message, parse_message, MessageType, PeerData};
use local_ip_address::local_ip;
use network::{
    listen_to_socket, ping_apps_on_network, send_message_to_socket, socket, PROTOCOL_VER,
};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
};
use utils::Rand;

fn main() {
    println!("Starting...");
    println!("Scanning network...");
    let my_local_ip = local_ip().expect("Could not determine my ip");
    println!("This is my local IP address: {:?}", my_local_ip);
    let port = 53300;
    let mut connection_map: HashMap<IpAddr, PeerData> = HashMap::new();
    let mut randomizer = Rand::new(0);
    let rnd = randomizer.rand();
    let cp = new_clipboard().unwrap();

    // getting my peer name
    let my_peer_name = format!("PC_num-{}", rnd);
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
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);
    let socket = socket(bind).unwrap();
    // discover peers on the network
    ping_apps_on_network(&socket, my_local_ip.to_canonical(), &greeting_message, port);
    // debug_send(&socket, &cp);

    // main listener loop
    loop {
        let res = listen_to_socket(&socket);
        process_message(res, &mut connection_map, &ack_msg, &cp, &socket);
    }
}

pub fn process_message(
    res: Option<(SocketAddr, Vec<u8>)>,
    connection_map: &mut HashMap<IpAddr, PeerData>,
    ack_msg: &[u8],
    cp: &impl Clipboard,
    socket: &UdpSocket,
) {
    if res.is_some() {
        let (ip_addr, data) = res.unwrap();
        let parsed = parse_message(data).unwrap_or_else(|err| {
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
                send_message_to_socket(socket, ip_addr, ack_msg);
            }
            encode::MessageType::Xcpy => {
                let cp_buffer_res = cp.read();
                if cp_buffer_res.is_err() {
                    println!("CLIPBOARD ERR: {:?}", cp_buffer_res.unwrap_err());
                } else {
                    let cp_buffer = cp_buffer_res.unwrap();
                    let msg_type = MessageType::Xpst(cp_buffer);
                    let message = compose_message(&msg_type, PROTOCOL_VER);
                    if message.is_err() {
                        println!("ENCODE ERR: {:?}", message.unwrap_err());
                    } else {
                        send_message_to_socket(socket, ip_addr, &message.unwrap());
                    }
                }
            }
            encode::MessageType::Xpst(_data) => {
                let res = cp.write(_data);
                if res.is_err() {
                    println!("CLIPBOARD ERR: {:?}", res.unwrap_err());
                }
            }
        }
    }
}
