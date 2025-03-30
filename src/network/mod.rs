#[cfg(target_os = "macos")]
pub mod macos;

use std::{
    io::{ErrorKind, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    time::Duration,
};

use crate::{
    debug_println,
    encode::{compose_message, MessageType, PeerData},
    utils::{format_bytes_size, write_progress},
};

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NetworkError {
    Connect(String),
    Write(String),
    Read(String),
    Blocked,
    Init(String),
    Unexpected(String),
}

pub const PROTOCOL_VER: u32 = 1;
pub const PORT: u16 = 53300;

pub const MULTICAST_IP: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
pub const BROADCAST_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(MULTICAST_IP), PORT);

pub trait NetworkListener: Sized {
    fn init(cb: Option<Box<dyn Fn()>>) -> Result<Self, NetworkError>;
    fn start_listen(&self) -> Result<(), NetworkError>;
    fn is_en0_connected() -> bool;
}

#[cfg(target_os = "macos")]
pub use macos::Network as NetworkChangeListener;

pub fn init_network_change_listener(
    cb: Option<Box<dyn Fn()>>,
) -> Result<impl NetworkListener, NetworkError> {
    NetworkChangeListener::init(cb)
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

pub fn listen_to_socket(socket: &UdpSocket, buf: &mut [u8; 1024]) -> Option<(SocketAddr, Vec<u8>)> {
    let result = socket.recv_from(buf);
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
    // handler.write_all(data).map_err(|err| {
    //     NetworkError::Write(format!("Failed to write into TCP handler: {:?}", err))
    // })?;

    let mut total_written = 0;
    while total_written < data.len() {
        match handler.write(&data[total_written..]) {
            Ok(n) => total_written += n,
            Err(e) => return Err(NetworkError::Write(format!("{:?}", e))),
        }
        write_progress(total_written, data.len());
    }

    debug_println!("Sent message via TCP: {:?}", format_bytes_size(data.len()));

    Ok(())
}

pub fn init_listeners(my_ip: IpAddr) -> Result<(UdpSocket, TcpListener), NetworkError> {
    let bind = SocketAddr::new(my_ip, PORT);
    let s = socket(bind).map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    // s.set_broadcast(true)
    //     .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    s.set_read_timeout(Some(Duration::new(0, 100)))
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    s.set_write_timeout(Some(Duration::new(0, 100)))
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;

    s.join_multicast_v4(&MULTICAST_IP, &my_ip.to_string().parse().unwrap())
        .map_err(|err| NetworkError::Connect(format!("Could not join multicast: {:?}", err)))?;
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
    let mut buffer = vec![0; 1024];
    let mut read: usize = 0;
    loop {
        write_progress(read, 0);
        let res = data.read(&mut buffer);

        if res.is_ok() {
            let curr_read = res.unwrap();
            if curr_read == 0 {
                break;
            }
            read += curr_read;

            buff.extend_from_slice(&buffer[..curr_read]);
        } else {
            let err = res.unwrap_err();
            if let ErrorKind::WouldBlock = err.kind() {
                continue;
            } else {
                break;
            }
        }
    }

    debug_println!(
        "Received data via TCP from {:?}. Size: {}",
        _ip,
        format_bytes_size(read)
    );
    Ok(read)
}
pub fn send_bye_packet(socket: &UdpSocket, target: SocketAddr) {
    debug_println!("Sending BYE packet...");
    let disconnect_msg = compose_message(&MessageType::Xdis, PROTOCOL_VER).unwrap();
    send_message_to_socket(socket, target, &disconnect_msg);
}

pub fn send_greeting_packet(socket: &UdpSocket, target: SocketAddr, p_data: PeerData) {
    debug_println!("Sending greeting message...");
    let greeting_message =
        compose_message(&MessageType::Xcon(p_data), PROTOCOL_VER).unwrap_or_default();
    send_message_to_socket(socket, target, &greeting_message);
}
