use std::str::FromStr;

use crate::clipboard::ClipboardData;
use crate::clipboard::StringType;
use crate::debug_println;
use crate::utils::format_bytes_size;

use super::protocol::check_offset_bounds;
use super::protocol::encode_chunks;
use super::protocol::read_data;
use super::protocol::read_header_expected;
use super::protocol::read_size;
use super::protocol::Chunk;
use super::protocol::EncodeError;
use super::protocol::ReaderOffset;
use super::ParseErrors;
use super::PeerData;

pub trait Transferable: Sized {
    fn serialize(&self) -> std::result::Result<Vec<u8>, EncodeError>;
    fn deserialize(data: &[u8]) -> std::result::Result<Self, ParseErrors>;
}

impl Transferable for PeerData {
    fn serialize(&self) -> std::result::Result<Vec<u8>, EncodeError> {
        // -24 for String struct, +1 for u8 string len
        let str_len = self.peer_name.len();
        let mut encoded: Vec<u8> = Vec::with_capacity((size_of::<Self>() - 24) + str_len + 1);

        let str_len: u8 = str_len.try_into().map_err(|err| {
            println!("Failed to serialize PeerData: {:?}", err);
            EncodeError::Overflow
        })?;
        encoded.extend(str_len.to_be_bytes());
        encoded.extend(self.peer_name.as_bytes());
        Ok(encoded)
    }
    fn deserialize(data: &[u8]) -> std::result::Result<Self, ParseErrors> {
        check_offset_bounds(data, 0, 8)?;

        let mut peer_data = PeerData {
            peer_name: String::new(),
        };

        let slice: [u8; 1] = data[0..1].try_into().map_err(|err| {
            println!("Error occurred while deserializing PeerData: {:?}", err);
            ParseErrors::InvalidStructure
        })?;
        let str_len = u8::from_be_bytes(slice);
        let str_len: usize = str_len.try_into().map_err(|err| {
            println!("Error occurred while deserializing PeerData: {:?}", err);
            ParseErrors::InvalidStructure
        })?;
        check_offset_bounds(data, 1, str_len)?;

        let peer_name = String::from_utf8(data[1..=str_len].to_vec()).map_err(|err| {
            println!("Error occurred while deserializing PeerData: {:?}", err);
            ParseErrors::InvalidStructure
        })?;

        peer_data.peer_name = peer_name;

        Ok(peer_data)
    }
}

impl Transferable for ClipboardData {
    fn deserialize(data: &[u8]) -> std::result::Result<Self, ParseErrors> {
        let mut o: ReaderOffset = ReaderOffset { offset: 0 };
        let chunk_header = String::from_utf8(read_data(data, &mut o, 4)?).map_err(|err| {
            ParseErrors::UnknownHeader(format!("Unable to deserialize Clipboard data: {:?}", err))
        })?;

        match chunk_header.as_str() {
            "XSTR" => {
                let len = read_size(data, &mut o)?;
                debug_println!(
                    "Reading string from clipboard. Length: {}",
                    format_bytes_size(len)
                );

                read_header_expected(data, &mut o, "XTYP")?;
                let s_type_len = read_size(data, &mut o)?;
                let s_type =
                    String::from_utf8(read_data(data, &mut o, s_type_len)?).map_err(|err| {
                        println!("Could not read string data type chunk: {:?}", err);
                        ParseErrors::InvalidStructure
                    })?;
                let s_type = StringType::from_str(&s_type).map_err(|err| {
                    println!("Invalid string type: {:?}", err);
                    ParseErrors::InvalidStructure
                });

                read_header_expected(data, &mut o, "XDAT")?;
                let s_data_len = read_size(data, &mut o)?;
                let string_buff = read_data(data, &mut o, s_data_len)?;

                Ok(ClipboardData::String((s_type?, string_buff)))
            }
            "XFIL" => {
                let len = read_size(data, &mut o)?;
                debug_println!(
                    "Reading file from clipboard. Length: {}",
                    format_bytes_size(len)
                );
                read_header_expected(data, &mut o, "XFME")?;

                let filename_size = read_size(data, &mut o)?;
                let filename =
                    String::from_utf8(read_data(data, &mut o, filename_size)?).map_err(|err| {
                        println!("Failed to read filename string: {:?}", err);
                        ParseErrors::InvalidStructure
                    })?;
                read_header_expected(data, &mut o, "XDAT")?;
                let file_len = read_size(data, &mut o)?;
                let file_data = read_data(data, &mut o, file_len)?;

                Ok(ClipboardData::File((filename, file_data)))
            }
            _ => Err(ParseErrors::UnknownHeader(format!(
                "Invalid clipboard data header: {:?}",
                chunk_header
            ))),
        }
    }
    fn serialize(&self) -> std::result::Result<Vec<u8>, EncodeError> {
        match self {
            ClipboardData::String((s_type, data)) => {
                let mut out: Vec<u8> = vec![];
                let s_type = s_type.to_string();
                let chunks: Vec<Chunk> = vec![
                    Chunk::new("XTYP", s_type.as_bytes()),
                    Chunk::new("XDAT", data),
                ];
                encode_chunks(&chunks, &mut out)?;
                let header_chunk = Chunk::new("XSTR", &out);
                let mut encoded = vec![];
                header_chunk.encode_chunk(&mut encoded)?;
                Ok(encoded)
            }
            ClipboardData::File(file_data) => {
                let mut out: Vec<u8> = vec![];
                let (filename, data) = file_data;
                let chunks = vec![
                    Chunk::new("XFME", filename.as_bytes()),
                    Chunk::new("XDAT", data),
                ];
                encode_chunks(&chunks, &mut out)?;
                let mut encoded = vec![];
                let header_chunk = Chunk::new("XFIL", &out);
                header_chunk.encode_chunk(&mut encoded)?;

                Ok(encoded)
            }
        }
    }
}
