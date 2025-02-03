mod udp;
mod utils;

use local_ip_address::local_ip;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};
use udp::{
    compose_message, listen_to_socket, parse_message, send_message_to_socket, socket, SocketMsg,
};
use utils::Rand;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    println!("Starting...");
    println!("Scanning network...");
    let my_local_ip = local_ip().expect("Could not determine my ip");
    println!("This is my local IP address: {:?}", my_local_ip);
    let port = 53300;
    let (tx, rx): (Sender<SocketMsg>, Receiver<SocketMsg>) = mpsc::channel();

    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);
    let socket = socket(bind).unwrap();
    ping_apps_on_network(&socket, my_local_ip.to_canonical(), port);
    let listener_handle = thread::spawn(move || listen_to_socket(&socket, &tx));

    loop {
        let res = rx.try_recv();
        if res.is_err() {
            match res.unwrap_err() {
                mpsc::TryRecvError::Empty => continue,
                mpsc::TryRecvError::Disconnected => {
                    println!("Critical: listener thread is down!");
                    break;
                }
            }
        } else {
            let data = res.unwrap().1;
            let parsed = parse_message(data).unwrap_or(udp::MessageType::NoMessage);
            match parsed {
                udp::MessageType::NoMessage => {
                    println!("Skipping message. Empty message received");
                }
                udp::MessageType::Xacn(_data) => {
                    println!("Ack got: {:?}", _data);
                }
                udp::MessageType::Xcon(_data) => {
                    println!("Connection got: {:?}", _data);
                }
                udp::MessageType::Xcpy => {}
                udp::MessageType::Xpst(_data) => {}
            }
        }
    }

    listener_handle
        .join()
        .expect("Failed to close listener thread");
    println!("Finising the execution");
}

fn ping_apps_on_network(socket: &UdpSocket, subnet_ip: IpAddr, port: u16) {
    let address_str = subnet_ip.to_string();
    let mut address = address_str
        .split(".")
        .map(|val| val.parse::<u8>().unwrap())
        .collect::<Vec<u8>>();
    let mut i: u8 = 0;
    let mut randomizer = Rand::new(0);

    while i != u8::MAX {
        let candidate_addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(
                address[0], address[1], address[2], address[3],
            )),
            port,
        );

        println!("Pinging: {:?}", candidate_addr);
        let rnd = randomizer.rand();
        let ping_message = compose_message(
            udp::MessageType::Xcon(udp::PeerData {
                peer_name: format!("PC_num-{}", rnd),
            }),
            1,
        )
        .map_err(|err| {
            println!("Failed to compose a message: {:?}", err);
        })
        .unwrap();
        send_message_to_socket(socket, candidate_addr, ping_message);
        i += 1;
        address[3] = i;
    }
}
