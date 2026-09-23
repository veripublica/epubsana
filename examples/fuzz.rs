//! Mutation fuzzer: corrupts two small books, each carrying many defects
//! epubsana repairs, thousands of times, and asks whether planning, applying
//! and re-validating any of the results panics.
//!
//! epubveri runs the same kind of fuzzer against a *valid* book, which is
//! right for a detector and wrong for a repairer: on a valid book nothing is
//! planned and no fixer code runs. So both seeds here are defective on
//! purpose, and each result goes through the whole of [`epubsana::repair`]
//! with every fix approved, the path the CLI's `--yes` takes.
//!
//! A panic in the library is a crash for everyone who embeds it (the CLI, the
//! WASM package, where a panic aborts, and epublift), and the book the user
//! wanted repaired never comes back. The unit tests cover the shapes someone
//! thought of; this covers the ones nobody did.
//!
//! Deterministic by construction: the same `--seed` generates the same books,
//! so a crash reproduces from its seed and index alone. Explore new ground
//! with a different seed. A crashing or slow book is written under
//! `epubsana-fuzz-crashes/` in the system temp directory and the run exits 1.
//!
//! Usage:
//!     cargo run --release --example fuzz
//!     cargo run --release --example fuzz -- --seed 7 --books 20000
//!     cargo run --release --example fuzz -- --plan   # what the seeds trigger
//!
//! Not a coverage-guided fuzzer: that needs libFuzzer, which is C++ and a
//! nightly toolchain, against the pure-Rust rule. The mutation strategy is
//! epubveri's (`harness/src/fuzz.rs`), so the two explore the same ground.

use std::collections::BTreeMap;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use epubsana::{Confirmer, Decision, Goal, Policy, ProposedFix, Workspace};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CONTAINER: &str = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/p.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

// ---- Seed 1: EPUB 3 ----------------------------------------------------------

const OPF3: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier><dc:title>T</dc:title><dc:language>en</dc:language><dc:description></dc:description><dc:source/><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata>
<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="c 2.xhtml" media-type="application/xhtml+xml"/><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="f" href="f.otf" media-type="application/vnd.ms-opentype"/><item id="gone" href="gone.png" media-type="image/png"/><item id="css" href="s.css" media-type="text/css"/></manifest>
<spine toc="ncx"><itemref idref="c1"/><itemref idref="c2"/><itemref idref="c1"/><itemref idref="nope"/></spine>
<guide><reference type="text" href="c1.xhtml#nowhere" title="t"/><reference type="toc" href="missing.xhtml" title="c"/></guide></package>"##;

const NAV3: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>t</title></head><body><nav epub:type="toc"><ol><li><a href="c1.xhtml">Chapter One</a></li><li><a href="c%202.xhtml">Chapter Two</a></li></ol></nav><nav epub:type="landmarks" hidden=""><ol></ol></nav></body></html>"#;

const C1: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en" lang="fr"><head><title></title><meta http-equiv="Content-Type" content="text/html; charset=utf-8"/><link rel="stylesheet" href="s.css"/></head><body>stray text<p id="1a">A&nbsp;b &mdash c &hellip;</p><p id="n2">note <a href="#n1">1</a></p><a href="x"><a href="y">nested</a></a><blockquote>q</blockquote></body></html>"##;

const C2: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>  </title></head><body><h1>Two</h1><p id="n1">the note <a href="#n2">back</a></p><p id="n1x">x</p></body></html>"##;

const NCX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd">
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="urn:isbn:0000"/></head><docTitle><text>T</text></docTitle><navMap><navPoint id="1" playOrder="3"><navLabel><text>One</text></navLabel><content src="c1.xhtml"/></navPoint><navPoint id="n 2" playOrder="1"><navLabel><text>Two</text></navLabel><content src="c 2.xhtml"/></navPoint><navPoint id="p3" playOrder="2"><navLabel><text>Back</text></navLabel><content src="c1.xhtml"/></navPoint></navMap></ncx>"#;

const CSS: &str = "@font-face{font-family:\"F\";src:url(f.otf)}\n@font-face{font-family:\"G\";src:url(missing.ttf)}\np{margin:1em}\n";

// ---- Seed 2: EPUB 2 ----------------------------------------------------------

const OPF2: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf"><dc:identifier id="uid"></dc:identifier><dc:identifier>urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier><dc:title>T</dc:title><dc:language>en</dc:language><dc:date></dc:date><dc:creator opf:file-as="">A</dc:creator><meta name="cover" content="img"/></metadata>
<manifest><item id="c1" href="c1.html" media-type="application/xhtml+xml" properties="scripted"/><item id="c2" href="c 2.xhtml" media-type="application/xhtml+xml"/><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="img" href="i.jpg" media-type="image/jpeg"/><item id="f" href="f.ttf" media-type="application/x-font-ttf"/></manifest>
<spine toc="ncx"><itemref idref="c1"/><itemref idref="c2"/><itemref idref="c2"/></spine>
<guide><reference type="cover" href="c1.html#nowhere" title="c"/><reference type="cover" href="c1.html#nowhere" title="c"/></guide></package>"##;

const C1_EPUB2: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml"><head><title></title><meta http-equiv="Content-Type" content="text/html; charset=iso-8859-1"/></head><body><section><p>A&nbsp;b&mdash;c &amp x</p></section><p id="n1">text <a href="c%202.xhtml#n1">1</a></p></body></html>"#;

const C2_EPUB2: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Two</title></head><body><h2>Two</h2><div align="center">x</div></body></html>"#;

/// Fragments that sit on the boundaries parsers and byte-offset arithmetic get
/// wrong: markup delimiters, entity starts, multi-byte letters, a BOM, a NUL.
const TOKENS: &[&str] = &[
    "<",
    ">",
    "&",
    "&#",
    "&#x",
    "\"",
    "'",
    "/",
    "</p>",
    "<p>",
    ";",
    "{",
    "}",
    "(",
    ")",
    "\\",
    "#",
    ":",
    "\u{e0}",
    "\u{c5}",
    "\u{200b}",
    "\u{a0}",
    "%",
    "%20",
    "..",
    "\0",
    "\u{feff}",
    "=",
    "-->",
    "<!--",
    "<![CDATA[",
    "]]>",
    "<!ENTITY",
    "<!DOCTYPE",
    "epub:",
    "xml:lang=\"",
    "id=\"",
    "href=\"",
    "src=\"",
    "&nbsp;",
    "&mdash",
    "\n",
    "99999999999999999999",
    "-1",
    "",
    " ",
];

/// xorshift64*: enough randomness to walk the input space, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// One to six edits to `s`, on characters so the result stays UTF-8.
fn mutate(s: &str, rng: &mut Rng) -> Vec<u8> {
    let mut c: Vec<char> = s.chars().collect();
    for _ in 0..1 + rng.below(6) {
        let i = rng.below(c.len() + 1);
        match rng.below(100) {
            0..35 => {
                let t = TOKENS[rng.below(TOKENS.len())];
                c.splice(i..i, t.chars());
            }
            35..60 if !c.is_empty() => {
                let end = (i + 1 + rng.below(8)).min(c.len());
                c.drain(i.min(c.len())..end);
            }
            60..80 if !c.is_empty() => {
                let (a, b) = (rng.below(c.len()), rng.below(c.len()));
                let run: Vec<char> = c[a.min(b)..a.max(b)].iter().take(200).copied().collect();
                c.splice(i..i, run);
            }
            _ => {
                let ch = char::from_u32(1 + rng.below(0x2FFF) as u32).unwrap_or('\u{1F600}');
                c.insert(i, ch);
            }
        }
    }
    let mut bytes = String::from_iter(c).into_bytes();
    // Occasionally break the encoding itself: a stray continuation byte or a
    // truncated sequence, which no &str-typed input can express.
    if rng.chance(10) && !bytes.is_empty() {
        let i = rng.below(bytes.len());
        bytes[i] = [0x80, 0xA0, 0x85, 0xC3, 0xE2, 0xFF][rng.below(6)];
    }
    bytes
}

fn seed_files(epub2: bool) -> Vec<(&'static str, Vec<u8>)> {
    let t = |s: &str| s.as_bytes().to_vec();
    if epub2 {
        vec![
            ("META-INF/container.xml", t(CONTAINER)),
            ("OEBPS/p.opf", t(OPF2)),
            ("OEBPS/c1.html", t(C1_EPUB2)),
            ("OEBPS/c 2.xhtml", t(C2_EPUB2)),
            ("OEBPS/toc.ncx", t(NCX)),
            ("OEBPS/f.ttf", b"OTTO\0\0\0\0".to_vec()),
        ]
    } else {
        vec![
            ("META-INF/container.xml", t(CONTAINER)),
            ("OEBPS/p.opf", t(OPF3)),
            ("OEBPS/nav.xhtml", t(NAV3)),
            ("OEBPS/c1.xhtml", t(C1)),
            ("OEBPS/c 2.xhtml", t(C2)),
            ("OEBPS/toc.ncx", t(NCX)),
            ("OEBPS/s.css", t(CSS)),
            ("OEBPS/f.otf", b"\0\x01\0\0\0\0\0\0".to_vec()),
        ]
    }
}

fn zip(files: &[(&str, Vec<u8>)], mimetype_first: bool) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut out));
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let mime = |zip: &mut ZipWriter<_>| {
            zip.start_file("mimetype", deflated).expect("zip");
            zip.write_all(b"application/epub+zip").expect("zip");
        };
        if mimetype_first {
            zip.start_file("mimetype", stored).expect("zip");
            zip.write_all(b"application/epub+zip").expect("zip");
        }
        for (name, data) in files {
            zip.start_file(*name, deflated).expect("zip");
            zip.write_all(data).expect("zip");
        }
        if !mimetype_first {
            mime(&mut zip); // PKG-006: fix.mimetype_packaging has work to do
        }
        zip.finish().expect("zip");
    }
    out
}

fn book(rng: &mut Rng, epub2: bool) -> Vec<u8> {
    let mut files = seed_files(epub2);
    for _ in 0..1 + rng.below(3) {
        let k = rng.below(files.len());
        let text = String::from_utf8_lossy(&files[k].1).into_owned();
        files[k].1 = mutate(&text, rng);
    }
    zip(&files, !rng.chance(10))
}

struct ApproveAll;

impl Confirmer for ApproveAll {
    fn decide(&mut self, _: &ProposedFix) -> Decision {
        Decision::Approve
    }
}

/// Load, plan, apply every fix, re-validate, and serialize: every stage a
/// hostile book can reach. An `Err` is a fine answer; only a panic is not.
fn run(bytes: &[u8]) {
    let Ok(mut ws) = Workspace::load(bytes) else {
        return;
    };
    if epubsana::repair(&mut ws, Goal::Valid, Policy::AskEach, &mut ApproveAll).is_ok() {
        let _ = ws.serialize();
    }
}

/// `--plan`: which fixers each unmutated seed triggers, so a seed that has
/// stopped exercising anything is visible rather than silently cheap.
fn show_plan() {
    for (label, epub2) in [("epub3", false), ("epub2", true)] {
        let ws = Workspace::load(&zip(&seed_files(epub2), false)).expect("seed loads");
        let report = ws.detect().expect("seed detects");
        let mut by_fixer: BTreeMap<&str, usize> = BTreeMap::new();
        for fix in epubsana::fixers::plan(&report, &ws, Goal::Valid) {
            *by_fixer.entry(fix.fix_id).or_default() += 1;
        }
        println!(
            "{label}: {} error(s), {} fatal(s); {} fixer(s) plan:",
            report.errors(),
            report.fatals(),
            by_fixer.len()
        );
        for (id, n) in by_fixer {
            println!("  {id} x{n}");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--plan") {
        show_plan();
        return;
    }
    let arg = |flag: &str, default: u64| -> u64 {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    };
    let seed = arg("--seed", 1);
    let books = arg("--books", 3000);
    // Anything slower than this on a book this small is a finding too, even
    // though in-process it cannot be interrupted, only reported afterwards.
    let slow = Duration::from_secs(5);

    // Outside the repository: a stray `target/` beside the sources is how a
    // stale build was once mistaken for a fresh one.
    let out = std::env::temp_dir().join("epubsana-fuzz-crashes");

    // The default panic hook stays: its message names the panicking line,
    // which is the first thing a crash needs.
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let (mut crashes, mut slows) = (0usize, 0usize);
    let start = Instant::now();
    for i in 0..books {
        let epub2 = i % 2 == 1;
        let bytes = book(&mut rng, epub2);
        let t = Instant::now();
        let r = catch_unwind(AssertUnwindSafe(|| run(&bytes)));
        let took = t.elapsed();
        let failure = if r.is_err() {
            crashes += 1;
            Some("PANIC")
        } else if took > slow {
            slows += 1;
            Some("SLOW")
        } else {
            None
        };
        if let Some(what) = failure {
            std::fs::create_dir_all(&out).expect("create crash dir");
            let path = out.join(format!("seed{seed}-book{i}.epub"));
            std::fs::write(&path, &bytes).expect("write crash book");
            println!(
                "  {what:<5} seed {seed} book {i} ({took:.1?}) -> {}",
                path.display()
            );
        }
    }
    println!(
        "fuzz: seed {seed}, {books} books in {:.1?}: {crashes} panic(s), {slows} slow",
        start.elapsed()
    );
    if crashes + slows > 0 {
        std::process::exit(1);
    }
}
