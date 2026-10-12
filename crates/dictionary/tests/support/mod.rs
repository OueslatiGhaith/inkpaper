//! Builds the StarDict files the dictionary tests open.

// each test crate uses only some of it
#![allow(dead_code)]

/// The three files of a StarDict dictionary, built in memory.
pub struct StarDict {
    pub ifo: String,
    pub idx: Vec<u8>,
    pub dict: Vec<u8>,
}

/// Builds a dictionary from entries given in StarDict's sort order, each
/// definition's data stored as it is.
pub fn stardict(same_type_sequence: Option<&str>, entries: &[(&str, &[u8])]) -> StarDict {
    let mut idx = Vec::new();
    let mut dict = Vec::new();

    for (word, data) in entries {
        idx.extend_from_slice(word.as_bytes());
        idx.push(0);
        idx.extend_from_slice(&(dict.len() as u32).to_be_bytes());
        idx.extend_from_slice(&(data.len() as u32).to_be_bytes());

        dict.extend_from_slice(data);
    }

    let mut ifo = format!(
        "StarDict's dict ifo file\nversion=2.4.2\nbookname=Test\nwordcount={}\nidxfilesize={}\n",
        entries.len(),
        idx.len(),
    );

    if let Some(types) = same_type_sequence {
        ifo.push_str(&format!("sametypesequence={types}\n"));
    }

    StarDict { ifo, idx, dict }
}

/// The optional gzip header fields to write before the deflated chunks.
#[derive(Default)]
pub struct GzipHeader<'a> {
    pub name: Option<&'a str>,
    pub comment: Option<&'a str>,
    pub crc: bool,
}

/// Compresses `data` like dictzip(1): chunks of `chunk_len` bytes deflated
/// with a full flush, their sizes in the gzip header's `RA` field.
pub fn dictzip(data: &[u8], chunk_len: u16, header: &GzipHeader) -> Vec<u8> {
    use miniz_oxide::deflate::core::{
        CompressorOxide, TDEFLFlush, compress, create_comp_flags_from_zip_params,
    };

    let mut compressor = CompressorOxide::new(create_comp_flags_from_zip_params(9, -15, 0));
    let mut chunks = Vec::new();
    let mut sizes = Vec::new();

    for chunk in data.chunks(usize::from(chunk_len)) {
        let mut out = vec![0; chunk.len() * 2 + 64];
        let (_, read, written) = compress(&mut compressor, chunk, &mut out, TDEFLFlush::Full);

        assert_eq!(read, chunk.len());

        chunks.extend_from_slice(&out[..written]);
        sizes.push(written as u16);
    }

    // the stream's last block, after the last chunk
    let mut out = vec![0; 64];
    let (_, _, written) = compress(&mut compressor, &[], &mut out, TDEFLFlush::Finish);
    chunks.extend_from_slice(&out[..written]);

    let mut field = Vec::new();
    field.extend_from_slice(&1u16.to_le_bytes());
    field.extend_from_slice(&chunk_len.to_le_bytes());
    field.extend_from_slice(&(sizes.len() as u16).to_le_bytes());

    for size in sizes {
        field.extend_from_slice(&size.to_le_bytes());
    }

    let mut extra = b"RA".to_vec();
    extra.extend_from_slice(&(field.len() as u16).to_le_bytes());
    extra.extend_from_slice(&field);

    let mut flags = 0x04;

    if header.crc {
        flags |= 0x02;
    }
    if header.name.is_some() {
        flags |= 0x08;
    }
    if header.comment.is_some() {
        flags |= 0x10;
    }

    let mut file = vec![0x1f, 0x8b, 8, flags, 0, 0, 0, 0, 2, 3];
    file.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    file.extend_from_slice(&extra);

    for text in [header.name, header.comment].into_iter().flatten() {
        file.extend_from_slice(text.as_bytes());
        file.push(0);
    }

    if header.crc {
        file.extend_from_slice(&[0, 0]);
    }

    file.extend_from_slice(&chunks);
    // the crc, unchecked, then the inflated size
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&(data.len() as u32).to_le_bytes());

    file
}
