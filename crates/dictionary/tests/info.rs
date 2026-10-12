use inkpaper_dictionary::{DictionaryInfo, InfoError};

#[test]
fn reads_the_ifo_fields() {
    let info = DictionaryInfo::parse(
        b"StarDict's dict ifo file\r\nversion=3.0.0\r\nbookname=Webster's 1913\r\n\
          wordcount=102217\r\nidxfilesize=2016521\r\nsametypesequence=h\r\n",
    )
    .unwrap();

    assert_eq!(info.name(), "Webster's 1913");
    assert_eq!(info.word_count(), 102_217);
    assert_eq!(info.index_file_size(), 2_016_521);
    assert_eq!(info.same_type_sequence(), Some("h"));
}

#[test]
fn reads_an_ifo_without_a_type_sequence() {
    let info = DictionaryInfo::parse(
        "\u{FEFF}StarDict's dict ifo file\nwordcount=2\nidxfilesize=30\nidxoffsetbits=32\n"
            .as_bytes(),
    )
    .unwrap();

    assert_eq!(info.name(), "");
    assert_eq!(info.same_type_sequence(), None);
}

#[test]
fn rejects_another_file() {
    assert_eq!(
        DictionaryInfo::parse(b"[Desktop Entry]\nwordcount=2\n"),
        Err(InfoError::NotStarDict),
    );
}

#[test]
fn rejects_missing_and_invalid_counts() {
    assert_eq!(
        DictionaryInfo::parse(b"StarDict's dict ifo file\nidxfilesize=30\n"),
        Err(InfoError::MissingField("wordcount")),
    );
    assert_eq!(
        DictionaryInfo::parse(b"StarDict's dict ifo file\nwordcount=two\nidxfilesize=30\n"),
        Err(InfoError::InvalidField("wordcount")),
    );
}

#[test]
fn rejects_64_bit_offsets() {
    assert_eq!(
        DictionaryInfo::parse(
            b"StarDict's dict ifo file\nwordcount=2\nidxfilesize=30\nidxoffsetbits=64\n",
        ),
        Err(InfoError::UnsupportedOffsetBits),
    );
}
