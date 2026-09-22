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
    /// [`Workspace::repackage_mimetype`] — is journalled, so a fix applied after
    /// a checkpoint can be undone by [`Workspace::seek`]ing back to it, and
    /// redone by seeking forward again. The guarantee is the one issue #7 asks
    /// for: **after seeking back, the workspace serializes byte-identically to
    /// what it serialized at the checkpoint.** It holds because the writer's
    /// output depends only on the entries, their order, the dirty set and the
    /// packaging flag, and a seek restores all four.
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
