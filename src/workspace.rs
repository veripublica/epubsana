//! The in-memory EPUB the fixer edits — a fidelity-preserving OCF container.
//!
//! Deliberately its *own* reader/writer, not epubveri's (which is strict and
//! read-only) nor epublift's (which *transforms* — a fixer must preserve, not
//! modernize).
//!
//! **An entry a fix did not touch is never decoded and re-encoded.** The
//! original archive is retained and such entries are *raw-copied*: identical
//! compressed bytes, compression method, timestamps and order, directory
//! entries included. Only entries a fix actually rewrote are re-encoded, and
//! those keep the compression method the original used.
//!
//! What this does *not* claim is container byte-identity. The zip writer
//! derives each local header rather than copying it, so the version-needed
//! field and the general-purpose hint bits (deflate level, data-descriptor)
//! come out as the writer's own — measured at ~180 bytes of header per book,
//! with every byte of every entry's data preserved. Nothing semantic is lost:
//! the one flag that carries meaning, bit 11 (UTF-8 entry names), is re-derived
//! from the name itself. Preserving raw headers is not reachable through `zip`'s
//! public API.
//!
//! Nothing here normalizes the container — not even `mimetype`. If a book's
//! packaging violates OCF, that is a defect for epubveri to report and a fixer
//! to *propose*, never something the writer launders on the way out. (It used
//! to: re-emitting `mimetype` first and stored repaired the OCF packaging rules
//! — `PKG-006` and its neighbours — as a side effect of writing any output,
//! with no fix item, no proposal and no approval. That is a silent mutation,
//! which is exactly what this crate promises never to do. `repackage_mimetype`
//! is now the only way it can happen, and only a fix calls it.)

use std::collections::{BTreeSet, HashMap};
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
}

impl Workspace {
    /// Read an EPUB from its raw bytes, preserving entry order.
    ///
    /// Directory entries are not exposed as entries (they have no content), but
    /// they are retained in `original` and reappear untouched in
    /// [`Workspace::serialize`].
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
        Ok(Workspace {
            original: bytes.to_vec(),
            order,
            entries,
            dirty: BTreeSet::new(),
            repackage_mimetype: false,
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
        self.repackage_mimetype = true;
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
    ///
    /// This marks the entry dirty: it is the *only* way an entry stops being
    /// raw-copied on the way out.
    pub fn set_bytes(&mut self, name: &str, data: Vec<u8>) {
        if !self.entries.contains_key(name) {
            self.order.push(name.to_string());
        }
        self.entries.insert(name.to_string(), data);
        self.dirty.insert(name.to_string());
    }

    /// Entry names, in container order.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.order.iter()
    }

    /// Re-zip the container, preserving everything a fix did not rewrite.
    ///
    /// Walks the *original* archive in its own order and raw-copies each entry
    /// — still compressed, so its bytes, compression method, timestamps and
    /// directory entries survive exactly. Only [`Workspace::set_bytes`] entries
    /// are re-encoded, and those keep whatever method the original used. Entries
    /// added after load (which have no original to preserve) are deflated and
    /// appended.
    ///
    /// A book nothing touched therefore serializes with every entry's data
    /// bit-for-bit intact and its packaging — including a non-conforming
    /// `mimetype` — exactly as it arrived.
    pub fn serialize(&self) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();
        {
            let mut src = ZipArchive::new(Cursor::new(self.original.as_slice()))?;
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let mut seen: BTreeSet<String> = BTreeSet::new();

            // An approved fix.mimetype_packaging asked for OCF order: mimetype
            // leads, stored. Its bytes are copied verbatim — only where it sits
            // and how it is compressed change.
            let hoisted = self.repackage_mimetype && self.entries.contains_key("mimetype");
            if hoisted {
                let stored =
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
                zip.start_file("mimetype", stored)?;
                zip.write_all(&self.entries["mimetype"])?;
                seen.insert("mimetype".to_string());
            }

            for i in 0..src.len() {
                let f = src.by_index_raw(i)?;
                let name = f.name().to_string();
                if hoisted && name == "mimetype" {
                    continue; // already written, in its rightful place
                }
                if !f.is_dir() {
                    seen.insert(name.clone());
                }
                if !self.dirty.contains(&name) {
                    // Untouched (or a directory entry): copy it through as-is.
                    zip.raw_copy_file(f)?;
                    continue;
                }
                // Rewritten by a fix: re-encode, but keep the original's method
                // so a Stored entry does not silently become Deflated.
                let method = f.compression();
                drop(f);
                zip.start_file(
                    &name,
                    SimpleFileOptions::default().compression_method(method),
                )?;
                zip.write_all(&self.entries[&name])?;
            }

            // Entries a fix added that the original never had.
            for name in &self.order {
                if seen.contains(name) {
                    continue;
                }
                let opts =
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
                zip.start_file(name, opts)?;
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

#[cfg(test)]
mod tests {
    use super::*;

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

    fn describe(bytes: &[u8]) -> Vec<(String, CompressionMethod, u64, u64)> {
        let mut z = ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
        (0..z.len())
            .map(|i| {
                let f = z.by_index_raw(i).unwrap();
                (
                    f.name().to_string(),
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
}
