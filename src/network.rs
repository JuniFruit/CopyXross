use std::{
    io::Write,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket},
};

use crate::{
    clipboard::Clipboard,
    debug_println,
    encode::{compose_message, MessageType},
};

#[derive(Debug)]
#[allow(dead_code)]
pub enum NetworkError {
    Connect(String),
    Write(String),
    Read(String),
}

pub const PROTOCOL_VER: u32 = 1;

#[allow(dead_code)]
pub fn debug_send(_socket: &UdpSocket, cp: &impl Clipboard) {
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
    let mut buf: [u8; 1024] = [0; 1024];

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

pub fn send_message_to_peer(peer_addr: &SocketAddr, data: &[u8]) -> Result<(), NetworkError> {
    let mut handler = TcpStream::connect(peer_addr).map_err(|err| {
        NetworkError::Connect(format!("Failed to establish TCP connection: {:?}", err))
    })?;
    let written = handler.write(data).map_err(|err| {
        NetworkError::Write(format!("Failed to write into TCP handler: {:?}", err))
    })?;
    if written < data.len() {
        return Err(NetworkError::Write(format!(
            "Buffer was not written in full. {}/{}",
            written,
            data.len()
        )));
    }

    Ok(())
}
