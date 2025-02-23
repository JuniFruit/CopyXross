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
mod transferable;

use crate::clipboard::ClipboardData;
use crate::debug_println;
use protocol::{
    encode_chunks, encode_header, encode_size, read_data, read_header, read_header_expected,
    read_size, Chunk, EncodeError, ReaderOffset,
};

pub use protocol::{HeaderType, MessageType, ParseErrors, PeerData};
use std::{
    net::{SocketAddr, UdpSocket},
    str::FromStr,
};
pub use transferable::Transferable;

pub fn parse_message(data: Vec<u8>) -> Result<MessageType, ParseErrors> {
    let mut reader = ReaderOffset { offset: 0 };
    read_header_expected(&data, &mut reader, "XCOP")?;
    let file_size = read_size(&data, &mut reader)?;

    debug_println!("Reading message. Size: {:?}", file_size);

    while reader.offset <= data.len() {
        let header = read_header(&data, &mut reader)?;
        let header = HeaderType::from_str(&header)?;
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
    let mut header: String = String::new();
    let mut bytes: Vec<u8> = vec![];
    match message {
        MessageType::Xcon(_data) => {
            header = HeaderType::Xcon.to_string();
            bytes = _data.serialize()?;
        }
        MessageType::Xacn(_data) => {
            header = HeaderType::Xacn.to_string();
            bytes = _data.serialize()?;
        }
        MessageType::Xcpy => {
            header = HeaderType::Xcpy.to_string();
        }
        MessageType::Xpst(data) => {
            header = HeaderType::Xpst.to_string();
            bytes = data.serialize()?;
        }
        MessageType::NoMessage => {}
    }
    // signature chunk
    encode_header(&HeaderType::Xcop.to_string(), &mut result);
    encode_size(bytes.len() + 4 + 4, &mut result)?;
    let ver_header = HeaderType::Xver.to_string();
    let ver_data = u32::to_be_bytes(protocol_ver);
    let chunks: Vec<Chunk> = vec![
        // protocol_ver chunk
        Chunk::new(&ver_header, &ver_data),
        // main message
        Chunk::new(&header, &bytes),
    ];
    encode_chunks(&chunks, &mut result)?;

    Ok(result)
}
