//! The in-memory EPUB the fixer edits — a fidelity-preserving OCF container.
//!
//! Deliberately its *own* reader/writer, not epubveri's (which is strict and
//! read-only) nor epublift's (which *transforms* — a fixer must preserve, not
//! modernize).
//!
//! **An entry a fix did not touch goes out as the bytes it came in as.** The
//! original archive is retained, and the writer (`zipwrite`, our own
//! since 0.36.0) copies such an entry's local header, data and data descriptor
//! verbatim, and its central-directory record verbatim but for the offset of
//! the local header. Entry order is kept in the file and in the central
//! directory, each its own, and so is the archive comment. Only entries a fix
//! actually rewrote are re-encoded; those keep the original's compression
//! method and header fields, extra fields included, and only CRC, sizes and
//! the data-descriptor flag follow the new data.
//!
//! What is *not* kept: bytes outside every entry (before the first local
//! header, between entries, after the end record), and a ZIP64 field on a
//! rewritten entry, whose sizes the 32-bit fields then state.
//!
//! Before 0.36.0, `zip`'s writer derived both headers, so every written book
//! lost its local-header extra fields and an extra field on `mimetype`
//! (`PKG-005`) was cleared without a fix. That is why [`Workspace::detect`]
//! reads the **original** bytes while nothing has been changed: from 0.22.0
//! to 0.35.0, validating the writer's output instead would have hidden
//! `PKG-005` from every report. It still does, for the bytes outside every
//! entry. `fix.mimetype_packaging` is the repair for `PKG-005`; when another
//! fix writes the book and that one is declined, the field stays.
//!
//! Otherwise nothing here normalizes the container — not even `mimetype`. If a
//! book's packaging violates OCF, that is a defect for epubveri to report and a
//! fixer to *propose*, never something the writer launders on the way out. (It used
//! to: re-emitting `mimetype` first and stored repaired the OCF packaging rules
//! — `PKG-006` and its neighbours — as a side effect of writing any output,
//! with no fix item, no proposal and no approval. That is a silent mutation,
//! which is exactly what this crate promises never to do. `repackage_mimetype`
//! is now the only way it can happen, and only a fix calls it.)

use std::collections::{BTreeSet, HashMap};
use std::io::{Cursor, Read};

use flate2::read::DeflateDecoder;

use zip::{CompressionMethod, ZipArchive};

use crate::zipwrite::{Writer, layout};

/// Errors from loading, serializing, or re-validating a [`Workspace`].
#[derive(Debug)]
pub enum Error {
    Zip(zip::result::ZipError),
    Io(std::io::Error),
    /// Re-planning after a revert did not reproduce the plan the caller was
    /// shown. [`crate::repair`] stops with nothing applied rather than apply a
    /// plan nobody approved.
    Nondeterministic(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Zip(e) => write!(f, "zip error: {e}"),
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Nondeterministic(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Error::Zip(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// The most one container entry may inflate to. [`Workspace::load`] holds every
/// entry in memory, before epubveri has seen the book, so without a cap a 1 MB
/// zip of zeros cost 1.1 GB. The same value as epubveri's own per-entry cap;
/// the largest entry on the 474-book test shelf is 21.8 MB.
pub const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// The most a whole container may inflate to, summed over its entries. The
/// largest book on the test shelf inflates to 86.5 MB.
pub const MAX_BOOK_BYTES: u64 = 256 * 1024 * 1024;

/// A book past either cap is refused whole, never loaded in part: a fixer that
/// scans every entry for a reference would read a skipped one as "nothing
/// refers to it", which is the unsafe answer. Reported as an `Io` error of kind
/// `InvalidData`, so no new `Error` variant is needed.
fn too_large(msg: String) -> Error {
    Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

fn size(bytes: u64) -> String {
    if bytes.is_multiple_of(1 << 20) {
        format!("{} MiB", bytes >> 20)
    } else {
        format!("{bytes}-byte")
    }
}

/// Whether an entry is read and written as a stylesheet: a `.css` name, as
/// epubsana's fixers have always told stylesheets apart. A `text/css` item
/// under another name is UTF-8 to us, as it was before styloria.
pub(crate) fn is_stylesheet(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 4 && b[b.len() - 4..].eq_ignore_ascii_case(b".css")
}

/// A mutable EPUB container: entry names in original order + their bytes.
pub struct Workspace {
    /// The bytes we were loaded from. Kept so untouched entries can be copied
    /// out still-compressed rather than re-encoded — this is what makes the
    /// preservation guarantee real rather than aspirational.
    original: Vec<u8>,
    order: Vec<String>,
    entries: HashMap<String, Vec<u8>>,
    /// Entries a fix rewrote (or added). Only these are re-encoded.
    dirty: BTreeSet<String>,
    /// Set by an *approved* fix (never by the writer) to put `mimetype` back
    /// where OCF wants it. See [`Workspace::repackage_mimetype`].
    repackage_mimetype: bool,
    /// The archive's directory entries (names ending in `/`), in order. They
    /// carry no content, so they are not entries here, but a fix can drop one.
    directories: Vec<String>,
    /// Directory entries an approved fix dropped; the writer skips them. See
    /// [`Workspace::drop_directory`].
    dropped_directories: BTreeSet<String>,
    /// Every mutation since load, oldest first, and how far along it we are.
    /// See [`Workspace::checkpoint`].
    journal: Vec<Record>,
    cursor: usize,
}

/// A position in a [`Workspace`]'s history, taken by [`Workspace::checkpoint`]
/// and returned to by [`Workspace::seek`].
///
/// Only meaningful for the workspace that issued it. Writing after seeking
/// backwards discards every later position, exactly as typing after an undo
/// does in an editor, so a checkpoint from that discarded branch is refused
/// rather than silently reinterpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Checkpoint(usize);

/// One mutation, held so it can be undone and redone.
///
/// Each record keeps **the other version** of what it touched — the prior
/// state while the mutation is applied, the mutated state while it is undone —
/// and moving across it in either direction is one swap. Nothing is cloned:
/// the bytes a write replaced move into the journal instead of being dropped.
#[derive(Debug)]
enum Record {
    Entry {
        name: String,
        /// `None` means the entry does not exist on this side of the record:
        /// before the write that added it, or after undoing that write.
        other: Option<Vec<u8>>,
        other_dirty: bool,
    },
    Repackage {
        other: bool,
    },
    /// A directory entry dropped (or, while undone, restored).
    DropDirectory {
        name: String,
    },
}

/// Entry `index`'s contents, at most `limit` bytes of them, read the way
/// epubveri reads them. Stored and deflated entries are inflated here from
/// their raw bytes: zip 9 bounds every read by the size the header declares
/// (zip-rs/zip2#1022) and checks the CRC, so a book epubcheck and epubveri
/// accept — one entry longer than its header says, or with a wrong CRC — would
/// fail to load. Anything else (another method, an encrypted entry) goes
/// through zip's own reader, which refuses it as before.
fn inflate(
    zip: &mut ZipArchive<Cursor<Vec<u8>>>,
    index: usize,
    limit: u64,
) -> Result<Vec<u8>, Error> {
    let mut data = Vec::new();
    let raw = zip.by_index_raw(index)?;
    if !raw.encrypted() {
        match raw.compression() {
            CompressionMethod::Stored => {
                raw.take(limit).read_to_end(&mut data)?;
                return Ok(data);
            }
            CompressionMethod::Deflated => {
                DeflateDecoder::new(raw)
                    .take(limit)
                    .read_to_end(&mut data)?;
                return Ok(data);
            }
            _ => {}
        }
    }
    drop(raw);
    zip.by_index(index)?.take(limit).read_to_end(&mut data)?;
    Ok(data)
}

impl Workspace {
    /// Read an EPUB from its raw bytes, preserving entry order.
    ///
    /// Directory entries are not exposed as entries (they have no content), but
    /// they are retained in `original` and reappear untouched in
    /// [`Workspace::serialize`].
    pub fn load(bytes: &[u8]) -> Result<Workspace, Error> {
        Self::load_capped(bytes, MAX_ENTRY_BYTES, MAX_BOOK_BYTES)
    }

    fn load_capped(bytes: &[u8], max_entry: u64, max_book: u64) -> Result<Workspace, Error> {
        let mut zip = ZipArchive::new(Cursor::new(bytes.to_vec()))?;
        let mut order = Vec::new();
        let mut entries = HashMap::new();
        let mut total: u64 = 0;
        let mut directories = Vec::new();
        for i in 0..zip.len() {
            let f = zip.by_index_raw(i)?;
            if f.is_dir() {
                directories.push(f.name()?.into_owned());
                continue;
            }
            let name = f.name()?.into_owned();
            drop(f);
            // Read one byte past the cap, so an entry whose header understates
            // its size is still caught by what it actually inflates to.
            let data = inflate(&mut zip, i, max_entry.saturating_add(1))?;
            if data.len() as u64 > max_entry {
                return Err(too_large(format!(
                    "entry {name} inflates past the {} limit",
                    size(max_entry)
                )));
            }
            total += data.len() as u64;
            if total > max_book {
                return Err(too_large(format!(
                    "the book inflates past the {} limit",
                    size(max_book)
                )));
            }
            order.push(name.clone());
            entries.insert(name, data);
        }
        Ok(Workspace {
            original: bytes.to_vec(),
            order,
            entries,
            dirty: BTreeSet::new(),
            repackage_mimetype: false,
            directories,
            dropped_directories: BTreeSet::new(),
            journal: Vec::new(),
            cursor: 0,
        })
    }

    /// Emit the `mimetype` entry first and stored, as OCF requires — the one
    /// packaging change epubsana can make, and only when a fix has proposed it
    /// and the caller approved (`fix.mimetype_packaging`, `PKG-006`).
    ///
    /// This is deliberately a mutator rather than writer behaviour: the writer
    /// preserving packaging is what makes "no mutation without an approved fix"
    /// true, so the *only* way packaging changes is for someone to ask here.
    /// Content is untouched — `mimetype`'s own bytes included.
    pub fn repackage_mimetype(&mut self) {
        let prior = std::mem::replace(&mut self.repackage_mimetype, true);
        self.record(Record::Repackage { other: prior });
    }

    /// Leave the directory entry `name` out of the written archive — the
    /// repair for `PKG-014`, an empty directory. A directory entry carries no
    /// content, so this removes nothing a reader can see; it is still a change
    /// to the packaging, so like [`Workspace::repackage_mimetype`] it is a
    /// mutator a fix calls, never something the writer decides.
    ///
    /// # Panics
    ///
    /// If `name` is not one of [`Workspace::directories`]. A fix asking to drop
    /// something else is a bug, and guessing what it meant is not an option.
    pub fn drop_directory(&mut self, name: &str) {
        assert!(
            self.directories.iter().any(|d| d == name),
            "{name} is not a directory entry of this archive"
        );
        if self.dropped_directories.insert(name.to_string()) {
            self.record(Record::DropDirectory {
                name: name.to_string(),
            });
        }
    }

    /// The archive's directory entries, in archive order — including any a fix
    /// has dropped.
    pub fn directories(&self) -> impl Iterator<Item = &String> {
        self.directories.iter()
    }

    /// A container entry as text, or `None` if absent.
    ///
    /// A stylesheet (a `.css` name, ASCII case-insensitive) is decoded as CSS
    /// Syntax 3 says and as epubveri decodes it (`styloria::decode_stylesheet`
    /// with no protocol or referring-document label: a BOM, then a byte-exact
    /// `@charset`, then UTF-8), without its BOM, so a span from parsing it is
    /// a span in the text epubveri parsed. Everything else, a stylesheet in an
    /// encoding styloria does not decode, and one labelled with the
    /// `replacement` encoding is UTF-8 (lossy): styloria decodes `replacement`
    /// to one U+FFFD as the Encoding Standard says, but epubcheck reads such a
    /// sheet as UTF-8, and so does epubveri.
    pub fn get_text(&self, name: &str) -> Option<String> {
        let bytes = self.entries.get(name)?;
        if is_stylesheet(name)
            && let Ok(d) = styloria::decode_stylesheet(bytes, None, None)
            && d.encoding.name() != "replacement"
        {
            return Some(d.text.into_owned());
        }
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    /// Whether [`Workspace::set_text`] can write this entry back without
    /// damage: always for an absent entry or one that is not a stylesheet; for
    /// a stylesheet, only when every byte decoded. A fixer that edits a
    /// stylesheet declines when this is false: writing it back would turn the
    /// bytes that did not decode into U+FFFD.
    pub fn text_round_trips(&self, name: &str) -> bool {
        match self.entries.get(name) {
            Some(bytes) if is_stylesheet(name) => {
                styloria::decode_stylesheet(bytes, None, None).is_ok_and(|d| d.lossless)
            }
            _ => true,
        }
    }

    /// Replace (or add) a text entry.
    ///
    /// A stylesheet is written in the encoding it was read in, with its BOM if
    /// it had one, so every byte the edit did not touch is unchanged. A new
    /// one is UTF-8 without a BOM.
    ///
    /// # Panics
    ///
    /// If `name` is a stylesheet that does not round-trip
    /// ([`Workspace::text_round_trips`]), or `text` holds a character its
    /// encoding cannot hold. Fixers check the first before proposing and only
    /// insert ASCII; writing U+FFFD over a book's bytes is not a fallback.
    pub fn set_text(&mut self, name: &str, text: String) {
        let bytes = match self.entries.get(name) {
            Some(old) if is_stylesheet(name) => {
                let d = styloria::decode_stylesheet(old, None, None)
                    .ok()
                    .filter(|d| d.lossless)
                    .unwrap_or_else(|| panic!("{name} does not decode losslessly"));
                styloria::encode_stylesheet(&text, d.encoding, d.had_bom)
                    .unwrap_or_else(|e| panic!("{name} cannot be re-encoded: {e:?}"))
            }
            _ => text.into_bytes(),
        };
        self.set_bytes(name, bytes);
    }

    /// Replace (or add) a raw entry, keeping original position if it existed.
    ///
    /// This marks the entry dirty: it is the *only* way an entry stops being
    /// raw-copied on the way out.
    pub fn set_bytes(&mut self, name: &str, data: Vec<u8>) {
        let prior = self.entries.insert(name.to_string(), data);
        if prior.is_none() {
            self.order.push(name.to_string());
        }
        let prior_dirty = !self.dirty.insert(name.to_string());
        self.record(Record::Entry {
            name: name.to_string(),
            other: prior,
            other_dirty: prior_dirty,
        });
    }

    /// The current position in this workspace's history.
    ///
    /// Every mutation — [`Workspace::set_bytes`], [`Workspace::set_text`],
    /// [`Workspace::repackage_mimetype`], [`Workspace::drop_directory`] — is
    /// journalled, so a fix applied after
    /// a checkpoint can be undone by [`Workspace::seek`]ing back to it, and
    /// redone by seeking forward again. The guarantee is the one issue #7 asks
    /// for: **after seeking back, the workspace serializes byte-identically to
    /// what it serialized at the checkpoint.** It holds because the writer's
    /// output depends only on the entries, their order, the dirty set, the
    /// packaging flag and the dropped directories, and a seek restores all five.
    ///
    /// Journalling is unconditional and costs no copy: the bytes a write
    /// replaces are moved into the journal rather than dropped. What it does
    /// cost is that superseded versions stay alive until the workspace does.
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.cursor)
    }

    /// Move the workspace to `to`, undoing or redoing every mutation between
    /// here and there.
    ///
    /// # Panics
    ///
    /// If `to` lies beyond the end of the journal — a checkpoint from another
    /// workspace, or from a branch discarded by writing after a backward seek.
    /// That is a caller bug, and guessing which state was meant is not an
    /// option for a tool that promises never to mutate without approval.
    pub fn seek(&mut self, to: Checkpoint) {
        assert!(
            to.0 <= self.journal.len(),
            "checkpoint {} is not in this workspace's history (length {})",
            to.0,
            self.journal.len()
        );
        while self.cursor > to.0 {
            self.cursor -= 1;
            self.swap(self.cursor);
        }
        while self.cursor < to.0 {
            self.swap(self.cursor);
            self.cursor += 1;
        }
    }

    /// Append a mutation that has just been made, discarding any redo branch.
    fn record(&mut self, r: Record) {
        self.journal.truncate(self.cursor);
        self.journal.push(r);
        self.cursor += 1;
    }

    /// Cross record `i` in whichever direction it has not yet been crossed.
    /// The operation is its own inverse, which is what lets one function serve
    /// both undo and redo.
    fn swap(&mut self, i: usize) {
        match &mut self.journal[i] {
            Record::Repackage { other } => {
                std::mem::swap(other, &mut self.repackage_mimetype);
            }
            Record::DropDirectory { name } => {
                // Its own inverse: a record exists only for a real change.
                if !self.dropped_directories.remove(name.as_str()) {
                    self.dropped_directories.insert(name.clone());
                }
            }
            Record::Entry {
                name,
                other,
                other_dirty,
            } => {
                let incoming = other.take();
                let arrives = incoming.is_some();
                let current = match incoming {
                    Some(data) => self.entries.insert(name.clone(), data),
                    None => self.entries.remove(name),
                };
                // An entry appears or disappears only by the write that added
                // it, and records are crossed strictly in reverse on the way
                // back, so the name is always last in `order` when it goes.
                match (current.is_some(), arrives) {
                    (false, false) => unreachable!("a journalled entry exists on one side"),
                    (true, false) => {
                        let last = self.order.pop();
                        debug_assert_eq!(last.as_deref(), Some(name.as_str()));
                    }
                    (false, true) => self.order.push(name.clone()),
                    (true, true) => {}
                }
                *other = current;
                let current_dirty = self.dirty.contains(name.as_str());
                if *other_dirty {
                    self.dirty.insert(name.clone());
                } else {
                    self.dirty.remove(name.as_str());
                }
                *other_dirty = current_dirty;
            }
        }
    }

    /// Entry names, in container order.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.order.iter()
    }

    /// Re-zip the container, preserving everything a fix did not rewrite.
    ///
    /// Walks the *original* archive and copies each entry verbatim, headers
    /// included, in its file order and its central-directory order. Only
    /// [`Workspace::set_bytes`] entries are re-encoded, and those keep the
    /// original's method and header fields. Entries added after load (which
    /// have no original to preserve) are deflated and appended.
    ///
    /// A book nothing touched therefore serializes byte-identically, apart from
    /// bytes outside every entry (see the module docs), which is why
    /// [`Workspace::detect`] still reads the original while the workspace is
    /// untouched.
    pub fn serialize(&self) -> Result<Vec<u8>, Error> {
        let src = self.original.as_slice();
        let comment = ZipArchive::new(Cursor::new(src))?.comment().to_vec();
        let mut zip = Writer::new(src);
        let mut seen: BTreeSet<String> = BTreeSet::new();

        // An approved fix.mimetype_packaging asked for OCF order: mimetype
        // leads, stored, with no local extra field. Its bytes are written
        // unchanged; only where it sits and how it is packaged change.
        let hoisted = self.repackage_mimetype && self.entries.contains_key("mimetype");
        for raw in layout(src)? {
            if raw.is_dir {
                if !self.dropped_directories.contains(&raw.name) {
                    zip.copy(&raw); // unless an approved fix dropped it
                }
                continue;
            }
            seen.insert(raw.name.clone());
            if hoisted && raw.name == "mimetype" {
                zip.rewrite(&raw, &self.entries["mimetype"], true)?;
            } else if self.dirty.contains(&raw.name) {
                // Rewritten by a fix: re-encoded with the original's method, so
                // a Stored entry does not silently become Deflated.
                zip.rewrite(&raw, &self.entries[&raw.name], false)?;
            } else {
                zip.copy(&raw);
            }
        }

        // Entries a fix added that the original never had.
        for name in &self.order {
            if !seen.contains(name) {
                zip.add(name, &self.entries[name])?;
            }
        }
        zip.finish(&comment)
    }

    /// Run epubveri against the current container state.
    ///
    /// **While nothing has been changed, that state is the original bytes**, and
    /// they are what is validated. Until 0.36.0 the writer dropped local-header
    /// extra fields, so validating its output instead hid `PKG-005` from every
    /// "before" report: three shelf books whose only error it was were reported
    /// valid, with exit status 0, while the file on disk was not. It still
    /// leaves out bytes outside every entry (see the module docs).
    /// Once any change is made, the bytes that would be written are the state,
    /// and those are validated.
    pub fn detect(&self) -> Result<epubveri::report::Report, Error> {
        if self.is_untouched() {
            return Ok(epubveri::validate_bytes(self.original.clone()));
        }
        Ok(epubveri::validate_bytes(self.serialize()?))
    }

    /// No entry rewritten and no packaging change requested: what
    /// [`Workspace::serialize`] would write is the book as it arrived, apart from
    /// bytes outside every entry. A [`Workspace::seek`] back to the start
    /// restores this, since the journal restores the dirty set and the flag.
    fn is_untouched(&self) -> bool {
        self.dirty.is_empty() && !self.repackage_mimetype && self.dropped_directories.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    /// A container that deliberately breaks OCF: `mimetype` is neither first nor
    /// stored. It also carries a directory entry and a Stored non-mimetype entry
    /// — the three things the old writer destroyed.
    fn awkward_epub() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.add_directory("META-INF/", stored).unwrap();
            zip.start_file("META-INF/container.xml", stored).unwrap();
            zip.write_all(b"<container/>").unwrap();
            // mimetype: neither first nor stored — what PKG-006 reports, and
            // then some. The old writer silently repaired both.
            zip.start_file("mimetype", deflated).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            zip.start_file("text.html", deflated).unwrap();
            zip.write_all(b"<html><body>hello hello hello</body></html>")
                .unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    /// A book that is valid in every way the writer can see, whose `mimetype`
    /// leads, stored, with an extra field in its local header — `PKG-005`, and
    /// the shape of three shelf books (a 36-byte field there).
    fn mimetype_with_extra_field() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let mut first = zip::write::FullFileOptions::default()
                .compression_method(CompressionMethod::Stored);
            first
                .add_extra_field(0x5455, [1u8, 0, 0, 0, 0], false)
                .unwrap();
            zip.start_file("mimetype", first).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file("META-INF/container.xml", stored).unwrap();
            zip.write_all(b"<container/>").unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    /// A container holding exactly these entries, as bytes.
    fn with_entries(files: &[(&str, &[u8])]) -> Workspace {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file("mimetype", stored).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            for (name, body) in files {
                zip.start_file(*name, stored).unwrap();
                zip.write_all(body).unwrap();
            }
            zip.finish().unwrap();
        }
        Workspace::load(&buf).unwrap()
    }

    /// A Latin-1 stylesheet is read as Latin-1 and written back as Latin-1:
    /// before styloria, `get_text` made the `é` U+FFFD and `set_text` wrote
    /// the replacement character over it as UTF-8.
    #[test]
    fn a_stylesheet_is_written_back_in_its_own_encoding() {
        let css = b"@charset \"iso-8859-1\";\np::before { content: \"\xe9\" }\nq { a: b }";
        let mut ws = with_entries(&[("s.CSS", css)]);
        assert!(ws.text_round_trips("s.CSS"));
        let text = ws.get_text("s.CSS").unwrap();
        assert!(text.contains("\"\u{e9}\""));
        ws.set_text("s.CSS", text.replace("\nq { a: b }", ""));
        let mut want = css.to_vec();
        want.truncate(css.len() - b"\nq { a: b }".len());
        assert_eq!(ws.entries["s.CSS"], want);
    }

    #[test]
    fn a_stylesheet_keeps_its_byte_order_mark() {
        let mut ws = with_entries(&[("s.css", b"\xef\xbb\xbfp { a: b }")]);
        let text = ws.get_text("s.css").unwrap();
        assert_eq!(text, "p { a: b }", "the BOM is not text");
        ws.set_text("s.css", "p { a: c }".to_string());
        assert_eq!(ws.entries["s.css"], b"\xef\xbb\xbfp { a: c }");
    }

    /// `iso-2022-kr` is a `replacement` label: styloria decodes the sheet to
    /// one U+FFFD, while epubcheck and epubveri read it as UTF-8. A span
    /// from parsing it must be a span in the text epubveri parsed.
    #[test]
    fn a_replacement_labelled_stylesheet_is_read_as_utf8() {
        let css = b"@charset \"iso-2022-kr\";\np { a: b }";
        let ws = with_entries(&[("s.css", css)]);
        assert_eq!(ws.get_text("s.css").unwrap(), String::from_utf8_lossy(css));
        assert!(!ws.text_round_trips("s.css"));
    }

    /// A stylesheet with a byte that does not decode cannot be written back
    /// without U+FFFD, so writing it is a fixer's bug, not a fallback.
    #[test]
    #[should_panic(expected = "does not decode losslessly")]
    fn a_lossy_stylesheet_is_never_written() {
        let mut ws = with_entries(&[("s.css", b"p { content: \"\xff\" }")]);
        assert!(!ws.text_round_trips("s.css"));
        ws.set_text("s.css", "p {}".to_string());
    }

    /// Only stylesheets are decoded as CSS; a markup entry stays UTF-8 text,
    /// and a name ending in a multi-byte character is not sliced through it.
    #[test]
    fn only_a_css_name_is_a_stylesheet() {
        assert!(is_stylesheet("a/b.css") && is_stylesheet("B.Css"));
        assert!(!is_stylesheet("b.xhtml") && !is_stylesheet("css") && !is_stylesheet("aş.ş"));
        let ws = with_entries(&[("a.xhtml", b"@charset \"iso-8859-1\"; \xc3\xa9")]);
        assert!(ws.text_round_trips("a.xhtml"));
        assert_eq!(
            ws.get_text("a.xhtml").unwrap(),
            "@charset \"iso-8859-1\"; \u{e9}"
        );
    }

    fn has_pkg005(r: &epubveri::report::Report) -> bool {
        r.messages.iter().any(|m| m.id == "PKG-005")
    }

    /// **The bug this guards against:** `detect()` used to validate the
    /// writer's output, which until 0.36.0 dropped local-header extra fields, so an
    /// untouched book's `PKG-005` was invisible and a book whose only error it
    /// was got called valid.
    #[test]
    fn an_untouched_book_is_validated_as_it_arrived() {
        let bytes = mimetype_with_extra_field();
        assert!(
            has_pkg005(&epubveri::validate_bytes(bytes.clone())),
            "the fixture must carry PKG-005, or this proves nothing"
        );
        let ws = Workspace::load(&bytes).unwrap();
        assert!(has_pkg005(&ws.detect().unwrap()));
        // Since 0.36.0 the writer copies the field too, so its output carries
        // the finding as well.
        assert!(has_pkg005(&epubveri::validate_bytes(
            ws.serialize().unwrap()
        )));
    }

    /// Once anything changes, the bytes that would be written are validated:
    /// an edit elsewhere leaves `mimetype`'s extra field where it was, and
    /// only the approved repackaging removes it.
    #[test]
    fn a_changed_book_is_validated_as_it_would_be_written() {
        let mut ws = Workspace::load(&mimetype_with_extra_field()).unwrap();
        let start = ws.checkpoint();
        ws.set_text("META-INF/container.xml", "<container />".to_string());
        assert!(!ws.is_untouched());
        assert!(has_pkg005(&ws.detect().unwrap()));
        ws.seek(start);
        assert!(has_pkg005(&ws.detect().unwrap()));
        ws.repackage_mimetype();
        assert!(!has_pkg005(&ws.detect().unwrap()));
    }

    fn entry_names(bytes: &[u8]) -> Vec<String> {
        let z = ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
        z.file_names().map(|n| n.unwrap().into_owned()).collect()
    }

    /// A dropped directory entry is left out of the written archive, nothing
    /// else moves, and the journal can put it back.
    #[test]
    fn a_dropped_directory_is_left_out_and_comes_back_on_seek() {
        let mut ws = Workspace::load(&awkward_epub()).unwrap();
        assert_eq!(ws.directories().collect::<Vec<_>>(), ["META-INF/"]);
        let before = ws.serialize().unwrap();
        let start = ws.checkpoint();
        ws.drop_directory("META-INF/");
        assert!(!ws.is_untouched());
        let after = entry_names(&ws.serialize().unwrap());
        assert!(!after.contains(&"META-INF/".to_string()), "{after:?}");
        let mut expected = entry_names(&before);
        expected.retain(|n| n != "META-INF/");
        assert_eq!(after, expected, "only the directory entry may go");
        ws.seek(start);
        assert_eq!(ws.serialize().unwrap(), before);
        assert!(ws.is_untouched());
    }

    #[test]
    #[should_panic(expected = "is not a directory entry")]
    fn dropping_something_that_is_not_a_directory_is_a_bug() {
        let mut ws = Workspace::load(&awkward_epub()).unwrap();
        ws.drop_directory("text.html");
    }

    /// Overwrite the 4-byte field at `local` in `name`'s local header and at
    /// `central` in its central-directory header with `value`.
    fn patch_headers(
        mut epub: Vec<u8>,
        name: &str,
        local: usize,
        central: usize,
        value: u32,
    ) -> Vec<u8> {
        let find = |epub: &[u8], sig: &[u8], at: usize| {
            (0..epub.len() - at)
                .find(|&i| {
                    epub[i..].starts_with(sig) && epub[i + at..].starts_with(name.as_bytes())
                })
                .unwrap()
        };
        let l = find(&epub, b"PK\x03\x04", 30);
        epub[l + local..l + local + 4].copy_from_slice(&value.to_le_bytes());
        let c = find(&epub, b"PK\x01\x02", 46);
        epub[c + central..c + central + 4].copy_from_slice(&value.to_le_bytes());
        epub
    }

    /// zip 9 refuses an entry that inflates past the size its header declares
    /// (zip-rs/zip2#1022); epubcheck, epubveri and zip 8 read it whole.
    #[test]
    fn an_entry_longer_than_its_header_says_is_read_whole() {
        let orig = awkward_epub();
        let whole = Workspace::load(&orig).unwrap();
        for name in ["META-INF/container.xml", "text.html"] {
            let epub = patch_headers(orig.clone(), name, 22, 24, 4);
            let ws = Workspace::load(&epub).unwrap();
            assert_eq!(ws.entries.get(name), whole.entries.get(name), "{name}");
        }
    }

    /// epubcheck does not check CRCs, nor does epubveri since 0.29.0; zip 9
    /// does by default, and zip 8 did too, so such a book used not to load.
    #[test]
    fn an_entry_with_a_wrong_crc_still_loads() {
        let orig = awkward_epub();
        for name in ["META-INF/container.xml", "text.html"] {
            let epub = patch_headers(orig.clone(), name, 14, 16, 0xdead_beef);
            let ws = Workspace::load(&epub).unwrap();
            assert_eq!(
                ws.entries.get(name),
                Workspace::load(&orig).unwrap().entries.get(name)
            );
            // Untouched, so it goes out as it came in, wrong CRC and all.
            assert_eq!(
                ws.serialize().unwrap(),
                Workspace::load(&epub).unwrap().serialize().unwrap()
            );
        }
    }

    /// An encrypted entry is not read from its raw bytes, which would load
    /// ciphertext as the entry: zip refuses it, as it always did.
    #[test]
    fn an_encrypted_entry_is_not_read_raw() {
        // The flags and method fields, in one write: bit 0 (encrypted), Stored.
        let epub = patch_headers(awkward_epub(), "META-INF/container.xml", 6, 8, 1);
        assert!(Workspace::load(&epub).is_err());
    }

    /// The local and central extra-field lengths of `name` in `epub`.
    fn extra_lengths(epub: &[u8], name: &str) -> (u16, u16) {
        let at = |sig: &[u8], name_at: usize, len_at: usize| {
            let i = (0..epub.len() - name_at)
                .find(|&i| {
                    epub[i..].starts_with(sig) && epub[i + name_at..].starts_with(name.as_bytes())
                })
                .unwrap();
            u16::from_le_bytes([epub[i + len_at], epub[i + len_at + 1]])
        };
        (at(b"PK\x03\x04", 30, 28), at(b"PK\x01\x02", 46, 30))
    }

    /// An untouched entry keeps an Info-ZIP Unix field (0x7875) in both
    /// headers. zip 9.0's `raw_copy_file` refused one, which stopped a shelf
    /// book from being written; our writer copies the headers it was given.
    #[test]
    fn an_untouched_entry_keeps_a_unix_extra_field() {
        let mut epub = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut epub));
            let mut opts = zip::write::FullFileOptions::default()
                .compression_method(CompressionMethod::Deflated);
            opts.add_extra_field(0x5455, [1u8, 0, 0, 0, 0], false)
                .unwrap();
            zip.start_file("text.html", opts).unwrap();
            zip.write_all(b"<html/>").unwrap();
            zip.finish().unwrap();
        }
        // Re-tag the field 0x7875 in both headers.
        for (sig, name_at) in [(&b"PK\x03\x04"[..], 30), (&b"PK\x01\x02"[..], 46)] {
            let i = (0..epub.len())
                .find(|&i| epub[i..].starts_with(sig))
                .unwrap()
                + name_at
                + 9;
            assert_eq!(epub[i..i + 2], [0x55, 0x54]);
            epub[i..i + 2].copy_from_slice(&0x7875u16.to_le_bytes());
        }
        assert_eq!(extra_lengths(&epub, "text.html"), (9, 9));
        let out = Workspace::load(&epub).unwrap().serialize().unwrap();
        assert_eq!(out, epub);
    }

    /// An archive with what `zip`'s writer cannot copy: a local-only and a
    /// central-only extra field, an entry comment, unusual external attributes,
    /// a stored entry with version 2.0 (zip 9.1.3 rewrote it to 1.0), and an
    /// archive comment.
    fn header_rich_epub() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file("mimetype", stored).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            let mut local_only = zip::write::FullFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .unix_permissions(0o600);
            local_only
                .add_extra_field(0x5455, [1u8, 1, 2, 3, 4], false)
                .unwrap();
            zip.start_file("a.html", local_only).unwrap();
            zip.write_all(b"<html><body>a a a a</body></html>").unwrap();
            let mut central_only = zip::write::FullFileOptions::default()
                .compression_method(CompressionMethod::Stored)
                .with_file_comment("an entry comment");
            central_only
                .add_extra_field(0x6666, [9u8, 8, 7], true)
                .unwrap();
            zip.start_file("b.css", central_only).unwrap();
            zip.write_all(b"p { a: b }").unwrap();
            zip.set_comment("an archive comment").unwrap();
            zip.finish().unwrap();
        }
        // b.css's version fields to 2.0, in both headers.
        let at = |buf: &[u8], sig: &[u8], name_at: usize| {
            (0..buf.len())
                .find(|&i| buf[i..].starts_with(sig) && buf[i + name_at..].starts_with(b"b.css"))
                .unwrap()
        };
        let l = at(&buf, b"PK\x03\x04", 30);
        buf[l + 4] = 20;
        let c = at(&buf, b"PK\x01\x02", 46);
        buf[c + 4] = 20;
        buf[c + 6] = 20;
        buf
    }

    /// The writer's promise in its strongest form: an archive with no bytes
    /// outside its entries comes back byte-identical when nothing touched it.
    #[test]
    fn an_untouched_book_is_written_byte_identically() {
        for epub in [
            awkward_epub(),
            header_rich_epub(),
            mimetype_with_extra_field(),
        ] {
            let out = Workspace::load(&epub).unwrap().serialize().unwrap();
            assert_eq!(out, epub);
        }
    }

    /// A streamed archive puts sizes and CRC in a data descriptor after each
    /// entry's data (bit 3): untouched, the descriptor is copied with it; a
    /// rewritten entry states its sizes in its header and drops the flag.
    #[test]
    fn a_data_descriptor_is_copied_or_dropped_with_its_entry() {
        let mut epub = Vec::new();
        {
            let mut zip = ZipWriter::new_stream(&mut epub);
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file("mimetype", stored).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            for name in ["a.html", "b.html"] {
                zip.start_file(name, SimpleFileOptions::default()).unwrap();
                zip.write_all(b"<html><body>streamed</body></html>")
                    .unwrap();
            }
            zip.finish().unwrap();
        }
        let raws = layout(&epub).unwrap();
        assert!(raws.iter().any(|r| epub[r.local.start + 6] & 0x08 != 0));
        assert_eq!(Workspace::load(&epub).unwrap().serialize().unwrap(), epub);

        let mut ws = Workspace::load(&epub).unwrap();
        ws.set_text("a.html", "<html>new</html>".into());
        let out = ws.serialize().unwrap();
        let after = layout(&out).unwrap();
        let a = after.iter().find(|r| r.name == "a.html").unwrap();
        assert_eq!(out[a.local.start + 6] & 0x08, 0);
        assert_eq!(out[a.central.start + 8] & 0x08, 0);
        let b = |raws: &[crate::zipwrite::Raw], bytes: &[u8]| {
            let r = raws.iter().find(|r| r.name == "b.html").unwrap();
            bytes[r.local.clone()].to_vec()
        };
        assert_eq!(
            b(&after, &out),
            b(&raws, &epub),
            "b.html was not copied whole"
        );
        let mut z = ZipArchive::new(Cursor::new(out)).unwrap();
        for (name, want) in [
            ("a.html", &b"<html>new</html>"[..]),
            ("b.html", b"<html><body>streamed</body></html>"),
        ] {
            let mut got = Vec::new();
            z.by_name(name).unwrap().read_to_end(&mut got).unwrap();
            assert_eq!(got, want, "{name}");
        }
    }

    /// A streamed archive: every entry is followed by a data descriptor, 24
    /// bytes long when `large` makes the local headers carry a ZIP64 field.
    fn streamed_epub(large: bool) -> Vec<u8> {
        let mut epub = Vec::new();
        {
            let mut zip = ZipWriter::new_stream(&mut epub);
            let opts = SimpleFileOptions::default().large_file(large);
            for name in ["a.html", "b.html"] {
                zip.start_file(name, opts).unwrap();
                zip.write_all(b"<html><body>streamed</body></html>")
                    .unwrap();
            }
            zip.finish().unwrap();
        }
        epub
    }

    /// `epub` without the bytes `cut`, which lie inside one entry's local
    /// part, with every offset after them moved back.
    fn without(epub: &[u8], cut: std::ops::Range<usize>) -> Vec<u8> {
        let len = cut.len();
        let moved = |o: usize| if o >= cut.end { o - len } else { o };
        let mut out = epub.to_vec();
        out.drain(cut.clone());
        for r in layout(epub).unwrap() {
            let c = moved(r.central.start);
            let o = u32::from_le_bytes(out[c + 42..c + 46].try_into().unwrap()) as usize;
            out[c + 42..c + 46].copy_from_slice(&(moved(o) as u32).to_le_bytes());
        }
        let e = out.len() - 22;
        let cd = u32::from_le_bytes(out[e + 16..e + 20].try_into().unwrap()) as usize;
        out[e + 16..e + 20].copy_from_slice(&(moved(cd) as u32).to_le_bytes());
        out
    }

    /// The end of `name`'s data in `epub`, where its descriptor begins.
    fn data_end(epub: &[u8], name: &str) -> usize {
        let mut z = ZipArchive::new(Cursor::new(epub.to_vec())).unwrap();
        let f = z.by_name(name).unwrap();
        (f.data_start().unwrap() + f.compressed_size()) as usize
    }

    /// Every form a descriptor takes is copied whole and alone: with or
    /// without its signature, 16 or 24 bytes, or missing, where the next
    /// entry's header follows the data at once.
    #[test]
    fn every_form_of_data_descriptor_is_copied_whole_and_alone() {
        let plain = streamed_epub(false);
        let large = streamed_epub(true);
        let d = data_end(&large, "a.html");
        assert_eq!(&large[d..d + 4], b"PK\x07\x08");
        assert!(
            large[d + 24..].starts_with(b"PK\x03\x04"),
            "a 24-byte descriptor"
        );
        let d = data_end(&plain, "a.html");
        let unsigned = without(&plain, d..d + 4);
        let missing = without(&plain, d..d + 16);
        assert!(missing[d..].starts_with(b"PK\x03\x04"));
        for epub in [plain, large, unsigned, missing] {
            let out = Workspace::load(&epub).unwrap().serialize().unwrap();
            assert_eq!(out, epub);
            let mut ws = Workspace::load(&epub).unwrap();
            ws.set_text("b.html", "<html/>".into());
            let out = ws.serialize().unwrap();
            let pick = |bytes: &[u8]| {
                let r = layout(bytes).unwrap().into_iter().next().unwrap();
                bytes[r.local].to_vec()
            };
            assert_eq!(pick(&out), pick(&epub), "a.html took a neighbour's bytes");
            let mut got = Vec::new();
            ZipArchive::new(Cursor::new(out))
                .unwrap()
                .by_name("b.html")
                .unwrap()
                .read_to_end(&mut got)
                .unwrap();
            assert_eq!(got, b"<html/>");
        }
    }

    /// An entry whose stated size runs into the next entry's header (the
    /// shape of a "cloaked" archive, which `zip` loads) is still copied with
    /// all the data it claims: the descriptor search starts where the data
    /// ends, not where the next header does.
    #[test]
    fn an_entry_overlapping_the_next_is_copied_with_all_its_data() {
        let mut epub = streamed_epub(false);
        let a = layout(&epub).unwrap().remove(0);
        let c = a.central.start;
        let csize = u32::from_le_bytes(epub[c + 20..c + 24].try_into().unwrap()) + 20;
        epub[c + 20..c + 24].copy_from_slice(&csize.to_le_bytes());
        let claimed = a.local.start..data_end(&epub, "a.html");
        assert!(
            claimed.end > layout(&epub).unwrap()[1].local.start,
            "must overlap"
        );
        let out = Workspace::load(&epub).unwrap().serialize().unwrap();
        let copied = layout(&out).unwrap().remove(0).local;
        assert!(out[copied].starts_with(&epub[claimed]));
    }

    /// `zip` loads an entry that claims more bytes than the archive holds, as
    /// far as they go. There is no whole entry to copy, so nothing is written.
    #[test]
    fn an_entry_past_the_end_of_the_archive_is_not_written() {
        let epub = patch_headers(awkward_epub(), "text.html", 18, 20, 0x7FFF_FFFF);
        let ws = Workspace::load(&epub).unwrap();
        assert!(refused_write(ws.serialize()));
    }

    fn refused_write(r: Result<Vec<u8>, Error>) -> bool {
        matches!(r, Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::InvalidData)
    }

    /// The bytes of `epub` with its central directory's records reversed, so
    /// the file order and the central-directory order of its entries differ.
    fn with_central_reversed(epub: &[u8]) -> Vec<u8> {
        let raws = layout(epub).unwrap();
        let cd = raws.iter().map(|r| r.central.start).min().unwrap();
        let mut out = epub[..cd].to_vec();
        for r in raws.iter().rev() {
            out.extend_from_slice(&epub[r.central.clone()]);
        }
        let end = raws.iter().map(|r| r.central.end).max().unwrap();
        out.extend_from_slice(&epub[end..]);
        out
    }

    /// epubveri judges `PKG-006` on the file's leading bytes since 0.35.0, so
    /// the file order is packaging and is kept, apart from the central
    /// directory's.
    #[test]
    fn file_order_and_central_order_are_each_kept() {
        let epub = with_central_reversed(&header_rich_epub());
        assert_eq!(entry_names(&epub), ["b.css", "a.html", "mimetype"]);
        assert_eq!(Workspace::load(&epub).unwrap().serialize().unwrap(), epub);
        let mut ws = Workspace::load(&epub).unwrap();
        ws.set_text("a.html", "<html/>".into());
        let out = ws.serialize().unwrap();
        assert_eq!(entry_names(&out), ["b.css", "a.html", "mimetype"]);
        assert!(out.starts_with(b"PK\x03\x04") && out[30..].starts_with(b"mimetype"));
    }

    /// Bytes before the first entry are not part of any entry, and are not
    /// written; every entry still is, at its new offset.
    #[test]
    fn bytes_before_the_first_entry_are_not_written() {
        let epub = header_rich_epub();
        let mut prefixed = b"junk!".to_vec();
        prefixed.extend_from_slice(&epub);
        let ws = Workspace::load(&prefixed).unwrap();
        assert_eq!(ws.serialize().unwrap(), epub);
    }

    /// A rewritten entry keeps every header field but those its new data
    /// decides: time, extra fields, comment and attributes stay.
    #[test]
    fn a_rewritten_entry_keeps_its_header_fields() {
        let epub = header_rich_epub();
        let mut ws = Workspace::load(&epub).unwrap();
        ws.set_text("a.html", "<html>edited</html>".into());
        ws.set_text("b.css", "p { a: c }".into());
        let out = ws.serialize().unwrap();
        let (before, after) = (layout(&epub).unwrap(), layout(&out).unwrap());
        for name in ["a.html", "b.css"] {
            let pick = |raws: &[crate::zipwrite::Raw], bytes: &[u8]| {
                let r = raws.iter().find(|r| r.name == name).unwrap().clone();
                let (l, c) = (&bytes[r.local.clone()], &bytes[r.central.clone()]);
                let ln = u16::from_le_bytes([l[26], l[27]]) as usize;
                let lx = u16::from_le_bytes([l[28], l[29]]) as usize;
                // Local: version, flags, method, time, date; name and extra.
                let mut keep = l[4..14].to_vec();
                keep.extend_from_slice(&l[30..30 + ln + lx]);
                // Central: made by .. date, then attributes, name, extra, comment.
                keep.extend_from_slice(&c[4..16]);
                keep.extend_from_slice(&c[28..42]);
                keep.extend_from_slice(&c[46..]);
                keep
            };
            assert_eq!(pick(&after, &out), pick(&before, &epub), "{name}");
        }
        let mut z = ZipArchive::new(Cursor::new(out)).unwrap();
        assert_eq!(z.comment(), b"an archive comment");
        let mut got = String::new();
        z.by_name("b.css")
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "p { a: c }");
    }

    /// A ZIP64 field states sizes the writer now puts in the 32-bit fields, so
    /// a rewritten entry goes without it; an untouched one keeps it.
    #[test]
    fn a_rewritten_entry_drops_its_zip64_field() {
        let mut epub = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut epub));
            let large = SimpleFileOptions::default().large_file(true);
            for name in ["a.html", "b.html"] {
                zip.start_file(name, large).unwrap();
                zip.write_all(b"<html/>").unwrap();
            }
            zip.finish().unwrap();
        }
        let has_zip64 = |bytes: &[u8], name: &str| {
            let r = layout(bytes)
                .unwrap()
                .into_iter()
                .find(|r| r.name == name)
                .unwrap();
            let l = &bytes[r.local.clone()];
            let (ln, lx) = (
                u16::from_le_bytes([l[26], l[27]]) as usize,
                u16::from_le_bytes([l[28], l[29]]) as usize,
            );
            l[30 + ln..30 + ln + lx].starts_with(&[1, 0])
        };
        assert!(has_zip64(&epub, "a.html") && has_zip64(&epub, "b.html"));
        let mut ws = Workspace::load(&epub).unwrap();
        ws.set_text("a.html", "<html>x</html>".into());
        let out = ws.serialize().unwrap();
        assert!(!has_zip64(&out, "a.html"));
        assert!(has_zip64(&out, "b.html"));
        let mut got = String::new();
        ZipArchive::new(Cursor::new(out))
            .unwrap()
            .by_name("a.html")
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "<html>x</html>");
    }

    fn refused(r: Result<Workspace, Error>) -> bool {
        matches!(r, Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::InvalidData)
    }

    /// `awkward_epub`'s largest entry inflates to 43 bytes and the three sum
    /// to 75, so each cap is tested one byte either side of what it admits.
    #[test]
    fn a_book_past_either_size_cap_is_refused_whole() {
        let epub = awkward_epub();
        assert!(Workspace::load_capped(&epub, 43, u64::MAX).is_ok());
        assert!(refused(Workspace::load_capped(&epub, 42, u64::MAX)));
        assert!(Workspace::load_capped(&epub, u64::MAX, 75).is_ok());
        assert!(refused(Workspace::load_capped(&epub, u64::MAX, 74)));
    }

    fn describe(bytes: &[u8]) -> Vec<(String, CompressionMethod, u64, u64)> {
        let mut z = ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
        (0..z.len())
            .map(|i| {
                let f = z.by_index_raw(i).unwrap();
                (
                    f.name().unwrap().into_owned(),
                    f.compression(),
                    f.compressed_size(),
                    f.crc32() as u64,
                )
            })
            .collect()
    }

    #[test]
    fn untouched_entries_keep_their_exact_compressed_bytes() {
        let orig = awkward_epub();
        let out = Workspace::load(&orig).unwrap().serialize().unwrap();
        // Order, compression method, compressed size and CRC all survive — which
        // together mean no entry was decoded and re-encoded.
        assert_eq!(describe(&orig), describe(&out));
    }

    #[test]
    fn a_directory_entry_is_not_dropped() {
        let orig = awkward_epub();
        let out = Workspace::load(&orig).unwrap().serialize().unwrap();
        assert!(describe(&out).iter().any(|(n, ..)| n == "META-INF/"));
    }

    #[test]
    fn non_conforming_mimetype_is_preserved_not_laundered() {
        let orig = awkward_epub();
        let out = Workspace::load(&orig).unwrap().serialize().unwrap();
        let entries = describe(&out);
        // Still not first and still deflated: writing output must never repair
        // packaging behind the user's back (that is a fix's job, once approved).
        assert_ne!(entries[0].0, "mimetype");
        let mt = entries.iter().find(|(n, ..)| n == "mimetype").unwrap();
        assert_eq!(mt.1, CompressionMethod::Deflated);
    }

    #[test]
    fn a_rewritten_entry_keeps_the_original_compression_method() {
        let orig = awkward_epub();
        let mut ws = Workspace::load(&orig).unwrap();
        // container.xml was Stored; rewriting it must not silently deflate it.
        ws.set_text("META-INF/container.xml", "<container v=\"2\"/>".into());
        let out = ws.serialize().unwrap();
        let e = describe(&out);
        let c = e
            .iter()
            .find(|(n, ..)| n == "META-INF/container.xml")
            .unwrap();
        assert_eq!(c.1, CompressionMethod::Stored);
        // ...and the neighbours are still untouched.
        let a = describe(&orig);
        let pick = |v: &Vec<(String, CompressionMethod, u64, u64)>, n: &str| {
            v.iter().find(|(x, ..)| x == n).cloned().unwrap()
        };
        assert_eq!(pick(&a, "text.html"), pick(&e, "text.html"));
        assert_eq!(pick(&a, "mimetype"), pick(&e, "mimetype"));
    }

    #[test]
    fn repackage_mimetype_hoists_it_first_and_stored() {
        let orig = awkward_epub();
        let mut ws = Workspace::load(&orig).unwrap();
        ws.repackage_mimetype();
        let out = ws.serialize().unwrap();
        let e = describe(&out);
        assert_eq!(e[0].0, "mimetype");
        assert_eq!(e[0].1, CompressionMethod::Stored);
        // It appears once — hoisting must not leave the original copy behind.
        assert_eq!(e.iter().filter(|(n, ..)| n == "mimetype").count(), 1);
        assert_eq!(e.len(), describe(&orig).len());
    }

    #[test]
    fn repackaging_mimetype_changes_no_content() {
        let orig = awkward_epub();
        let mut ws = Workspace::load(&orig).unwrap();
        ws.repackage_mimetype();
        let out = ws.serialize().unwrap();
        // Every entry's bytes — mimetype's own included — survive verbatim.
        let mut a = ZipArchive::new(Cursor::new(orig.clone())).unwrap();
        let mut b = ZipArchive::new(Cursor::new(out)).unwrap();
        for name in ["mimetype", "META-INF/container.xml", "text.html"] {
            let mut x = Vec::new();
            let mut y = Vec::new();
            a.by_name(name).unwrap().read_to_end(&mut x).unwrap();
            b.by_name(name).unwrap().read_to_end(&mut y).unwrap();
            assert_eq!(x, y, "content of {name} changed");
        }
        // ...and the entries we did not hoist keep their exact compressed bytes.
        let (da, db) = (describe(&orig), describe(&b.into_inner().into_inner()));
        for name in ["META-INF/container.xml", "text.html", "META-INF/"] {
            let pick = |v: &Vec<(String, CompressionMethod, u64, u64)>| {
                v.iter().find(|(n, ..)| n == name).cloned().unwrap()
            };
            assert_eq!(pick(&da), pick(&db), "{name} was re-encoded");
        }
    }

    #[test]
    fn an_added_entry_is_appended_without_disturbing_the_rest() {
        let orig = awkward_epub();
        let mut ws = Workspace::load(&orig).unwrap();
        ws.set_text("new.html", "<html/>".into());
        let out = ws.serialize().unwrap();
        let (a, b) = (describe(&orig), describe(&out));
        assert_eq!(b.len(), a.len() + 1);
        assert_eq!(&b[..a.len()], &a[..]);
        assert_eq!(b.last().unwrap().0, "new.html");
    }

    /// The acceptance criterion of issue #7, stated as a test: after a
    /// speculative change is undone, the workspace serializes byte-identically
    /// to before it. Every mutator is exercised — a rewrite (which also flips
    /// an entry from raw-copied to re-encoded), an added entry (which also
    /// grows `order`) and the packaging flag — and so is writing one entry twice.
    #[test]
    fn seeking_back_serializes_byte_identically() {
        let mut ws = Workspace::load(&awkward_epub()).unwrap();
        let start = ws.checkpoint();
        let pristine = ws.serialize().unwrap();

        ws.set_text("text.html", "<html>one</html>".into());
        ws.set_text("META-INF/container.xml", "<container v=\"2\"/>".into());
        let middle = ws.checkpoint();
        let at_middle = ws.serialize().unwrap();

        ws.set_text("text.html", "<html>two</html>".into());
        ws.set_text("new.html", "<html/>".into());
        ws.repackage_mimetype();
        assert_ne!(ws.serialize().unwrap(), at_middle);

        ws.seek(middle);
        assert_eq!(ws.serialize().unwrap(), at_middle);
        assert_eq!(ws.get_text("text.html").unwrap(), "<html>one</html>");
        assert!(ws.get_text("new.html").is_none());
        assert!(!ws.names().any(|n| n == "new.html"));

        ws.seek(start);
        assert_eq!(ws.serialize().unwrap(), pristine);
    }

    /// Redo is what makes a bisection possible without re-planning: a fix's
    /// `apply` is `FnOnce`, so once undone it cannot be run again, and the
    /// journal has to be able to put its effect back by itself.
    #[test]
    fn seeking_forward_restores_what_was_undone() {
        let mut ws = Workspace::load(&awkward_epub()).unwrap();
        let start = ws.checkpoint();
        ws.set_text("text.html", "<html>edited</html>".into());
        ws.set_text("new.html", "<html/>".into());
        ws.repackage_mimetype();
        let end = ws.checkpoint();
        let at_end = ws.serialize().unwrap();

        ws.seek(start);
        ws.seek(end);
        assert_eq!(ws.serialize().unwrap(), at_end);
        assert_eq!(ws.names().last().unwrap(), "new.html");
    }

    /// Undoing a write must also undo the dirty mark, or the entry would be
    /// re-encoded on the way out: same content, different compressed bytes,
    /// and the preservation guarantee quietly broken by the undo itself.
    #[test]
    ///
    /// The fixture's entry carries its own timestamp on purpose. `awkward_epub`
    /// uses the writer's defaults, so a re-encoded copy of it comes out
    /// byte-identical to the raw one and this test passed with the dirty mark
    /// left in place — found by mutating the guard, not by reading the test.
    fn an_undone_rewrite_is_raw_copied_again() {
        let mut orig = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut orig));
            let stamped = SimpleFileOptions::default().last_modified_time(
                zip::DateTime::from_date_and_time(2011, 3, 14, 15, 9, 26).unwrap(),
            );
            zip.start_file("text.html", stamped).unwrap();
            zip.write_all(b"<html><body>hello</body></html>").unwrap();
            zip.finish().unwrap();
        }
        let mut ws = Workspace::load(&orig).unwrap();
        let start = ws.checkpoint();
        ws.set_text("text.html", "<html>x</html>".into());
        ws.seek(start);
        assert_eq!(
            ws.serialize().unwrap(),
            Workspace::load(&orig).unwrap().serialize().unwrap()
        );
    }

    /// Writing after a backward seek discards the redo branch, as an editor
    /// does. A checkpoint into that branch no longer names any state, and
    /// seeking to it is refused rather than guessed at.
    #[test]
    #[should_panic(expected = "not in this workspace's history")]
    fn a_checkpoint_on_a_discarded_branch_is_refused() {
        let mut ws = Workspace::load(&awkward_epub()).unwrap();
        let start = ws.checkpoint();
        ws.set_text("text.html", "<html>a</html>".into());
        ws.set_text("text.html", "<html>b</html>".into());
        let lost = ws.checkpoint();
        ws.seek(start);
        ws.set_text("text.html", "<html>c</html>".into());
        ws.seek(lost);
    }
}
