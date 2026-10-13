//! StarDict dictionaries for the simulator's `/dictionaries`, defining the
//! words of the long-text and hyphenation fixtures. One is HTML in a
//! `.dict.dz` with synonyms, the other plain text in a `.dict`.

use std::path::Path;

use anyhow::Result;
use miniz_oxide::deflate::core::{
    CompressorOxide, TDEFLFlush, compress, create_comp_flags_from_zip_params,
};

/// Small, so longer definitions span chunks.
const DICTZIP_CHUNK_LEN: u16 = 1024;

/// What a dictionary's files are named and say they hold.
struct Fixture {
    folder: &'static str,
    stem: &'static str,
    name: &'static str,
    same_type_sequence: &'static str,
    compressed: bool,
}

const HTML: Fixture = Fixture {
    folder: "Fixture HTML",
    stem: "fixture-html",
    name: "Fixture HTML Dictionary",
    same_type_sequence: "h",
    compressed: true,
};

const PLAIN: Fixture = Fixture {
    folder: "Fixture Plain",
    stem: "fixture-plain",
    name: "Fixture Plain Dictionary",
    same_type_sequence: "tm",
    compressed: false,
};

const HTML_ENTRIES: &[(&str, &str)] = &[
    (
        "accompany",
        "<i>verb</i><br/>To go somewhere with someone, or to come with something.",
    ),
    (
        "acknowledge",
        "<i>verb</i><br/>To accept or admit that something exists or is true.",
    ),
    (
        "administration",
        "<i>noun</i><br/>The people who manage an organization or govern a country.",
    ),
    (
        "backward",
        "<i>adverb</i><br/>Toward the back, or toward the beginning.",
    ),
    (
        "boundary",
        "<i>noun</i><br/>A line that marks where something ends.",
    ),
    (
        "characteristic",
        "<i>adjective</i><br/>Typical of a person or thing.",
    ),
    (
        "characterization",
        "<i>noun</i><br/>The way something is described or portrayed.",
    ),
    (
        "complicated",
        "<i>adjective</i><br/>Made of many parts, and so hard to understand.",
    ),
    (
        "conscientious",
        "<i>adjective</i><br/>Wanting to do one's work well and thoroughly.",
    ),
    (
        "contain",
        "<i>verb</i><br/>To have something inside, or as a part.",
    ),
    (
        "counterproductive",
        "<i>adjective</i><br/>Having the opposite of the effect intended.",
    ),
    (
        "deliberate",
        concat!(
            "<i>adjective</i><ol><li>Done on purpose: <i>a deliberate lie</i>.</li>",
            "<li>Slow and careful.<ul><li><i>She spoke in a deliberate voice.</i></li></ul></li></ol>",
            "<i>verb</i><ol><li>To think <b>carefully</b> before deciding.</li></ol>",
        ),
    ),
    (
        "deterministic",
        "<i>adjective</i><br/>Always producing the same result from the same input.",
    ),
    (
        "disproportionate",
        "<i>adjective</i><br/>Too large or too small in relation to something else.",
    ),
    (
        "documentation",
        "<i>noun</i><br/>Documents that describe or explain something, like a program's manual &amp; its examples.",
    ),
    (
        "even",
        "<i>adjective</i><br/>Flat, level or equal; with no part larger than another.",
    ),
    (
        "extraordinary",
        "<i>adjective</i><br/>Very unusual or remarkable.",
    ),
    (
        "forward",
        "<i>adverb</i><br/>Toward the front, or toward the end.",
    ),
    ("gap", "<i>noun</i><br/>A space between two things."),
    (
        "hyphenation",
        "<i>noun</i><br/>Dividing a word between lines with a hyphen, at a break its patterns allow: <b>hy-phen-a-tion</b>.",
    ),
    (
        "incomprehensibility",
        "<i>noun</i><br/>Being impossible to understand.",
    ),
    (
        "inconvenience",
        "<i>verb</i><br/>To cause someone trouble or difficulty.",
    ),
    (
        "institutionalization",
        "<i>noun</i><br/>Making something an established custom or part of an organization.",
    ),
    (
        "internationalization",
        "<i>noun</i><br/>Making something suitable for many countries and languages; <i>i18n</i>.",
    ),
    (
        "justify",
        "<i>verb</i><br/>To space a line's words so it fills the line from margin to margin.",
    ),
    ("line", "<i>noun</i><br/>A row of words across a page."),
    (
        "move",
        "<i>verb</i><br/>To go from one place or position to another.",
    ),
    ("multiple", "<i>adjective</i><br/>More than one; many."),
    ("notwithstanding", "<i>preposition</i><br/>In spite of."),
    (
        "page",
        "<i>noun</i><br/>One side of a sheet of paper in a book, or a screen of text in an e-reader.",
    ),
    (
        "paginate",
        "<i>verb</i><br/>To divide text into pages, or to number them.",
    ),
    ("paragraph", PARAGRAPH),
    (
        "participant",
        "<i>noun</i><br/>Someone who takes part in something.",
    ),
    ("preserve", "<i>verb</i><br/>To keep something as it is."),
    (
        "punctuation",
        "<i>noun</i><br/>Marks like full stops and commas that divide writing and make it clear. AT&T writes an ampersand bare.",
    ),
    (
        "recommendation",
        "<i>noun</i><br/>A suggestion that something is good or should be done.",
    ),
    (
        "representative",
        "<i>noun</i><br/>Someone who acts or speaks for others.",
    ),
    (
        "responsibility",
        "<i>noun</i><br/>A duty to deal with something.",
    ),
    (
        "sentence",
        "<i>noun</i><br/>A group of words that expresses a statement, question or command.",
    ),
    (
        "text",
        // markup like some dictionaries': unclosed paragraphs, crossed tags
        concat!(
            "<div class=\"sense\"><span style=\"color:#888\">noun</span>",
            "<p>The words of something written or printed.",
            "<p>The main body of a book, <b>apart from <i>its notes</b> and index</i>.</div>",
        ),
    ),
    (
        "unquestionable",
        "<i>adjective</i><br/>Too certain to be doubted.",
    ),
    (
        "viewport",
        "<i>noun</i><br/>The part of a screen content is shown in.",
    ),
    (
        "word",
        "<i>noun</i><br/>A unit of language, written with a space on either side.",
    ),
    (
        "wrap",
        "<i>verb</i><br/>To carry text over onto the next line.",
    ),
];

/// Long enough to take several pages of the definition card.
const PARAGRAPH: &str = "<i>noun</i>\
    <ol>\
    <li>A distinct section of writing, usually about one idea, that begins on a new line.</li>\
    <li>A mark (¶) once used to show where a new section of text begins.</li>\
    <li>A short passage in a newspaper or magazine.</li>\
    <li>A numbered section of a legal document or contract, which may be cited by its number.</li>\
    <li>In typesetting, a block of text set with its own indent and spacing before and after it.</li>\
    </ol>\
    <p><i>verb</i></p>\
    <ol>\
    <li>To divide text into paragraphs.</li>\
    <li>To write about something in a short paragraph.</li>\
    </ol>\
    <p>Paragraphs are separated either by indenting their first line or by a blank line \
    between them; books usually indent, while the web usually leaves a gap. Long \
    paragraphs are harder to read on small screens, so many e-readers let the reader \
    choose the spacing between them and the indent of their first lines.</p>\
    <p>The word comes through Latin from the Greek <i>paragraphos</i>, a short stroke \
    written in the margin to mark a break in sense, from <i>para-</i>, beside, and \
    <i>graphein</i>, to write.</p>";

/// Spellings the stems don't reach, each with the headword it's a form of.
const HTML_SYNONYMS: &[(&str, &str)] = &[
    ("deliberately", "deliberate"),
    ("deterministically", "deterministic"),
    ("disproportionately", "disproportionate"),
    ("extraordinarily", "extraordinary"),
    ("justified", "justify"),
    ("uncharacteristically", "characteristic"),
    ("unquestionably", "unquestionable"),
];

/// Each word's pronunciation, then its meaning.
const PLAIN_ENTRIES: &[(&str, &str, &str)] = &[
    ("line", "laɪn", "A row of words across a page."),
    ("page", "peɪdʒ", "One side of a sheet of paper in a book."),
    (
        "paragraph",
        "ˈpærəɡrɑːf",
        "A distinct section of writing, beginning on a new line.",
    ),
    (
        "sentence",
        "ˈsɛntəns",
        "A group of words that expresses a complete thought.",
    ),
    ("text", "tɛkst", "The words of something written."),
    ("word", "wɜːd", "A unit of language."),
];

/// Writes the dictionaries into `directory`, each in its own folder, and
/// lists them.
pub fn generate(directory: &Path) -> Result<()> {
    println!("Generated dictionary fixtures:");

    let html: Vec<(&str, Vec<u8>)> = HTML_ENTRIES
        .iter()
        .map(|&(word, definition)| (word, definition.as_bytes().to_vec()))
        .collect();

    write(directory, &HTML, html, HTML_SYNONYMS)?;

    let plain = PLAIN_ENTRIES
        .iter()
        .map(|&(word, pronunciation, meaning)| {
            (
                word,
                [pronunciation.as_bytes(), b"\0", meaning.as_bytes()].concat(),
            )
        })
        .collect();

    write(directory, &PLAIN, plain, &[])
}

fn write(
    directory: &Path,
    fixture: &Fixture,
    mut entries: Vec<(&str, Vec<u8>)>,
    synonyms: &[(&str, &str)],
) -> Result<()> {
    let folder = directory.join(fixture.folder);

    std::fs::create_dir_all(&folder)?;

    entries.sort_by(|(left, _), (right, _)| stardict_order(left, right));

    let mut idx = Vec::new();
    let mut dict = Vec::new();

    for (word, data) in &entries {
        idx.extend_from_slice(word.as_bytes());
        idx.push(0);
        idx.extend_from_slice(&u32::try_from(dict.len())?.to_be_bytes());
        idx.extend_from_slice(&u32::try_from(data.len())?.to_be_bytes());

        dict.extend_from_slice(data);
    }

    let mut synonyms = synonyms.to_vec();
    synonyms.sort_by(|(left, _), (right, _)| stardict_order(left, right));

    let mut syn = Vec::new();

    for (synonym, headword) in &synonyms {
        let ordinal = entries
            .iter()
            .position(|(word, _)| word == headword)
            .ok_or_else(|| anyhow::anyhow!("{synonym} names a missing headword {headword}"))?;

        syn.extend_from_slice(synonym.as_bytes());
        syn.push(0);
        syn.extend_from_slice(&u32::try_from(ordinal)?.to_be_bytes());
    }

    let mut ifo = format!(
        "StarDict's dict ifo file\n\
         version=3.0.0\n\
         bookname={}\n\
         wordcount={}\n\
         idxfilesize={}\n\
         sametypesequence={}\n\
         description=Generated by cargo xtask fixtures for InkPaper's simulator.\n",
        fixture.name,
        entries.len(),
        idx.len(),
        fixture.same_type_sequence,
    );

    if !syn.is_empty() {
        ifo.push_str(&format!("synwordcount={}\n", synonyms.len()));
    }

    let mut files = vec![
        (format!("{}.ifo", fixture.stem), ifo.into_bytes()),
        (format!("{}.idx", fixture.stem), idx),
    ];

    if fixture.compressed {
        let name = format!("{}.dict.dz", fixture.stem);
        let compressed = dictzip(&dict, &format!("{}.dict", fixture.stem));

        files.push((name, compressed));
    } else {
        files.push((format!("{}.dict", fixture.stem), dict));
    }

    if !syn.is_empty() {
        files.push((format!("{}.syn", fixture.stem), syn));
    }

    for (name, bytes) in files {
        std::fs::write(folder.join(&name), &bytes)?;

        println!(
            "  {:<40} {:>8} bytes",
            format!("{}/{name}", fixture.folder),
            bytes.len()
        );
    }

    Ok(())
}

/// StarDict's index order: ASCII letters compared without case, then
/// byte by byte.
fn stardict_order(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_ascii_lowercase()
        .cmp(&right.to_ascii_lowercase())
        .then_with(|| left.cmp(right))
}

/// `data` compressed like dictzip(1): chunks deflated on their own with a
/// full flush, their sizes in the gzip header's `RA` field, so a reader can
/// inflate any chunk alone.
fn dictzip(data: &[u8], original_name: &str) -> Vec<u8> {
    let mut compressor = CompressorOxide::new(create_comp_flags_from_zip_params(9, -15, 0));
    let mut chunks = Vec::new();
    let mut sizes = Vec::new();

    for chunk in data.chunks(usize::from(DICTZIP_CHUNK_LEN)) {
        let mut out = vec![0; chunk.len() * 2 + 64];
        let (_, _, written) = compress(&mut compressor, chunk, &mut out, TDEFLFlush::Full);

        chunks.extend_from_slice(&out[..written]);
        sizes.push(written as u16);
    }

    // the stream's last block, after the last chunk
    let mut out = vec![0; 64];
    let (_, _, written) = compress(&mut compressor, &[], &mut out, TDEFLFlush::Finish);
    chunks.extend_from_slice(&out[..written]);

    let mut field = Vec::new();
    field.extend_from_slice(&1u16.to_le_bytes());
    field.extend_from_slice(&DICTZIP_CHUNK_LEN.to_le_bytes());
    field.extend_from_slice(&(sizes.len() as u16).to_le_bytes());

    for size in sizes {
        field.extend_from_slice(&size.to_le_bytes());
    }

    let mut extra = b"RA".to_vec();
    extra.extend_from_slice(&(field.len() as u16).to_le_bytes());
    extra.extend_from_slice(&field);

    // gzip's magic, deflate, the extra field and a name, no time, Unix
    let mut file = vec![0x1f, 0x8b, 8, 0x04 | 0x08, 0, 0, 0, 0, 2, 3];
    file.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    file.extend_from_slice(&extra);
    file.extend_from_slice(original_name.as_bytes());
    file.push(0);
    file.extend_from_slice(&chunks);
    file.extend_from_slice(&crc32(data).to_le_bytes());
    file.extend_from_slice(&(data.len() as u32).to_le_bytes());

    file
}

/// gzip's CRC-32.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;

    for &byte in data {
        crc ^= u32::from(byte);

        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }

    !crc
}
