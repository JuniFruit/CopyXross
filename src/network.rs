use std::{
    io::Write,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket},
};

use crate::{
    clipboard::Clipboard,
    debug_println,
    encode::{compose_message, MessageType},
};

pub const PROTOCOL_VER: u32 = 1;

pub fn ping_apps_on_network(socket: &UdpSocket, subnet_ip: IpAddr, message: &[u8], port: u16) {
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
        if candidate_ip == subnet_ip {
            continue;
        }

        send_message_to_socket(socket, candidate_addr, message);
        i += 1;
        address[3] = i
    }
}

pub fn debug_send(socket: &UdpSocket, cp: &impl Clipboard) {
    let addr = IpAddr::V4(Ipv4Addr::new(172, 20, 10, 6));
    let addr = SocketAddr::new(addr, 53300);
    let mut handler = TcpStream::connect(addr).unwrap();

    let cp_buff = cp.read().unwrap();
    let msg = compose_message(&MessageType::Xpst(cp_buff), PROTOCOL_VER).unwrap();

    handler.write_all(&msg).unwrap();

    // send_message_to_socket(socket, addr, &msg);
}

pub fn socket(listen_on: SocketAddr) -> std::io::Result<UdpSocket> {
    let attempt = UdpSocket::bind(listen_on);
    match attempt {
        Ok(sock) => {
            println!("Bound socket to {}", listen_on);
            Ok(sock)
        }
        Err(err) => {
            println!("Could not bind: {:?}", err);
            Err(err)
        }
    }
}

pub fn listen_to_socket(socket: &UdpSocket) -> Option<(SocketAddr, Vec<u8>)> {
    let mut buf: [u8; 5024] = [0; 5024];

    let result = socket.recv_from(&mut buf);
    match result {
        Ok((_amt, src)) => {
            debug_println!("Received data from {}. Size: {}", src, _amt);
            if _amt < 1 {
                return None;
            }
            Some((src, Vec::<u8>::from(&buf[.._amt])))
        }
        Err(err) => {
            debug_println!("Read error: {:?}", err);
            None
        }
    }
}

pub fn send_message_to_socket(socket: &UdpSocket, target: SocketAddr, data: &[u8]) {
    match socket.send_to(data, target) {
        Ok(amt) => {
            debug_println!("Sent packet size {} bytes", amt);
        }
        Err(e) => {
            debug_println!("Error sending message: {:?}", e)
        }
    }
}
