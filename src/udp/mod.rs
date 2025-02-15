//! App communication protocol
//!
//! Provides necessary functions to ensure that messages between apps are
//! correctly composed and read.
//!
//! Protocol is loosely based on IFF file type encoding.
//!
//! Each message consists of chunks and they are organized in certain order.
//! Each chunk has 4-byte header, 4-byte length info and the data itself.
//!
//! Exmaple:
//!
//! <table>
//!     <thead>
//!         <tr>
//!             <th>Offset</th>
//!             <th>Len</th>
//!             <th>Header</th>
//!             <th>Example</th>
//!         </tr>
//!     </thead>
//!     <tbody>
//!         <tr>
//!             <td>0</td>
//!             <td>4</td>
//!             <td>
//!                 Header
//!             </td>
//!             <td>XCOP</td>
//!         </tr>
//!         <tr>
//!             <td>4</td>
//!             <td>4</td>
//!             <td>
//!                 Message (chunk) Length
//!             </td>
//!             <td>13</td>
//!         </tr>
//!         <tr>
//!             <td>8</td>
//!             <td>4</td>
//!             <td>
//!                 Header
//!             </td>
//!             <td>XVER</td>
//!         </tr>
//!         <tr>
//!             <td>12</td>
//!             <td>4</td>
//!             <td>
//!                 Len
//!             </td>
//!             <td>1</td>
//!         </tr>
//!         <tr>
//!             <td>16</td>
//!             <td>1</td>
//!             <td>
//!                 Version
//!             </td>
//!             <td>1</td>
//!         </tr>
//!         <tr>
//!             <td>17</td>
//!             <td>4</td>
//!             <td>
//!                 Header
//!             </td>
//!             <td>PING</td>
//!         </tr>
//!         <tr>
//!             <td>21</td>
//!             <td>4</td>
//!             <td>
//!                 Length
//!             </td>
//!             <td>0</td>
//!         </tr>
//!     </tbody>
//! </table>
//!

mod protocol;

use crate::{clipboard::ClipboardData, debug_println};
use protocol::{
    encode_data, encode_header, encode_message_heading, read_data, read_header, read_size,
    EncodeError, ReaderOffset,
};

pub use protocol::{HeaderType, MessageType, ParseErrors, PeerData, Transferable};
use std::net::{SocketAddr, UdpSocket};

pub fn parse_message(data: Vec<u8>) -> Result<MessageType, ParseErrors> {
    let mut reader = ReaderOffset { offset: 0 };
    let file_header = read_header(&data, &mut reader)?;
    let file_size = read_size(&data, &mut reader)?;

    debug_println!("Reading message. Size: {:?}", file_size);

    if file_header != HeaderType::Xcop {
        return Err(ParseErrors::InvalidStructure);
    }

    while reader.offset <= data.len() {
        let header = read_header(&data, &mut reader)?;
        let size = read_size(&data, &mut reader)?;
        let data = read_data(&data, &mut reader, size)?;

        match header {
            HeaderType::Xver => {
                // let data: [u8; 4] = data.as_slice().try_into().map_err(|err| {
                //     println!("Could not parse version: {:?}", err);
                //     ParseErrors::InvalidStructure
                // })?;
            }
            HeaderType::Xacn => {
                let data = data.as_slice();
                let peer_d = PeerData::deserialize(data)?;
                return Ok(MessageType::Xacn(peer_d));
            }
            HeaderType::Xcon => {
                let data = data.as_slice();
                let peer_d = PeerData::deserialize(data)?;
                return Ok(MessageType::Xcon(peer_d));
            }
            HeaderType::Xcpy => return Ok(MessageType::Xcpy),
            HeaderType::Xpst => {
                let decoded = ClipboardData::deserialize(data.as_slice())?;
                return Ok(MessageType::Xpst(decoded));
            }
            HeaderType::Xcop => {
                // already handled
                continue;
            }
        }
    }
    Ok(MessageType::NoMessage)
}

pub fn compose_message(message: &MessageType, protocol_ver: u32) -> Result<Vec<u8>, EncodeError> {
    let mut result: Vec<u8> = vec![];
    let mut out: Vec<u8> = vec![];
    match message {
        MessageType::Xcon(_data) => {
            encode_header(HeaderType::Xcon, &mut out)?;
            let bytes = _data.serialize()?;
            encode_data(&bytes, &mut out)?;
        }
        MessageType::Xacn(_data) => {
            encode_header(HeaderType::Xacn, &mut out)?;
            let bytes = _data.serialize()?;
            encode_data(&bytes, &mut out)?;
        }
        MessageType::Xcpy => {
            encode_header(HeaderType::Xcpy, &mut out)?;
            encode_data(&vec![], &mut out)?;
        }
        MessageType::Xpst(data) => {
            encode_header(HeaderType::Xpst, &mut out)?;
            let encoded = data.serialize()?;
            encode_data(&encoded, &mut out)?;
        }
        MessageType::NoMessage => {}
    }
    // encode first part of the message
    encode_message_heading(protocol_ver, out.len(), &mut result)?;
    // append message
    result.extend(out);

    Ok(result)
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
