use std::{
    io::{ErrorKind, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    time::Duration,
};

use crate::{
    clipboard::Clipboard,
    debug_println,
    encode::{compose_message, MessageType},
    utils::format_bytes_size,
    PORT,
};

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NetworkError {
    Connect(String),
    Write(String),
    Read(String),
    Blocked,
}

pub const PROTOCOL_VER: u32 = 1;

#[allow(dead_code)]
pub fn debug_send(_socket: &UdpSocket, cp: &impl Clipboard) {
    let addr = IpAddr::V4(Ipv4Addr::new(172, 20, 10, 6));
    let addr = SocketAddr::new(addr, PORT);
    let mut handler = TcpStream::connect(addr).unwrap();

    let cp_buff = cp.read().unwrap();
    let msg = compose_message(&MessageType::Xpst(cp_buff), PROTOCOL_VER).unwrap();

    let s = handler.write(&msg).unwrap();
    println!("Written to TCP: {}", s);

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
            debug_println!(
                "Received data from {}. Size: {}",
                src,
                format_bytes_size(_amt)
            );
            if _amt < 1 {
                return None;
            }
            Some((src, Vec::<u8>::from(&buf[.._amt])))
        }
        Err(err) => {
            if err.kind() != ErrorKind::WouldBlock && err.kind() != ErrorKind::TimedOut {
                debug_println!("Read error: {:?}", err);
            }
            None
        }
    }
}

pub fn send_message_to_socket(socket: &UdpSocket, target: SocketAddr, data: &[u8]) {
    match socket.send_to(data, target) {
        Ok(amt) => {
            debug_println!("Sent packet size {} bytes", format_bytes_size(amt));
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
    handler.write_all(data).map_err(|err| {
        NetworkError::Write(format!("Failed to write into TCP handler: {:?}", err))
    })?;
    // if written < data.len() {
    //     return Err(NetworkError::Write(format!(
    //         "Buffer was not written in full. {}/{}",
    //         format_bytes_size(written),
    //         format_bytes_size(data.len())
    //     )));
    // }
    debug_println!("Sent message via TCP: {:?}", format_bytes_size(data.len()));

    Ok(())
}

pub fn init_listeners(my_ip: IpAddr) -> Result<(UdpSocket, TcpListener), NetworkError> {
    let bind = SocketAddr::new(my_ip, PORT);
    let s = socket(bind).map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    s.set_broadcast(true)
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    s.set_read_timeout(Some(Duration::new(1, 0)))
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    s.set_write_timeout(Some(Duration::new(1, 0)))
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;

    let tcp = TcpListener::bind(bind).map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    tcp.set_nonblocking(true)
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    Ok((s, tcp))
}

pub fn listen_to_tcp(socket: &TcpListener, buff: &mut Vec<u8>) -> Result<usize, NetworkError> {
    let (mut data, _ip) = socket.accept().map_err(|err| {
        if err.kind() == ErrorKind::TimedOut || err.kind() == ErrorKind::WouldBlock {
            return NetworkError::Blocked;
        }
        NetworkError::Read(format!("{:?}", err))
    })?;

    let read = data
        .read_to_end(buff)
        .map_err(|err| NetworkError::Read(format!("{:?}", err)))?;
    debug_println!(
        "Received data via TCP from {:?}. Size: {}",
        _ip,
        format_bytes_size(read)
    );
    Ok(read)
}
