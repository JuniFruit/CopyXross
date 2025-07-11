#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

use std::{
    io::{ErrorKind, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    time::Duration,
};

use crate::{
    debug_println,
    encode::{compose_message, MessageType, PeerData},
    utils::{format_bytes_size, log_into_file, write_progress},
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

pub const BROADCAST_IP: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 255);
pub const BROADCAST_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(BROADCAST_IP), PORT);

pub trait NetworkListener: Sized {
    fn init(cb: Option<Box<dyn Fn()>>) -> Result<Self, NetworkError>;
    fn start_listen(&self) -> Result<(), NetworkError>;
    fn is_en0_connected() -> bool;
}

#[cfg(target_os = "macos")]
pub use macos::Network as NetworkChangeListener;
#[cfg(target_os = "windows")]
pub use windows::Network as NetworkChangeListener;

pub fn init_network_change_listener(
    cb: Option<Box<dyn Fn()>>,
) -> Result<impl NetworkListener, NetworkError> {
    NetworkChangeListener::init(cb)
}

pub fn socket(listen_on: SocketAddr) -> std::io::Result<UdpSocket> {
    let attempt = UdpSocket::bind(listen_on);
    match attempt {
        Ok(sock) => {
            let _ = log_into_file(format!("Bound socket to {}", listen_on).as_str());
            Ok(sock)
        }
        Err(err) => {
            let _ = log_into_file(format!("Could not bind: {:?}", err).as_str());
            Err(err)
        }
    }
}

pub fn listen_to_socket(
    socket: Option<&UdpSocket>,
    buf: &mut [u8; 1024],
) -> Option<(SocketAddr, Vec<u8>)> {
    if let Some(socket) = socket {
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
    } else {
        debug_println!("UDP Socket is not bound");
        None
    }
}

pub fn send_message_to_socket<A>(socket: Option<&UdpSocket>, target: A, data: &[u8])
where
    A: std::net::ToSocketAddrs,
{
    if let Some(socket) = socket {
        match socket.send_to(data, target) {
            Ok(amt) => {
                debug_println!("Sent packet size {} bytes", format_bytes_size(amt));
            }
            Err(e) => {
                debug_println!("Error sending message: {:?}", e)
            }
        }
    } else {
        let _ = log_into_file("Failed to send packet. UDP Socket is not bound!");
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
    s.set_broadcast(true)
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    s.set_read_timeout(Some(Duration::new(0, 500)))
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    s.set_write_timeout(Some(Duration::new(1, 0)))
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;

    let tcp = TcpListener::bind(bind).map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    s.set_nonblocking(true)
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    tcp.set_nonblocking(true)
        .map_err(|err| NetworkError::Connect(format!("{:?}", err)))?;
    Ok((s, tcp))
}

pub fn listen_to_tcp(
    socket: Option<&TcpListener>,
    buff: &mut Vec<u8>,
) -> Result<usize, NetworkError> {
    if let Some(socket) = socket {
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
    } else {
        Err(NetworkError::Read("Tcp socket is not bound!".to_string()))
    }
}
pub fn send_bye_packet(socket: Option<&UdpSocket>, target: SocketAddr) {
    let _ = log_into_file("Sending BYE message...");
    let disconnect_msg = compose_message(&MessageType::Xdis, PROTOCOL_VER).unwrap();
    send_message_to_socket(socket, target, &disconnect_msg);
}

pub fn send_greeting_packet(socket: Option<&UdpSocket>, target: SocketAddr, p_data: PeerData) {
    let _ = log_into_file("Sending greeting message...");
    let greeting_message =
        compose_message(&MessageType::Xcon(p_data), PROTOCOL_VER).unwrap_or_default();
    send_message_to_socket(socket, target, &greeting_message);
}
