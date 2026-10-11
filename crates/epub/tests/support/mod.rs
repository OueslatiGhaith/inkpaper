//! Builds the ZIP archives the EPUB tests open.

// each test crate uses only some of it
#![allow(dead_code)]

use miniz_oxide::deflate::compress_to_vec;

pub const STORED: u16 = 0;
pub const DEFLATED: u16 = 8;

pub struct ZipEntry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
    pub compression: u16,
}

/// A ZIP archive of `(name, data)` entries, all stored uncompressed.
pub fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let entries: Vec<_> = entries
        .iter()
        .map(|&(name, data)| ZipEntry {
            name,
            data,
            compression: STORED,
        })
        .collect();

    build_zip(&entries)
}

pub fn build_zip(entries: &[ZipEntry<'_>]) -> Vec<u8> {
    struct CentralEntry<'a> {
        name: &'a str,
        compression: u16,
        compressed_size: u32,
        uncompressed_size: u32,
        local_offset: u32,
    }

    let mut output = Vec::new();
    let mut central = Vec::new();

    for entry in entries {
        let compressed = match entry.compression {
            STORED => entry.data.to_vec(),
            DEFLATED => compress_to_vec(entry.data, 6),
            _ => unreachable!(),
        };

        let local_offset = u32::try_from(output.len()).unwrap();
        let compressed_size = u32::try_from(compressed.len()).unwrap();
        let uncompressed_size = u32::try_from(entry.data.len()).unwrap();
        let name_len = u16::try_from(entry.name.len()).unwrap();

        push_u32(&mut output, 0x0403_4b50);
        push_u16(&mut output, 20);
        // UTF-8 entry names.
        push_u16(&mut output, 0x0800);
        push_u16(&mut output, entry.compression);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        // CRC isn't validated by our V1 reader.
        push_u32(&mut output, 0);
        push_u32(&mut output, compressed_size);
        push_u32(&mut output, uncompressed_size);
        push_u16(&mut output, name_len);
        push_u16(&mut output, 0);

        output.extend_from_slice(entry.name.as_bytes());
        output.extend_from_slice(&compressed);

        central.push(CentralEntry {
            name: entry.name,
            compression: entry.compression,
            compressed_size,
            uncompressed_size,
            local_offset,
        });
    }

    let central_offset = u32::try_from(output.len()).unwrap();

    for entry in &central {
        let name_len = u16::try_from(entry.name.len()).unwrap();

        push_u32(&mut output, 0x0201_4b50);
        push_u16(&mut output, 20);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0x0800);
        push_u16(&mut output, entry.compression);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(&mut output, entry.compressed_size);
        push_u32(&mut output, entry.uncompressed_size);
        push_u16(&mut output, name_len);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(&mut output, entry.local_offset);

        output.extend_from_slice(entry.name.as_bytes());
    }

    let central_size = u32::try_from(output.len() - central_offset as usize).unwrap();
    let entry_count = u16::try_from(central.len()).unwrap();

    push_u32(&mut output, 0x0605_4b50);
    push_u16(&mut output, 0);
    push_u16(&mut output, 0);
    push_u16(&mut output, entry_count);
    push_u16(&mut output, entry_count);
    push_u32(&mut output, central_size);
    push_u32(&mut output, central_offset);
    push_u16(&mut output, 0);

    output
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
