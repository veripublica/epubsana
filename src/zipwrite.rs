//! The container writer behind [`Workspace::serialize`](crate::Workspace::serialize).
//!
//! Our own, not `zip`'s. An entry a fix did not touch goes out as the bytes it
//! came in as: its local header, data and data descriptor verbatim, and its
//! central-directory record verbatim except for the one field that must change,
//! the offset of its local header. `zip`'s writer derives both headers instead,
//! and what it derives changed under us twice: zip 9.0's `raw_copy_file` wrote
//! central extra fields into the local header, and zip 9.1.3 rewrote a stored
//! entry's version fields (20 → 10) and copied some extra fields into both
//! headers (zip-rs/zip2#1172). Each change altered the bytes of entries no fix
//! had touched, in 481 of 562 shelf books (2026-10-28).
//!
//! Reading still goes through `zip`: where an entry starts is where the reader
//! that loaded it found it ([`layout`]), and every location is checked against
//! the bytes before anything is copied.

use std::io::Write;
use std::ops::Range;

use flate2::Compression;
use flate2::write::DeflateEncoder;

use crate::workspace::Error;

const LOCAL_SIG: &[u8; 4] = b"PK\x03\x04";
const CENTRAL_SIG: &[u8; 4] = b"PK\x01\x02";
const DESCRIPTOR_SIG: &[u8; 4] = b"PK\x07\x08";
const EOCD_SIG: &[u8; 4] = b"PK\x05\x06";
const ZIP64_EXTRA: u16 = 0x0001;
pub(crate) const STORED: u16 = 0;
pub(crate) const DEFLATED: u16 = 8;

fn malformed(msg: String) -> Error {
    Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn fits_u32(n: usize, what: &str) -> Result<u32, Error> {
    u32::try_from(n).map_err(|_| malformed(format!("{what} does not fit a 32-bit zip field")))
}

/// One central-directory record of the original archive, and where its entry
/// lies in the original bytes.
#[derive(Debug, Clone)]
pub(crate) struct Raw {
    pub name: String,
    pub is_dir: bool,
    /// The local header, the data and the data descriptor, if any.
    pub local: Range<usize>,
    /// The central-directory record.
    pub central: Range<usize>,
}

impl Raw {
    /// The compression method the central directory records.
    pub fn method(&self, src: &[u8]) -> u16 {
        u16_at(src, self.central.start + 10)
    }
}

/// Every central-directory record of `src`, in central-directory order, with
/// its entry located. `zip` names each local header's offset and compressed
/// size, and its `by_index_raw` has read and checked both the local header and
/// the central record there, so their signatures and lengths are not checked
/// again. What it does not check is that the data fits: `zip` loads an entry
/// that claims more bytes than the archive holds, short, and copying it is an
/// error here.
///
/// A data descriptor (bit 3) is CRC and two sizes, 8 bytes each when the local
/// header carries a ZIP64 field (APPNOTE 4.3.9), optionally after its
/// signature. It is taken as far as the next local header or central record
/// at most, so a short or missing one is copied as it is and never takes a
/// neighbour's bytes with it.
pub(crate) fn layout(src: &[u8]) -> Result<Vec<Raw>, Error> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(src))?;
    let mut found = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i)?;
        let name = f.name()?.into_owned();
        let at = |o: u64| {
            usize::try_from(o).map_err(|_| malformed(format!("entry {name}: offset too large")))
        };
        let (start, csize, c) = (
            at(f.header_start())?,
            at(f.compressed_size())?,
            at(f.central_header_start())?,
        );
        found.push((name, f.is_dir(), start, csize, c));
    }
    let mut starts: Vec<usize> = found.iter().flat_map(|e| [e.2, e.4]).collect();
    starts.sort_unstable();

    let mut out = Vec::with_capacity(found.len());
    for (name, is_dir, start, csize, c) in found {
        let flags = u16_at(src, start + 6);
        let (n, x) = (
            u16_at(src, start + 26) as usize,
            u16_at(src, start + 28) as usize,
        );
        let data_end = start + 30 + n + x + csize;
        if data_end > src.len() {
            return Err(malformed(format!(
                "entry {name}: data runs past the end of the archive"
            )));
        }
        let mut end = data_end;
        if flags & 0x0008 != 0 {
            let next = starts
                .iter()
                .copied()
                .find(|&s| s >= data_end)
                .unwrap_or(src.len());
            let zip64 = extra_fields(&src[start + 30 + n..start + 30 + n + x])
                .any(|(id, _)| id == ZIP64_EXTRA);
            let sig = usize::from(src.get(end..end + 4) == Some(DESCRIPTOR_SIG)) * 4;
            end = (end + sig + if zip64 { 20 } else { 12 }).min(next);
        }
        let clen = 46
            + u16_at(src, c + 28) as usize
            + u16_at(src, c + 30) as usize
            + u16_at(src, c + 32) as usize;
        out.push(Raw {
            name,
            is_dir,
            local: start..end,
            central: c..c + clen,
        });
    }
    Ok(out)
}

/// The `(id, data)` fields of an extra-field block. A truncated last field
/// ends the walk; it is never read past.
fn extra_fields(block: &[u8]) -> impl Iterator<Item = (u16, &[u8])> {
    let mut at = 0;
    std::iter::from_fn(move || {
        if at + 4 > block.len() {
            return None;
        }
        let (id, len) = (u16_at(block, at), u16_at(block, at + 2) as usize);
        let data = block.get(at + 4..at + 4 + len)?;
        at += 4 + len;
        Some((id, data))
    })
}

/// `block` without its ZIP64 field, which describes sizes and offsets this
/// writer states in the 32-bit fields instead. A truncated tail is dropped
/// with it, since no reader could have parsed it.
fn without_zip64(block: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for (id, data) in extra_fields(block) {
        if id != ZIP64_EXTRA {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&(data.len() as u16).to_le_bytes());
            out.extend_from_slice(data);
        }
    }
    out
}

/// Set the local-header offset of the central-directory `record`. When the
/// 32-bit field says `0xFFFFFFFF`, the offset lives in the record's ZIP64
/// field, after the uncompressed and compressed sizes that are also marked
/// there (APPNOTE 4.5.3), and is written in that place.
pub(crate) fn set_offset(record: &mut [u8], offset: u32) -> Result<(), Error> {
    if u32_at(record, 42) != u32::MAX {
        record[42..46].copy_from_slice(&offset.to_le_bytes());
        return Ok(());
    }
    let n = u16_at(record, 28) as usize;
    let x = u16_at(record, 30) as usize;
    let mut skip = 0;
    for size_at in [24, 20] {
        if u32_at(record, size_at) == u32::MAX {
            skip += 8;
        }
    }
    let mut at = 46 + n;
    while at + 4 <= 46 + n + x {
        let (id, len) = (u16_at(record, at), u16_at(record, at + 2) as usize);
        if id == ZIP64_EXTRA && skip + 8 <= len && at + 4 + len <= 46 + n + x {
            let slot = at + 4 + skip;
            record[slot..slot + 8].copy_from_slice(&u64::from(offset).to_le_bytes());
            return Ok(());
        }
        at += 4 + len;
    }
    Err(malformed(
        "a central-directory record marks its offset as ZIP64 but carries no ZIP64 offset".into(),
    ))
}

/// The bytes of one entry to write: its local part and its central record,
/// whose offset field is filled in when the local part's place is known.
struct Item {
    local: Vec<u8>,
    central: Vec<u8>,
}

/// An archive being written. Entries go to the central directory in the order
/// they are added, and to the file in the order of their `local_key`, so a
/// book whose local entries lie in another order than its central directory
/// lists them keeps both orders.
pub(crate) struct Writer<'a> {
    src: &'a [u8],
    items: Vec<((u8, usize), Item)>,
}

impl<'a> Writer<'a> {
    pub fn new(src: &'a [u8]) -> Self {
        Writer {
            src,
            items: Vec::new(),
        }
    }

    /// An untouched entry, verbatim, where it lay in the original.
    pub fn copy(&mut self, raw: &Raw) {
        let item = Item {
            local: self.src[raw.local.clone()].to_vec(),
            central: self.src[raw.central.clone()].to_vec(),
        };
        self.items.push(((1, raw.local.start), item));
    }

    /// An entry a fix rewrote, at its original place, with every header field
    /// of the original but those the new data decides: CRC, sizes, and the
    /// data-descriptor flag (the sizes are now in the header). The ZIP64 field
    /// goes with it. `method` is the original's unless `first_stored` asks for
    /// `mimetype`'s OCF packaging: written first, stored, and with no local
    /// extra field (`PKG-005`).
    pub fn rewrite(&mut self, raw: &Raw, data: &[u8], first_stored: bool) -> Result<(), Error> {
        let src = self.src;
        let l = raw.local.start;
        let c = raw.central.start;
        let method = if first_stored {
            STORED
        } else {
            raw.method(src)
        };
        let (crc, body) = encode(data, method, &raw.name)?;
        let flags = u16_at(src, l + 6) & !0x0008;
        let cflags = u16_at(src, c + 8) & !0x0008;
        let ln = u16_at(src, l + 26) as usize;
        let lx = u16_at(src, l + 28) as usize;
        let local_extra = if first_stored {
            Vec::new()
        } else {
            without_zip64(&src[l + 30 + ln..l + 30 + ln + lx])
        };
        let mut local = header(
            LOCAL_SIG,
            &[
                u16_at(src, l + 4),
                flags,
                method,
                u16_at(src, l + 10),
                u16_at(src, l + 12),
            ],
            crc,
            body.len(),
            data.len(),
            &src[l + 30..l + 30 + ln],
            &local_extra,
        )?;
        local.extend_from_slice(&body);

        let cn = u16_at(src, c + 28) as usize;
        let cx = u16_at(src, c + 30) as usize;
        let cc = u16_at(src, c + 32) as usize;
        let name_at = c + 46;
        let central_extra = without_zip64(&src[name_at + cn..name_at + cn + cx]);
        let comment = &src[name_at + cn + cx..name_at + cn + cx + cc];
        let central = central_record(
            &[
                u16_at(src, c + 4),
                u16_at(src, c + 6),
                cflags,
                method,
                u16_at(src, c + 12),
                u16_at(src, c + 14),
            ],
            crc,
            body.len(),
            data.len(),
            &src[name_at..name_at + cn],
            &central_extra,
            comment,
            u16_at(src, c + 36),
            u32_at(src, c + 38),
        )?;
        let key = if first_stored { (0, 0) } else { (1, l) };
        self.items.push((key, Item { local, central }));
        Ok(())
    }

    /// An entry the original never had, deflated, after every original entry.
    /// Its header fields are fixed, so the same content always writes the same
    /// bytes: version 2.0, MS-DOS, 1980-01-01 00:00, no extra field; bit 11
    /// marks a name that is not ASCII as UTF-8.
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), Error> {
        let (crc, body) = encode(data, DEFLATED, name)?;
        let flags = if name.is_ascii() { 0 } else { 0x0800 };
        let fields = [20, flags, DEFLATED, 0, 0x0021];
        let mut local = header(
            LOCAL_SIG,
            &fields,
            crc,
            body.len(),
            data.len(),
            name.as_bytes(),
            &[],
        )?;
        local.extend_from_slice(&body);
        let mut cfields = [20; 6];
        cfields[1..].copy_from_slice(&fields);
        let central = central_record(
            &cfields,
            crc,
            body.len(),
            data.len(),
            name.as_bytes(),
            &[],
            &[],
            0,
            0,
        )?;
        let key = (2, self.items.len());
        self.items.push((key, Item { local, central }));
        Ok(())
    }

    /// The archive: every local part in file order, the central directory in
    /// the order entries were added (a hoisted `mimetype` first in both), and
    /// an end record carrying `comment`.
    pub fn finish(self, comment: &[u8]) -> Result<Vec<u8>, Error> {
        let count = entry_count(self.items.len())?;
        let mut file_order: Vec<usize> = (0..self.items.len()).collect();
        file_order.sort_by_key(|&i| self.items[i].0);
        let mut central_order: Vec<usize> = (0..self.items.len()).collect();
        central_order.sort_by_key(|&i| self.items[i].0.0 != 0);
        let mut out = Vec::new();
        let mut offsets = vec![0u32; self.items.len()];
        for i in file_order {
            offsets[i] = fits_u32(out.len(), "an entry offset")?;
            out.extend_from_slice(&self.items[i].1.local);
        }
        let cd_start = fits_u32(out.len(), "the central-directory offset")?;
        for i in central_order {
            let mut record = self.items[i].1.central.clone();
            set_offset(&mut record, offsets[i])?;
            out.extend_from_slice(&record);
        }
        let cd_size = fits_u32(out.len() - cd_start as usize, "the central-directory size")?;
        let comment_len = u16::try_from(comment.len())
            .map_err(|_| malformed("the archive comment is too long".into()))?;
        out.extend_from_slice(EOCD_SIG);
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_start.to_le_bytes());
        out.extend_from_slice(&comment_len.to_le_bytes());
        out.extend_from_slice(comment);
        Ok(out)
    }
}

/// The entry count of the end record. 0xFFFF there means "see the ZIP64
/// record", which this writer does not write, so the most it holds is 0xFFFE.
fn entry_count(n: usize) -> Result<u16, Error> {
    u16::try_from(n)
        .ok()
        .filter(|&n| n != u16::MAX)
        .ok_or_else(|| malformed("more entries than a non-ZIP64 archive holds".into()))
}

/// `data`'s CRC-32 and its bytes compressed by `method`. Only the two methods
/// an entry can have been loaded with are written.
fn encode(data: &[u8], method: u16, name: &str) -> Result<(u32, Vec<u8>), Error> {
    let mut crc = flate2::Crc::new();
    crc.update(data);
    let body = match method {
        STORED => data.to_vec(),
        DEFLATED => {
            let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
            e.write_all(data)?;
            e.finish()?
        }
        m => {
            return Err(malformed(format!(
                "entry {name}: compression method {m} cannot be written"
            )));
        }
    };
    Ok((crc.sum(), body))
}

/// A local header: `fields` are version needed, flags, method, time, date.
fn header(
    sig: &[u8; 4],
    fields: &[u16; 5],
    crc: u32,
    csize: usize,
    usize_: usize,
    name: &[u8],
    extra: &[u8],
) -> Result<Vec<u8>, Error> {
    let mut h = sig.to_vec();
    for f in fields {
        h.extend_from_slice(&f.to_le_bytes());
    }
    h.extend_from_slice(&crc.to_le_bytes());
    h.extend_from_slice(&fits_u32(csize, "a compressed size")?.to_le_bytes());
    h.extend_from_slice(&fits_u32(usize_, "a size")?.to_le_bytes());
    h.extend_from_slice(&(name.len() as u16).to_le_bytes());
    h.extend_from_slice(&len16(extra)?.to_le_bytes());
    h.extend_from_slice(name);
    h.extend_from_slice(extra);
    Ok(h)
}

fn len16(b: &[u8]) -> Result<u16, Error> {
    u16::try_from(b.len()).map_err(|_| malformed("an extra-field block is too long".into()))
}

/// A central-directory record with offset 0, set later: `fields` are version
/// made by, version needed, flags, method, time, date.
#[allow(clippy::too_many_arguments)]
fn central_record(
    fields: &[u16; 6],
    crc: u32,
    csize: usize,
    usize_: usize,
    name: &[u8],
    extra: &[u8],
    comment: &[u8],
    internal: u16,
    external: u32,
) -> Result<Vec<u8>, Error> {
    let mut r = CENTRAL_SIG.to_vec();
    for f in fields {
        r.extend_from_slice(&f.to_le_bytes());
    }
    r.extend_from_slice(&crc.to_le_bytes());
    r.extend_from_slice(&fits_u32(csize, "a compressed size")?.to_le_bytes());
    r.extend_from_slice(&fits_u32(usize_, "a size")?.to_le_bytes());
    r.extend_from_slice(&(name.len() as u16).to_le_bytes());
    r.extend_from_slice(&len16(extra)?.to_le_bytes());
    r.extend_from_slice(&len16(comment)?.to_le_bytes());
    r.extend_from_slice(&[0; 2]); // disk number start
    r.extend_from_slice(&internal.to_le_bytes());
    r.extend_from_slice(&external.to_le_bytes());
    r.extend_from_slice(&[0; 4]); // local header offset, set in `finish`
    r.extend_from_slice(name);
    r.extend_from_slice(extra);
    r.extend_from_slice(comment);
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A central record for `a`, its three 32-bit size and offset fields set
    /// to `fields`, with `extra` as its extra-field block.
    fn record(fields: [u32; 3], extra: &[u8]) -> Vec<u8> {
        let mut r = central_record(&[20; 6], 0, 0, 0, b"a", extra, &[], 0, 0).unwrap();
        r[20..24].copy_from_slice(&fields[0].to_le_bytes());
        r[24..28].copy_from_slice(&fields[1].to_le_bytes());
        r[42..46].copy_from_slice(&fields[2].to_le_bytes());
        r
    }

    fn zip64(values: &[u64]) -> Vec<u8> {
        let mut x = ZIP64_EXTRA.to_le_bytes().to_vec();
        x.extend_from_slice(&((values.len() * 8) as u16).to_le_bytes());
        for v in values {
            x.extend_from_slice(&v.to_le_bytes());
        }
        x
    }

    #[test]
    fn a_32_bit_offset_is_set_in_place() {
        let mut r = record([5, 5, 7], &[]);
        set_offset(&mut r, 99).unwrap();
        assert_eq!(u32_at(&r, 42), 99);
    }

    /// The ZIP64 field holds only the values its 32-bit fields mark, in the
    /// order uncompressed size, compressed size, offset: the offset is the
    /// third, second or first value, and the sizes are never overwritten.
    #[test]
    fn a_zip64_offset_is_set_after_the_sizes_it_follows() {
        let m = u32::MAX;
        for (fields, values, slot) in [
            ([m, m, m], vec![1, 2, 3], 2),
            ([5, m, m], vec![1, 3], 1),
            ([m, 5, m], vec![2, 3], 1),
            ([5, 5, m], vec![3], 0),
        ] {
            let mut extra = 0x5455u16.to_le_bytes().to_vec();
            extra.extend_from_slice(&[1, 0, 0]);
            extra.extend_from_slice(&zip64(&values));
            let mut r = record(fields, &extra);
            set_offset(&mut r, 99).unwrap();
            let mut want = values.clone();
            want[slot] = 99;
            // After the name and the 5-byte 0x5455 field, past the ZIP64 field's header.
            let field = &r[46 + 1 + 5 + 4..];
            for (i, v) in want.iter().enumerate() {
                let got = u64::from_le_bytes(field[i * 8..i * 8 + 8].try_into().unwrap());
                assert_eq!(got, *v, "{fields:?} value {i}");
            }
            assert_eq!(u32_at(&r, 42), m, "the 32-bit field still points to ZIP64");
        }
    }

    #[test]
    fn a_zip64_offset_with_nowhere_to_go_is_an_error() {
        let m = u32::MAX;
        assert!(set_offset(&mut record([5, 5, m], &[]), 1).is_err());
        // The field holds the two sizes but not the offset.
        assert!(set_offset(&mut record([m, m, m], &zip64(&[1, 2])), 1).is_err());
    }

    /// A field whose stated length runs past the block ends the walk there.
    #[test]
    fn a_truncated_extra_field_is_not_read_past() {
        let mut block = 0x5455u16.to_le_bytes().to_vec();
        block.extend_from_slice(&[1, 0, 7]);
        block.extend_from_slice(&[0x66, 0x66, 9, 0, 1]);
        assert_eq!(extra_fields(&block).count(), 1);
        assert_eq!(without_zip64(&block), block[..5]);
    }

    #[test]
    fn an_archive_past_0xfffe_entries_is_refused() {
        assert_eq!(entry_count(0xFFFE).unwrap(), 0xFFFE);
        assert!(entry_count(0xFFFF).is_err());
        assert!(entry_count(0x10000).is_err());
        assert!(Writer::new(&[]).finish(&[]).is_ok());
    }
}
