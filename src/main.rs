mod clipboard;
mod udp;
mod utils;

use clipboard::{new_clipboard, Clipboard};
use local_ip_address::local_ip;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
};
use udp::{
    compose_message, listen_to_socket, parse_message, send_message_to_socket, socket, MessageType,
    PeerData,
};
use utils::Rand;

const PROTOCOL_VER: u32 = 1;

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
    cp.read();

    // getting my peer name
    let my_peer_name = format!("PC_num-{}", rnd);
    let my_peer_data = udp::PeerData {
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

    // main listener loop
    loop {
        let res = listen_to_socket(&socket);
        if res.is_some() {
            let (ip_addr, data) = res.unwrap();
            let parsed = parse_message(data).unwrap_or_else(|err| {
                println!("Parsing error: {:?}", err);
                MessageType::NoMessage
            });
            match parsed {
                udp::MessageType::NoMessage => {
                    println!("Skipping message. Empty message received");
                }
                udp::MessageType::Xacn(_data) => {
                    println!("Ack got: {:?}", _data);
                    connection_map.insert(ip_addr.ip(), _data);
                }
                udp::MessageType::Xcon(_data) => {
                    println!("Connection got: {:?}", _data);
                    connection_map.insert(ip_addr.ip(), _data);
                    send_message_to_socket(&socket, ip_addr, &ack_msg);
                }
                udp::MessageType::Xcpy => {}
                udp::MessageType::Xpst(_data) => {}
            }
        }
    }
}

fn ping_apps_on_network(socket: &UdpSocket, subnet_ip: IpAddr, message: &[u8], port: u16) {
    let address_str = subnet_ip.to_string();
    let mut address = address_str
        .split(".")
        .map(|val| val.parse::<u8>().unwrap())
        .collect::<Vec<u8>>();
    let mut i: u8 = 0;
    while i != u8::MAX {
        let candidate_ip = IpAddr::V4(Ipv4Addr::new(
            address[0], address[1], address[2], address[3],
        ));

        let candidate_addr = SocketAddr::new(candidate_ip, port);
        // if candidate_ip == subnet_ip {
        //     continue;
        // }

        send_message_to_socket(socket, candidate_addr, message);
        i += 1;
        address[3] = i
    }
}
