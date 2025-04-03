use crate::{clipboard::ClipboardData, utils::log_into_file};
use std::str::FromStr;

const HEADER_SIZE: usize = 4;
const LENGTH_SIZE: usize = 4;
#[derive(Debug, PartialEq, Clone)]
pub struct PeerData {
    pub peer_name: String,
}

#[derive(Debug, PartialEq)]
pub enum MessageType {
    Xacn(PeerData),
    Xcon(PeerData),
    Xcpy,
    Xdis,
    Xpst(ClipboardData),
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
    Xdis,
}

impl FromStr for HeaderType {
    type Err = ParseErrors;

    fn from_str(input: &str) -> Result<HeaderType, Self::Err> {
        match input {
            "XCOP" => Ok(HeaderType::Xcop),
            "XVER" => Ok(HeaderType::Xver),
            "XACN" => Ok(HeaderType::Xacn),
            "XCON" => Ok(HeaderType::Xcon),
            "XCPY" => Ok(HeaderType::Xcpy),
            "XPST" => Ok(HeaderType::Xpst),
            "XDIS" => Ok(HeaderType::Xdis),
            _ => Err(ParseErrors::UnknownHeader(format!(
                "Unknown header: {}",
                input
            ))),
        }
    }
}
impl HeaderType {
    pub fn to_string(&self) -> &str {
        match self {
            Self::Xver => "XVER",
            Self::Xcop => "XCOP",
            Self::Xacn => "XACN",
            Self::Xcon => "XCON",
            Self::Xcpy => "XCPY",
            Self::Xpst => "XPST",
            Self::Xdis => "XDIS",
        }
    }
}
#[allow(dead_code)]
#[derive(Debug)]
pub enum ParseErrors {
    InvalidStructure,
    OutOfBounds,
    UnknownHeader(String),
}

#[derive(Debug)]
pub enum EncodeError {
    TooBig,
    Overflow,
}

pub struct ReaderOffset {
    pub offset: usize,
}

impl ReaderOffset {
    pub fn increase_by(&mut self, val: usize) {
        self.offset += val;
    }
}

pub fn check_offset_bounds(data: &[u8], offset: usize, size: usize) -> Result<(), ParseErrors> {
    if data.len() < offset + size {
        return Err(ParseErrors::OutOfBounds);
    }

    Ok(())
}

pub fn read_header(data: &[u8], o: &mut ReaderOffset) -> Result<String, ParseErrors> {
    check_offset_bounds(data, o.offset, HEADER_SIZE)?;
    let header: String = String::from_utf8(data[o.offset..o.offset + HEADER_SIZE].to_vec())
        .map_err(|err| {
            let _ =
                log_into_file(format!("Error occurred while parsing header: {:?}", err).as_str());
            ParseErrors::InvalidStructure
        })?;
    o.increase_by(HEADER_SIZE);
    Ok(header)
}

pub fn read_header_expected(
    data: &[u8],
    o: &mut ReaderOffset,
    expected: &str,
) -> Result<(), ParseErrors> {
    let header = read_header(data, o)?;
    if header != expected {
        return Err(ParseErrors::UnknownHeader(format!(
            "Expected header: {}. Received instead: {}",
            expected, header
        )));
    }
    Ok(())
}

pub fn read_size(data: &[u8], o: &mut ReaderOffset) -> Result<usize, ParseErrors> {
    check_offset_bounds(data, o.offset, LENGTH_SIZE)?;
    let slice: [u8; 4] = data[o.offset..o.offset + LENGTH_SIZE]
        .try_into()
        .map_err(|err| {
            let _ = log_into_file(format!("Error occurred while parsing len: {:?}", err).as_str());
            ParseErrors::InvalidStructure
        })?;
    let size: u32 = u32::from_be_bytes(slice);
    o.increase_by(LENGTH_SIZE);
    Ok(size as usize)
}

pub fn read_data(data: &[u8], o: &mut ReaderOffset, size: usize) -> Result<Vec<u8>, ParseErrors> {
    check_offset_bounds(data, o.offset, size)?;
    let read = data[o.offset..o.offset + size].to_vec();

    o.increase_by(size);
    Ok(read)
}

pub fn encode_data(data: &[u8], out: &mut Vec<u8>) -> Result<(), EncodeError> {
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

pub fn encode_header(header: &str, out: &mut Vec<u8>) {
    let bytes = header.to_string().as_bytes().to_vec();
    out.extend(bytes);
}

pub struct Chunk<'a> {
    header: &'a str,
    data: &'a [u8],
}

impl<'a> Chunk<'a> {
    pub fn new(chunk_header: &'a str, chunk_data: &'a [u8]) -> Self {
        Chunk {
            header: chunk_header,
            data: chunk_data,
        }
    }
    pub fn encode_chunk(&self, out: &mut Vec<u8>) -> Result<(), EncodeError> {
        encode_header(self.header, out);
        encode_data(self.data, out)?;
        Ok(())
    }
}

pub fn encode_chunks(chunks: &Vec<Chunk>, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    for c in chunks {
        c.encode_chunk(out)?;
    }
    Ok(())
}
