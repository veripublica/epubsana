//! The in-memory EPUB the fixer edits — a fidelity-preserving OCF container.
//!
//! Deliberately its *own* reader/writer, not epubveri's (which is strict and
//! read-only) nor epublift's (which *transforms* — a fixer must preserve, not
//! modernize). Untouched entries round-trip byte-for-byte; only the files a
//! fix actually rewrites change, and the `mimetype` entry is always re-emitted
//! first and stored (uncompressed), as OCF requires.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Errors from loading, serializing, or re-validating a [`Workspace`].
#[derive(Debug)]
pub enum Error {
    Zip(zip::result::ZipError),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Zip(e) => write!(f, "zip error: {e}"),
            Error::Io(e) => write!(f, "io error: {e}"),
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

/// A mutable EPUB container: entry names in original order + their bytes.
pub struct Workspace {
    order: Vec<String>,
    entries: HashMap<String, Vec<u8>>,
}

impl Workspace {
    /// Read an EPUB from its raw bytes, preserving entry order.
    pub fn load(bytes: &[u8]) -> Result<Workspace, Error> {
        let mut zip = ZipArchive::new(Cursor::new(bytes.to_vec()))?;
        let mut order = Vec::new();
        let mut entries = HashMap::new();
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            let name = f.name().to_string();
            let mut data = Vec::new();
            f.read_to_end(&mut data)?;
            order.push(name.clone());
            entries.insert(name, data);
        }
        Ok(Workspace { order, entries })
    }

    /// A container entry decoded as UTF-8 (lossy), or `None` if absent.
    pub fn get_text(&self, name: &str) -> Option<String> {
        self.entries
            .get(name)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }

    /// Replace (or add) a text entry.
    pub fn set_text(&mut self, name: &str, text: String) {
        self.set_bytes(name, text.into_bytes());
    }

    /// Replace (or add) a raw entry, keeping original position if it existed.
    pub fn set_bytes(&mut self, name: &str, data: Vec<u8>) {
        if !self.entries.contains_key(name) {
            self.order.push(name.to_string());
        }
        self.entries.insert(name.to_string(), data);
    }

    /// Entry names, in container order.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.order.iter()
    }

    /// Re-zip the container. `mimetype` is emitted first and stored; every
    /// other entry is deflated, preserving order.
    pub fn serialize(&self) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            if let Some(mt) = self.entries.get("mimetype") {
                zip.start_file("mimetype", stored)?;
                zip.write_all(mt)?;
            }
            for name in &self.order {
                if name == "mimetype" {
                    continue;
                }
                zip.start_file(name, deflated)?;
                zip.write_all(&self.entries[name])?;
            }
            zip.finish()?;
        }
        Ok(buf)
    }

    /// Run epubveri against the current container state.
    pub fn detect(&self) -> Result<epubveri::report::Report, Error> {
        Ok(epubveri::validate_bytes(self.serialize()?))
    }
}
