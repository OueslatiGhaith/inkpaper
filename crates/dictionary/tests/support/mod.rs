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
