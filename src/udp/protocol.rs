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

use std::str::FromStr;

const HEADER_SIZE: usize = 4;
const LENGTH_SIZE: usize = 4;

#[derive(Debug, PartialEq)]
pub struct PeerData {
    pub peer_name: String,
}

#[derive(Debug, PartialEq)]
pub enum MessageType {
    Xacn(PeerData),
    Xcon(PeerData),
    Xcpy,
    Xpst(Vec<u8>),
    NoMessage,
}
#[derive(Debug, PartialEq)]
pub enum HeaderType {
    Xver,
    Xcop,
    Xacn,
    Xcon,
    Xcpy,
    Xpst,
}

impl FromStr for HeaderType {
    type Err = ();

    fn from_str(input: &str) -> Result<HeaderType, Self::Err> {
        match input {
            "XCOP" => Ok(HeaderType::Xcop),
            "XVER" => Ok(HeaderType::Xver),
            "XACK" => Ok(HeaderType::Xacn),
            "XCON" => Ok(HeaderType::Xcon),
            "XCPY" => Ok(HeaderType::Xcpy),
            "XPST" => Ok(HeaderType::Xpst),
            _ => Err(()),
        }
    }
}
impl ToString for HeaderType {
    fn to_string(&self) -> String {
        match self {
            Self::Xver => String::from("XVER"),
            Self::Xcop => String::from("XCOP"),
            Self::Xacn => String::from("XACN"),
            Self::Xcon => String::from("XCON"),
            Self::Xcpy => String::from("XCPY"),
            Self::Xpst => String::from("XPST"),
        }
    }
}
#[derive(Debug)]
pub enum ParseErrors {
    InvalidStructure,
    OutOfBounds,
    UnknownHeader,
}

#[derive(Debug)]
pub enum EncodeError {
    TooBig,
}

pub struct ReaderOffset {
    pub offset: usize,
}

impl ReaderOffset {
    pub fn increase_by(&mut self, val: usize) {
        self.offset += val;
    }
}

fn check_offset_bounds(data: &Vec<u8>, offset: usize, size: usize) -> Result<(), ParseErrors> {
    if data.len() < offset + size {
        return Err(ParseErrors::OutOfBounds);
    }

    Ok(())
}

pub fn read_header(data: &Vec<u8>, o: &mut ReaderOffset) -> Result<HeaderType, ParseErrors> {
    check_offset_bounds(data, o.offset, HEADER_SIZE)?;
    let header: String = String::from_utf8(data[o.offset..o.offset + HEADER_SIZE].to_vec())
        .map_err(|err| {
            println!("Error occurred while parsing header: {:?}", err);
            ParseErrors::InvalidStructure
        })?;
    o.increase_by(HEADER_SIZE);
    Ok(HeaderType::from_str(&header).map_err(|_| ParseErrors::UnknownHeader))?
}

pub fn read_size(data: &Vec<u8>, o: &mut ReaderOffset) -> Result<usize, ParseErrors> {
    check_offset_bounds(data, o.offset, LENGTH_SIZE)?;
    let slice: [u8; 4] = data[o.offset..o.offset + LENGTH_SIZE]
        .try_into()
        .map_err(|err| {
            println!("Error occurred while parsing len: {:?}", err);
            ParseErrors::InvalidStructure
        })?;
    let size: u32 = u32::from_be_bytes(slice);
    o.increase_by(LENGTH_SIZE);
    Ok(size as usize)
}

pub fn read_data(
    data: &Vec<u8>,
    o: &mut ReaderOffset,
    size: usize,
) -> Result<Vec<u8>, ParseErrors> {
    check_offset_bounds(data, o.offset, size)?;
    let read = data[o.offset..o.offset + size].to_vec();

    o.increase_by(size);
    Ok(read)
}

pub fn encode_data(data: Vec<u8>, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    encode_size(data.len(), out)?;
    out.extend(data);
    Ok(())
}

pub fn encode_size(size: usize, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    if size as u32 > u32::MAX {
        return Err(EncodeError::TooBig);
    }
    let size: u32 = size.try_into().unwrap();
    let chunk_len = u32::to_be_bytes(size);
    out.extend_from_slice(chunk_len.as_slice());
    Ok(())
}

pub fn encode_message_heading(
    protocol_ver: u32,
    file_size: usize,
    out: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    // encode first message header
    encode_header(HeaderType::Xcop, out)?;
    encode_size(file_size, out)?;
    // encode protocol version chunk
    encode_header(HeaderType::Xver, out)?;
    encode_data(u32::to_be_bytes(protocol_ver).to_vec(), out)?;
    Ok(())
}

pub fn encode_header(header: HeaderType, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    let bytes = header.to_string().as_bytes().to_vec();
    out.extend(bytes);
    Ok(())
}
