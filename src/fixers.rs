//! The fix registry: turn epubveri findings into [`ProposedFix`]es.
//!
//! Each fixer keys off an epubveri message `rule` (or an unambiguous `id`) and
//! builds a proposal, or declines (returns nothing) when it can't fix a finding
//! safely. The registry grows one carefully-argued entry at a time, in the order
//! real books ask for: what a fixer changes, why that is content-preserving, and
//! when it declines is specified in `docs/FIXERS.md` before it is coded.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ops::Range;

use epubveri::report::{Report, Severity};

use crate::{Change, Goal, ProposedFix, Tier, Workspace, entities};

/// Build the ordered list of proposals for a detection [`Report`].
pub fn plan(report: &Report, ws: &Workspace, _goal: Goal) -> Vec<ProposedFix> {
    let mut fixes = Vec::new();
    fixes.extend(html_entities(report, ws));
    fixes.extend(ncx_ncnames(report, ws));
    fixes.extend(content_type_meta(report, ws));
    fixes.extend(ncx_dtb_uid(report, ws));
    fixes.extend(manifest_href_spaces(report, ws));
    fixes.extend(content_properties(report, ws));
    fixes.extend(empty_titles(report, ws));
    fixes.extend(bare_text_in_body(report, ws));
    fixes.extend(mimetype_packaging(report, ws));
    // Future fixers append here, in a sensible confirm order.
    fixes
}

/// The severity epubveri gave the finding a fixer addresses — a fix inherits it
/// verbatim (FORMATS.md §1.3). epubveri pushes a given `rule` at one severity,
/// so the first matching message speaks for the whole group; the fallback never
/// fires in practice (a fixer is only built from findings that are present) and
/// is deliberately the invalidating value, never a flattering one.
fn addressed_severity(report: &Report, id: &str, rule: Option<&str>) -> Severity {
    report
        .messages
        .iter()
        .find(|m| m.id == id && m.rule == rule)
        .map(|m| m.severity)
        .unwrap_or(Severity::Error)
}

/// `RSC-016` / `htm.entity.undeclared`: XHTML referencing HTML named entities
/// (`&nbsp;`, `&mdash;`, …) without a DTD. Grouped **per file** (one proposal
/// per document, not one per occurrence — a book can have thousands), replacing
/// each known entity with the character it denotes. Entities we don't map are
/// left untouched (they remain flagged — we never guess). Pure `AutoSafe`.
fn html_entities(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // file -> (entity name -> occurrence count), only for entities we can map.
    let mut by_file: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("htm.entity.undeclared") {
            continue;
        }
        let (Some(file), Some(name)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        if entities::lookup(name).is_none() {
            continue; // unknown entity — leave it alone, don't propose a guess
        }
        *by_file
            .entry(file.to_string())
            .or_default()
            .entry(name.clone())
            .or_insert(0) += 1;
    }

    let mut fixes = Vec::new();
    for (file, ents) in by_file {
        // Skip if the file isn't actually present as text (defensive).
        if ws.get_text(&file).is_none() {
            continue;
        }
        let distinct = ents.len();
        let total: usize = ents.values().sum();

        let preview: Vec<Change> = ents
            .iter()
            .map(|(name, count)| {
                let repl = entities::lookup(name).unwrap_or("");
                Change {
                    path: file.clone(),
                    note: format!("replace &{name}; → '{repl}' ({count}×)"),
                }
            })
            .collect();

        // The replacement pairs, applied by re-reading the file at apply time
        // (robust to any earlier edit).
        let repls: Vec<(String, &'static str)> = ents
            .keys()
            .map(|name| (format!("&{name};"), entities::lookup(name).unwrap()))
            .collect();
        let file_for_apply = file.clone();

        let summary = ents.keys().cloned().collect::<Vec<_>>().join(", ");

        fixes.push(ProposedFix {
            fix_id: "fix.html_entities",
            addresses_id: "RSC-016".to_string(),
            addresses_rule: Some("htm.entity.undeclared"),
            addresses_severity: addressed_severity(
                report,
                "RSC-016",
                Some("htm.entity.undeclared"),
            ),
            tier: Tier::AutoSafe,
            title: format!(
                "Map {distinct} undeclared HTML entit{} ({total}×) to characters in {file} ({summary})",
                if distinct == 1 { "y" } else { "ies" },
            ),
            rationale:
                "These are standard HTML named entities used in XHTML without a DTD that declares \
                 them. Replacing each with the exact character it denotes is content-preserving and \
                 removes the undeclared-entity error."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(mut text) = ws.get_text(&file_for_apply) {
                    for (from, to) in &repls {
                        text = text.replace(from, to);
                    }
                    ws.set_text(&file_for_apply, text);
                }
            }),
        });
    }
    fixes
}

/// `RSC-005` / `ncx.ids.invalid_ncname`: an `id` attribute in the NCX that is
/// not a valid XML NCName. Real corpus (25 books, 631×) shows two shapes, both
/// really UUIDs: raw UUIDs that start with a digit (`51100e1e-…`) and
/// brace-wrapped GUIDs (`{0F5794B8-…}`). NCX ids are never IDREF targets in an
/// EPUB — confirmed on the corpus, each bad id occurs exactly once
/// container-wide — so making them valid needs **no reference rewriting**.
/// Grouped per NCX file; `ConfirmNeeded` (a visible id change, so the user
/// approves it — unlike the invisible entity mapping).
fn ncx_ncnames(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // file -> ordered, de-duplicated bad ids (from params[0]).
    let mut by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("ncx.ids.invalid_ncname") {
            continue;
        }
        let (Some(file), Some(bad)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        let list = by_file.entry(file.to_string()).or_default();
        if !list.contains(bad) {
            list.push(bad.clone());
        }
    }

    let mut fixes = Vec::new();
    for (file, bad_ids) in by_file {
        let Some(text) = ws.get_text(&file) else {
            continue;
        };
        let mut used = existing_ids(&text);

        let mut renames: Vec<(String, String)> = Vec::new();
        for bad in &bad_ids {
            // Only touch an id whose attribute occurs exactly once, so the
            // surgical replace is unambiguous (declines duplicates / oddities).
            if attr_occurrences(&text, bad) != 1 {
                continue;
            }
            let Some(base) = sanitize_ncname(bad) else {
                continue; // nothing valid to preserve — never guess
            };
            let new = make_unique(base, &used);
            used.insert(new.clone());
            renames.push((bad.clone(), new));
        }
        if renames.is_empty() {
            continue;
        }

        let preview: Vec<Change> = renames
            .iter()
            .map(|(bad, new)| Change {
                path: file.clone(),
                note: format!("rename NCX id \"{bad}\" → \"{new}\""),
            })
            .collect();

        let n = renames.len();
        let renames_for_apply = renames.clone();
        let file_for_apply = file.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.ncx_ncnames",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("ncx.ids.invalid_ncname"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("ncx.ids.invalid_ncname"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Make {n} invalid NCX id{} a valid XML NCName in {file}",
                if n == 1 { "" } else { "s" },
            ),
            rationale:
                "An `id` in the NCX must be a valid XML NCName (it may not start with a digit, \
                 nor contain characters like '{', '}' or ':'). NCX ids are not referenced by \
                 IDREF anywhere in an EPUB, so sanitizing the value is content-preserving and \
                 clears the error without touching any reference."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(mut text) = ws.get_text(&file_for_apply) {
                    for (bad, new) in &renames_for_apply {
                        if let Some(updated) = replace_id_attr(&text, bad, new) {
                            text = updated;
                        }
                    }
                    ws.set_text(&file_for_apply, text);
                }
            }),
        });
    }
    fixes
}

/// Derive a valid XML NCName from an invalid `id`, preserving as much of the
/// original as possible: drop characters not allowed in an NCName, then prefix
/// `id_` if the result doesn't start with a letter or `_`. `None` when nothing
/// usable remains — we never invent an id from thin air.
fn sanitize_ncname(bad: &str) -> Option<String> {
    let filtered: String = bad
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .collect();
    let first = filtered.chars().next()?;
    Some(if first.is_alphabetic() || first == '_' {
        filtered
    } else {
        format!("id_{filtered}")
    })
}

/// Make `base` unique against `used` by suffixing `-2`, `-3`, … as needed, so a
/// rename never introduces a duplicate-id error.
fn make_unique(base: String, used: &HashSet<String>) -> String {
    if !used.contains(&base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !used.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Every `id` attribute value present in `text` (both quote styles), for
/// uniqueness checks. Over-inclusive (also sees `data-id` etc.) — harmless, it
/// only makes uniqueness stricter.
fn existing_ids(text: &str) -> HashSet<String> {
    let mut ids = HashSet::new();
    for quote in ['"', '\''] {
        let open = format!("id={quote}");
        let mut from = 0;
        while let Some(rel) = text[from..].find(open.as_str()) {
            let vstart = from + rel + open.len();
            match text[vstart..].find(quote) {
                Some(end_rel) => {
                    ids.insert(text[vstart..vstart + end_rel].to_string());
                    from = vstart + end_rel + 1;
                }
                None => break,
            }
        }
    }
    ids
}

/// Count `id="value"` / `id='value'` occurrences where `id` sits on an
/// attribute boundary (preceded by whitespace), so `data-id`/`xml:id` don't
/// count and the surgical replace stays unambiguous.
fn attr_occurrences(text: &str, value: &str) -> usize {
    let mut count = 0;
    for quote in ['"', '\''] {
        let needle = format!("id={quote}{value}{quote}");
        let mut from = 0;
        while let Some(rel) = text[from..].find(needle.as_str()) {
            let start = from + rel;
            if is_attr_boundary(text, start) {
                count += 1;
            }
            from = start + needle.len();
        }
    }
    count
}

/// Replace the single boundary `id="bad"` / `id='bad'` occurrence with `new`,
/// preserving the original quote style. `None` if not found on a boundary (the
/// caller guards, but this keeps apply defensive against unexpected text).
fn replace_id_attr(text: &str, bad: &str, new: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("id={quote}{bad}{quote}");
        let mut from = 0;
        while let Some(rel) = text[from..].find(needle.as_str()) {
            let start = from + rel;
            if is_attr_boundary(text, start) {
                let replacement = format!("id={quote}{new}{quote}");
                return Some(format!(
                    "{}{}{}",
                    &text[..start],
                    replacement,
                    &text[start + needle.len()..]
                ));
            }
            from = start + needle.len();
        }
    }
    None
}

/// True if byte `start` (the `i` of an `id=` match) begins a real attribute —
/// i.e. it's at the string start or preceded by whitespace. Excludes `data-id`,
/// `xml:id`, etc.
fn is_attr_boundary(text: &str, start: usize) -> bool {
    start == 0
        || text[..start]
            .chars()
            .next_back()
            .map(|c| c.is_whitespace())
            .unwrap_or(false)
}

/// `RSC-005` / `opf.content_document.invalid_content_type_meta`: a content
/// document whose legacy `<meta http-equiv="Content-Type" content="…">` does not
/// carry exactly `text/html; charset=utf-8` (real corpus: a bogus mime like
/// `http://www.w3.org/1999/xhtml; charset=utf-8`, or a missing space in
/// `text/html;charset=utf-8`; some files carry two such metas). Per the EPUB 3.3
/// reference, we normalize the encoding declaration to the current HTML5 form —
/// a single `<meta charset="utf-8"/>` — removing every legacy/duplicate
/// encoding meta so `conflicting_encoding_declarations` can't newly fire. This
/// is the first *structural* fixer: `params` is empty, so we parse the document
/// (roxmltree) to find each meta's exact byte range and edit surgically.
/// Declines (leaves flagged) any document that doesn't parse or that declares a
/// non-UTF-8 charset — we never blindly re-encode. `ConfirmNeeded`.
fn content_type_meta(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut files: BTreeSet<String> = BTreeSet::new();
    for m in &report.messages {
        if m.rule == Some("opf.content_document.invalid_content_type_meta")
            && let Some(loc) = m.location.as_deref()
        {
            files.insert(loc.to_string());
        }
    }

    let mut fixes = Vec::new();
    for file in files {
        let Some(text) = ws.get_text(&file) else {
            continue;
        };
        let Some(edits) = plan_encoding_normalization(&text) else {
            continue; // unparseable or non-UTF-8 — decline, never guess
        };
        if edits.is_empty() {
            continue;
        }

        let n = edits.len();
        let preview = vec![Change {
            path: file.clone(),
            note: format!(
                "normalize to a single <meta charset=\"utf-8\"/> ({n} encoding <meta> rewritten/removed)"
            ),
        }];
        let file_for_apply = file.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.content_type_meta",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("opf.content_document.invalid_content_type_meta"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("opf.content_document.invalid_content_type_meta"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Normalize the encoding declaration in {file} to HTML5 <meta charset=\"utf-8\">"
            ),
            rationale: "EPUB 3.3 content documents declare their encoding with the HTML5 \
                 `<meta charset=\"utf-8\">`. The legacy `<meta http-equiv=\"Content-Type\">` form \
                 (and any duplicate encoding declaration) is replaced so exactly one current-form \
                 declaration remains. Applied only when every declared charset is UTF-8 — the \
                 EPUB-required encoding — so this never re-encodes content."
                .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(edits) = plan_encoding_normalization(&text)
                {
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// One surgical byte-range edit (`replacement == ""` means delete).
struct MetaEdit {
    range: Range<usize>,
    replacement: String,
}

/// Compute the edits that collapse every encoding-declaration `<meta>` in an
/// XHTML document into a single `<meta charset="utf-8"/>`. `None` (decline) if
/// the document doesn't parse as XML or any encoding meta declares a non-UTF-8
/// charset. The returned edits are non-overlapping byte ranges over `text`.
/// Parse XML the way epubveri does — permitting a DTD/DOCTYPE, which NCX files
/// and many XHTML documents declare and which roxmltree's default parser
/// rejects. Every structural fixer parses through this so it sees exactly the
/// documents epubveri did.
fn parse_xml(text: &str) -> Option<roxmltree::Document<'_>> {
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(text, opts).ok()
}

/// A namespace-exact attribute lookup.
///
/// roxmltree 0.21 changed `Node::attribute(name)` to match by **local name,
/// ignoring namespace**, so `attribute("id")` now also returns `xml:id` and
/// `attribute("href")` also returns `xlink:href`. Every attribute epubsana's
/// fixers read is unqualified (a manifest `href`, an NCX `id`, a `meta`'s
/// `content`, …) — never a namespaced twin — so this restores the pre-0.21
/// behaviour: match only an attribute whose name carries no namespace. (Mirrors
/// epubveri's own `xmlext::NodeExt::attr_no_ns`, kept local to avoid depending
/// on epubveri's non-public helper.)
trait NodeExt<'a> {
    fn attr_no_ns(&self, name: &str) -> Option<&'a str>;
}

impl<'a> NodeExt<'a> for roxmltree::Node<'a, '_> {
    fn attr_no_ns(&self, name: &str) -> Option<&'a str> {
        self.attributes()
            .find(|a| a.namespace().is_none() && a.name() == name)
            .map(|a| a.value())
    }
}

fn plan_encoding_normalization(text: &str) -> Option<Vec<MetaEdit>> {
    let doc = parse_xml(text)?;

    // (byte range, is this a `charset=` meta?)
    let mut metas: Vec<(Range<usize>, bool)> = Vec::new();
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "meta")
    {
        let is_http_ct = n
            .attr_no_ns("http-equiv")
            .is_some_and(|v| v.eq_ignore_ascii_case("content-type"));
        let charset_attr = n.attr_no_ns("charset");
        if !is_http_ct && charset_attr.is_none() {
            continue; // not an encoding declaration
        }
        // Declared charset (from the `charset` attr, or `charset=` in `content`)
        // must be UTF-8; a non-UTF-8 declaration means we'd risk a re-encode.
        let declared = charset_attr
            .map(str::to_string)
            .or_else(|| n.attr_no_ns("content").and_then(declared_charset));
        if let Some(cs) = &declared
            && !cs.eq_ignore_ascii_case("utf-8")
        {
            return None;
        }
        metas.push((n.range(), charset_attr.is_some()));
    }

    if metas.is_empty() {
        return None;
    }
    metas.sort_by_key(|(r, _)| r.start);

    let mut edits = Vec::new();
    match metas.iter().position(|(_, is_charset)| *is_charset) {
        // An existing charset meta survives; drop every other encoding meta.
        Some(keep) => {
            for (i, (range, _)) in metas.iter().enumerate() {
                if i != keep {
                    edits.push(MetaEdit {
                        range: range.clone(),
                        replacement: String::new(),
                    });
                }
            }
        }
        // No charset meta: rewrite the first meta to the HTML5 form, drop rest.
        None => {
            for (i, (range, _)) in metas.iter().enumerate() {
                edits.push(MetaEdit {
                    range: range.clone(),
                    replacement: if i == 0 {
                        "<meta charset=\"utf-8\"/>".to_string()
                    } else {
                        String::new()
                    },
                });
            }
        }
    }
    Some(edits)
}

/// Apply non-overlapping byte-range edits to `text` (highest offset first, so
/// earlier offsets stay valid).
fn apply_edits(text: &str, mut edits: Vec<MetaEdit>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
    let mut out = text.to_string();
    for e in edits {
        out.replace_range(e.range, &e.replacement);
    }
    out
}

/// Extract the `charset=` token from an http-equiv `content` value, e.g.
/// `"text/html; charset=utf-8"` → `"utf-8"`. `None` if absent.
fn declared_charset(content: &str) -> Option<String> {
    let idx = content.to_ascii_lowercase().find("charset=")?;
    let value: String = content[idx + "charset=".len()..]
        .chars()
        .take_while(|c| !c.is_whitespace() && !matches!(c, ';' | '"' | '\'' | ',' | '>'))
        .collect();
    (!value.is_empty()).then_some(value)
}

/// `NCX-001`: the NCX `dtb:uid` doesn't match the package's unique identifier.
/// This finding carries no `rule`/`params`, but the `id` is unambiguous, so we
/// dispatch on it. The fix sets the NCX `<meta name="dtb:uid">` content to the
/// exact value of the OPF's unique identifier (the `dc:identifier` referenced by
/// `package/@unique-identifier`) — deterministic, single-valued, no guessing.
/// Declines if the package identifier can't be resolved or the NCX won't parse.
/// `ConfirmNeeded`.
fn ncx_dtb_uid(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut ncx_files: BTreeSet<String> = BTreeSet::new();
    for m in &report.messages {
        if m.id == "NCX-001"
            && let Some(loc) = m.location.as_deref()
        {
            ncx_files.insert(loc.to_string());
        }
    }

    let mut fixes = Vec::new();
    for file in ncx_files {
        let Some((_, old, new)) = compute_dtb_uid_edit(ws, &file) else {
            continue;
        };
        let preview = vec![Change {
            path: file.clone(),
            note: format!("set dtb:uid \"{old}\" → \"{new}\" (match the package identifier)"),
        }];
        let file_for_apply = file.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.ncx_dtb_uid",
            addresses_id: "NCX-001".to_string(),
            addresses_rule: None,
            addresses_severity: addressed_severity(report, "NCX-001", None),
            tier: Tier::ConfirmNeeded,
            title: format!("Sync the NCX dtb:uid to the package identifier in {file}"),
            rationale: "The NCX `dtb:uid` must equal the package's unique identifier — the \
                 `dc:identifier` the OPF `unique-identifier` points at. Its content is set to that \
                 exact value and nothing else in the document changes. Declined when the package \
                 identifier can't be resolved (a broken OPF), so this never guesses."
                .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some((edit, _, _)) = compute_dtb_uid_edit(ws, &file_for_apply)
                    && let Some(text) = ws.get_text(&file_for_apply)
                {
                    ws.set_text(&file_for_apply, apply_edits(&text, vec![edit]));
                }
            }),
        });
    }
    fixes
}

/// Build the single edit that rewrites the NCX `dtb:uid` to the package
/// identifier, plus the old and new values (for the preview). `None` (decline)
/// if the package id can't be resolved, the NCX won't parse / has no dtb:uid,
/// or it already matches.
fn compute_dtb_uid_edit(ws: &Workspace, file: &str) -> Option<(MetaEdit, String, String)> {
    let uid = package_unique_id(ws)?;
    let text = ws.get_text(file)?;
    let (range, old) = find_dtb_uid_meta(&text)?;
    if old.trim() == uid {
        return None; // already correct
    }
    let new_element = set_content_attr(&text[range.clone()], &uid)?;
    Some((
        MetaEdit {
            range,
            replacement: new_element,
        },
        old,
        uid,
    ))
}

/// Resolve the package's unique identifier: `container.xml` → OPF path →
/// `package/@unique-identifier` → the matching `dc:identifier`'s value (trimmed).
fn package_unique_id(ws: &Workspace) -> Option<String> {
    let container = ws.get_text("META-INF/container.xml")?;
    let opf_path = opf_path_from_container(&container)?;
    let opf = ws.get_text(&opf_path)?;
    unique_id_from_opf(&opf)
}

/// The first rootfile's `full-path` from an OCF `container.xml`.
fn opf_path_from_container(container: &str) -> Option<String> {
    parse_xml(container)?
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "rootfile")
        .and_then(|n| n.attr_no_ns("full-path"))
        .map(str::to_string)
}

/// The value of the `dc:identifier` referenced by `package/@unique-identifier`.
fn unique_id_from_opf(opf: &str) -> Option<String> {
    let doc = parse_xml(opf)?;
    // Mirror epubveri's resolution exactly (opf.rs): trim both sides of the
    // id match, and concatenate ALL descendant text, so our value is byte-for-
    // byte what epubveri compares dtb:uid against.
    let uid_id = doc.root_element().attr_no_ns("unique-identifier")?.trim();
    let value: String = doc
        .descendants()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "identifier"
                && n.attr_no_ns("id").map(str::trim) == Some(uid_id)
        })?
        .descendants()
        .filter(|t| t.is_text())
        .filter_map(|t| t.text())
        .collect();
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// The `<meta name="dtb:uid">` element's byte range and current `content`.
fn find_dtb_uid_meta(ncx: &str) -> Option<(Range<usize>, String)> {
    let doc = parse_xml(ncx)?;
    let meta = doc.descendants().find(|n| {
        n.is_element() && n.tag_name().name() == "meta" && n.attr_no_ns("name") == Some("dtb:uid")
    })?;
    Some((
        meta.range(),
        meta.attr_no_ns("content").unwrap_or("").to_string(),
    ))
}

/// Rewrite the `content="…"` value inside a single element's source text,
/// preserving quote style and every other attribute. `None` if there's no
/// quoted `content` attribute.
fn set_content_attr(element: &str, value: &str) -> Option<String> {
    let after = element.to_ascii_lowercase().find("content=")? + "content=".len();
    let quote = element[after..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let vstart = after + quote.len_utf8();
    let vend = vstart + element[vstart..].find(quote)?;
    Some(format!(
        "{}{}{}",
        &element[..vstart],
        value,
        &element[vend..]
    ))
}

/// `RSC-020` / `opf.manifest_item.unencoded_space_in_href`: a manifest `item`
/// whose `href` contains a raw space. An `href` is a URL, and a space is not a
/// legal URL character — it must be percent-encoded. The **file keeps its name**
/// (spaces in ZIP entry names are fine); only the reference is spelled
/// correctly, and `%20` resolves back to exactly the same entry. Nothing else in
/// the href is touched — we encode the reported defect, not everything that
/// *could* be encoded. `AutoSafe`.
fn manifest_href_spaces(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // opf path -> the hrefs epubveri flagged in it (params[0]), deduplicated.
    let mut by_opf: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.manifest_item.unencoded_space_in_href") {
            continue;
        }
        let (Some(opf), Some(href)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        by_opf
            .entry(opf.to_string())
            .or_default()
            .insert(href.clone());
    }

    let mut fixes = Vec::new();
    for (opf, hrefs) in by_opf {
        let Some(text) = ws.get_text(&opf) else {
            continue;
        };
        let edits = plan_href_encoding(&text, &hrefs);
        if edits.is_empty() {
            continue; // nothing we could locate — decline rather than guess
        }

        let preview: Vec<Change> = hrefs
            .iter()
            .filter(|h| h.contains(' '))
            .map(|h| Change {
                path: opf.clone(),
                note: format!("encode href \"{h}\" → \"{}\"", h.replace(' ', "%20")),
            })
            .collect();
        let n = edits.len();
        let opf_for_apply = opf.clone();
        let hrefs_for_apply = hrefs.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.manifest_href_spaces",
            addresses_id: "RSC-020".to_string(),
            addresses_rule: Some("opf.manifest_item.unencoded_space_in_href"),
            addresses_severity: addressed_severity(
                report,
                "RSC-020",
                Some("opf.manifest_item.unencoded_space_in_href"),
            ),
            tier: Tier::AutoSafe,
            title: format!(
                "Percent-encode {n} manifest href{} containing spaces in {opf}",
                if n == 1 { "" } else { "s" }
            ),
            rationale: "A manifest `href` is a URL, and a raw space is not a legal URL character. \
                 Each flagged space becomes `%20`, which resolves to the very same file — the \
                 entry's name in the container is not changed. Only the spaces epubveri flagged \
                 are encoded; nothing else in the href is touched."
                .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&opf_for_apply) {
                    let edits = plan_href_encoding(&text, &hrefs_for_apply);
                    ws.set_text(&opf_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// The byte range of the non-whitespace span within `range`, or `None` when it
/// is all whitespace. Wrapping this span rather than the whole node is what
/// keeps a document's existing line breaks and indentation outside the new
/// element.
///
/// `raw` must be the node's **source** slice (`&text[range]`), never roxmltree's
/// decoded `text()`: the two differ in length wherever an entity reference
/// appears, and offsets measured against the decoded form would land in the
/// wrong place in the document.
fn trimmed_span(range: Range<usize>, raw: &str) -> Option<Range<usize>> {
    let lead = raw.len() - raw.trim_start().len();
    let trail = raw.len() - raw.trim_end().len();
    if lead + trail >= raw.len() {
        return None; // whitespace only — not the defect, never wrap it
    }
    Some((range.start + lead)..(range.end - trail))
}

/// `RSC-005` / `htm.epub2_dom.bare_text_in_body`: an EPUB 2 content document
/// with text sitting directly in `<body>`, which XHTML 1.1 forbids (it wants
/// block-level content there; EPUB 3 is HTML5 and allows it, hence the rule's
/// EPUB-2 scope). `params` is empty, so — like `content_type_meta` — we parse
/// the document and find the text nodes ourselves.
///
/// Wraps each run of bare text in a `<div>`, grouped per document. `<div>` and
/// not `<p>` on purpose: it claims nothing about what the text *is* (the corpus
/// shows chapter titles and converter leftovers alike), and it reproduces the
/// anonymous block a reading system already lays the text out in, so nothing
/// moves on the page. That choice of default is what makes this `ConfirmNeeded`
/// rather than `AutoSafe`.
///
/// **Whitespace-only text nodes are never wrapped** — they are the line breaks
/// between sibling elements, epubveri does not report them, and they outnumber
/// the real findings 7594 to 54 on the corpus. Wrapping them would add thousands
/// of empty `<div>`s per book.
fn bare_text_in_body(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut docs: BTreeSet<String> = BTreeSet::new();
    for m in &report.messages {
        if m.rule == Some("htm.epub2_dom.bare_text_in_body")
            && let Some(loc) = m.location.as_deref()
        {
            docs.insert(loc.to_string());
        }
    }

    let mut fixes = Vec::new();
    for doc in docs {
        let Some(text) = ws.get_text(&doc) else {
            continue;
        };
        let Some(spans) = plan_body_text_wrapping(&text) else {
            continue; // won't parse, or has no body — decline
        };
        if spans.is_empty() {
            continue;
        }

        let preview: Vec<Change> = spans
            .iter()
            .take(8)
            .map(|r| {
                let snippet: String = text[r.clone()].chars().take(48).collect();
                Change {
                    path: doc.clone(),
                    note: format!("wrap in <div>: \"{snippet}\""),
                }
            })
            .collect();
        let n = spans.len();
        let doc_for_apply = doc.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.bare_text_in_body",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("htm.epub2_dom.bare_text_in_body"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("htm.epub2_dom.bare_text_in_body"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Wrap {n} run{} of bare text in <div> in {doc}",
                if n == 1 { "" } else { "s" }
            ),
            rationale:
                "XHTML 1.1 requires `<body>` to hold block-level content, so text sitting directly \
                 in it is invalid in EPUB 2. The text itself is not altered — a `<div>` is placed \
                 around it and nothing else is touched. `<div>` rather than `<p>` because it \
                 claims nothing about what the text is, and because a reading system already lays \
                 bare text out in an anonymous block — which is what a `<div>` is — so the page \
                 does not move. Whitespace between elements is left exactly as it is."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&doc_for_apply)
                    && let Some(spans) = plan_body_text_wrapping(&text)
                {
                    let edits = spans
                        .into_iter()
                        .map(|r| MetaEdit {
                            replacement: format!("<div>{}</div>", &text[r.clone()]),
                            range: r,
                        })
                        .collect();
                    ws.set_text(&doc_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// The non-whitespace spans of every text node sitting directly in `<body>`.
/// `None` (decline) if the document doesn't parse or has no `<body>`; an empty
/// vec means there was nothing bare to wrap.
fn plan_body_text_wrapping(text: &str) -> Option<Vec<Range<usize>>> {
    let doc = parse_xml(text)?;
    let body = doc.descendants().find(|n| n.tag_name().name() == "body")?;
    let mut spans = Vec::new();
    for child in body.children() {
        if !child.is_text() {
            continue;
        }
        let range = child.range();
        // The node's own source, so entity references keep their real width.
        let Some(raw) = text.get(range.clone()) else {
            continue;
        };
        if let Some(span) = trimmed_span(range, raw) {
            spans.push(span);
        }
    }
    Some(spans)
}

/// `PKG-006`: the archive carries a `mimetype` entry, but not first. OCF wants
/// it first and stored so a reader can identify the file from its opening bytes.
///
/// Dispatches on the bare `id` — `PKG-006` has no `rule` sub-code and needs
/// none: it says one thing, and its subject is the container itself, so unlike
/// `OPF-073` there is nothing to disambiguate.
///
/// The only fixer that touches **no content at all**: not one byte of any entry,
/// `mimetype` included. Only its position and compression method change, and OCF
/// allows exactly one answer for both. Pure `AutoSafe`.
///
/// Through 0.3.2 the writer did this unconditionally, repairing the defect as a
/// side effect of producing output — no proposal, no approval. The writer now
/// preserves packaging, and this proposes the repair in the open.
fn mimetype_packaging(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    if !report.messages.iter().any(|m| m.id == "PKG-006") {
        return Vec::new();
    }
    // Nothing to move — and we will not invent a mimetype, since that asserts
    // what the file *is* rather than repairing how it is packaged.
    if ws.get_text("mimetype").is_none() {
        return Vec::new();
    }

    vec![ProposedFix {
        fix_id: "fix.mimetype_packaging",
        addresses_id: "PKG-006".to_string(),
        addresses_rule: None,
        addresses_severity: addressed_severity(report, "PKG-006", None),
        tier: Tier::AutoSafe,
        title: "Move the `mimetype` entry first in the container, stored uncompressed".to_string(),
        rationale: "OCF requires the `mimetype` entry to be the archive's first entry and to be \
             stored uncompressed, so a reading system can identify the file from its opening \
             bytes. This changes no content whatsoever — not one byte of any entry, `mimetype` \
             included — only where that entry sits and how it is compressed. Every other entry \
             keeps its original order, bytes and compression."
            .to_string(),
        preview: vec![Change {
            path: "mimetype".to_string(),
            note:
                "move to the first entry in the ZIP and store it uncompressed (contents unchanged)"
                    .to_string(),
        }],
        apply_fn: Box::new(move |ws: &mut Workspace| ws.repackage_mimetype()),
    }]
}

/// One edit per manifest `item` whose `href` is exactly one of `hrefs`: the same
/// element with its href's spaces percent-encoded. Items we can't locate are
/// skipped (no edit), never guessed at.
fn plan_href_encoding(opf: &str, hrefs: &BTreeSet<String>) -> Vec<MetaEdit> {
    let Some(doc) = parse_xml(opf) else {
        return Vec::new();
    };
    let mut edits = Vec::new();
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
    {
        let Some(href) = n.attr_no_ns("href") else {
            continue;
        };
        if !hrefs.contains(href) || !href.contains(' ') {
            continue;
        }
        let range = n.range();
        if let Some(replacement) =
            set_attr_value(&opf[range.clone()], "href", &href.replace(' ', "%20"))
        {
            edits.push(MetaEdit { range, replacement });
        }
    }
    edits
}

/// `OPF-014` / `opf.content_document.property_used_undeclared`: a content
/// document uses a feature (`scripted`, `svg`, `remote-resources`, `switch`)
/// that its manifest `item` does not declare. epubveri has already *proven* the
/// usage — it reports the property name in `params[0]` — so the fix is to make
/// the manifest say what the document demonstrably does: add the token to that
/// item's `properties`. It adds a declaration; it never touches the content.
/// Declines when the manifest item can't be located. `AutoSafe`.
fn content_properties(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // content-document path -> the property tokens it uses but doesn't declare.
    let mut by_doc: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.content_document.property_used_undeclared") {
            continue;
        }
        let (Some(doc), Some(prop)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        by_doc
            .entry(doc.to_string())
            .or_default()
            .insert(prop.clone());
    }
    if by_doc.is_empty() {
        return Vec::new();
    }

    let Some(opf_path) = opf_path(ws) else {
        return Vec::new();
    };

    let mut fixes = Vec::new();
    for (doc, props) in by_doc {
        let Some((_, before, after)) = compute_properties_edit(ws, &opf_path, &doc, &props) else {
            continue;
        };

        let preview = vec![Change {
            path: opf_path.clone(),
            note: match &before {
                Some(b) => format!("manifest item for {doc}: properties \"{b}\" → \"{after}\""),
                None => format!("manifest item for {doc}: add properties=\"{after}\""),
            },
        }];
        let opf_for_apply = opf_path.clone();
        let doc_for_apply = doc.clone();
        let props_for_apply = props.clone();
        let listed = props.iter().cloned().collect::<Vec<_>>().join(", ");

        fixes.push(ProposedFix {
            fix_id: "fix.content_properties",
            addresses_id: "OPF-014".to_string(),
            addresses_rule: Some("opf.content_document.property_used_undeclared"),
            addresses_severity: addressed_severity(
                report,
                "OPF-014",
                Some("opf.content_document.property_used_undeclared"),
            ),
            tier: Tier::AutoSafe,
            title: format!("Declare the \"{listed}\" property in the manifest item for {doc}"),
            rationale:
                "EPUB 3.3 requires a manifest item to declare the features its content document \
                 uses. epubveri found the usage in the document itself, so the declaration is not \
                 a guess: the token is added to that item's `properties` (existing tokens are \
                 kept). The content document is not touched — only the manifest is made to tell \
                 the truth about it."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some((edit, _, _)) =
                    compute_properties_edit(ws, &opf_for_apply, &doc_for_apply, &props_for_apply)
                    && let Some(text) = ws.get_text(&opf_for_apply)
                {
                    ws.set_text(&opf_for_apply, apply_edits(&text, vec![edit]));
                }
            }),
        });
    }
    fixes
}

/// The edit that adds `props` to the `properties` of the manifest item for
/// `doc`, plus the old value (if any) and the new one. `None` (decline) when the
/// OPF won't parse, no item resolves to `doc`, or every token is already there.
fn compute_properties_edit(
    ws: &Workspace,
    opf_path: &str,
    doc: &str,
    props: &BTreeSet<String>,
) -> Option<(MetaEdit, Option<String>, String)> {
    let opf = ws.get_text(opf_path)?;
    let parsed = parse_xml(&opf)?;
    let base = dir_of(opf_path);

    let item = parsed
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
        .find(|n| {
            n.attr_no_ns("href")
                .map(|h| resolve_href(&base, h))
                .as_deref()
                == Some(doc)
        })?;

    let existing = item.attr_no_ns("properties").map(str::to_string);
    let mut tokens: Vec<String> = existing
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let mut added = false;
    for p in props {
        if !tokens.iter().any(|t| t == p) {
            tokens.push(p.clone());
            added = true;
        }
    }
    if !added {
        return None; // already declared — nothing to do
    }

    let new_value = tokens.join(" ");
    let range = item.range();
    let replacement = match existing {
        Some(_) => set_attr_value(&opf[range.clone()], "properties", &new_value)?,
        None => insert_attr(&opf[range.clone()], "properties", &new_value)?,
    };
    Some((
        MetaEdit { range, replacement },
        item.attr_no_ns("properties").map(str::to_string),
        new_value,
    ))
}

/// `RSC-005` / `opf.content_document.empty_title`: an XHTML `<title>` element
/// with no text. HTML requires a non-empty title, and it is the **most common
/// defect in the corpus** — whole libraries ship generated documents whose title
/// is `<title></title>`.
///
/// The text is never invented: it comes from the book itself, in this order —
/// the **TOC label** the book already gives this document (its NCX `navLabel`
/// or nav `<a>` text), else the document's **own first heading**. When neither
/// exists, the fixer **declines** and the finding stays reported. We do not fall
/// back to the book's `dc:title`: stamping the book's name onto every chapter is
/// a guess about intent, not a repair. `ConfirmNeeded` — it adds visible
/// metadata, so the user sees the text before it goes in.
fn empty_titles(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut docs: BTreeSet<String> = BTreeSet::new();
    for m in &report.messages {
        if m.rule == Some("opf.content_document.empty_title")
            && let Some(loc) = m.location.as_deref()
        {
            docs.insert(loc.to_string());
        }
    }
    if docs.is_empty() {
        return Vec::new();
    }

    let labels = toc_labels(ws);

    let mut fixes = Vec::new();
    for doc in docs {
        let Some(text) = ws.get_text(&doc) else {
            continue;
        };
        // The book's own name for this document first; its own first heading
        // second; otherwise decline.
        let (title, source) = match labels.get(&doc) {
            Some(label) => (label.clone(), "the book's table of contents"),
            None => match first_heading_text(&text) {
                Some(h) => (h, "the document's first heading"),
                None => continue, // nothing in the book names it — never invent
            },
        };
        if plan_title_fill(&text, &title).is_none() {
            continue; // no empty <title> found (or it won't parse) — decline
        }

        let preview = vec![Change {
            path: doc.clone(),
            note: format!("set <title> to \"{title}\" (from {source})"),
        }];
        let doc_for_apply = doc.clone();
        let title_for_apply = title.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.empty_title",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("opf.content_document.empty_title"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("opf.content_document.empty_title"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!("Fill the empty <title> in {doc} with \"{title}\""),
            rationale: "An XHTML `<title>` must not be empty. The text is taken from the book \
                 itself — the label its table of contents already gives this document, or, \
                 failing that, the document's own first heading — so nothing is invented. When \
                 the book names the document nowhere, the fix is declined and the finding stays \
                 reported."
                .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&doc_for_apply)
                    && let Some(edit) = plan_title_fill(&text, &title_for_apply)
                {
                    ws.set_text(&doc_for_apply, apply_edits(&text, vec![edit]));
                }
            }),
        });
    }
    fixes
}

/// The edit that replaces an empty `<title>` element with one carrying `title`.
/// `None` when the document won't parse or its title isn't actually empty (the
/// caller's finding is stale — decline rather than overwrite real text).
fn plan_title_fill(text: &str, title: &str) -> Option<MetaEdit> {
    let doc = parse_xml(text)?;
    let node = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "title")?;
    let has_text = node
        .descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .any(|t| !t.trim().is_empty());
    if has_text {
        return None; // not empty — never overwrite existing content
    }
    Some(MetaEdit {
        range: node.range(),
        replacement: format!("<title>{}</title>", escape_xml_text(title)),
    })
}

/// The label the book's own table of contents gives each content document:
/// container path → label. Read from the NCX (`navPoint` → `navLabel/text`) and
/// from an EPUB 3 nav document (`<a href>` text). A document listed twice keeps
/// the **first** label, and only non-empty labels are kept.
pub fn toc_labels(ws: &Workspace) -> BTreeMap<String, String> {
    let mut labels: BTreeMap<String, String> = BTreeMap::new();

    let toc_files: Vec<String> = ws
        .names()
        .filter(|n| n.ends_with(".ncx") || n.ends_with(".xhtml") || n.ends_with(".html"))
        .cloned()
        .collect();

    for toc in toc_files {
        let Some(text) = ws.get_text(&toc) else {
            continue;
        };
        let Some(doc) = parse_xml(&text) else {
            continue;
        };
        let base = dir_of(&toc);

        // NCX: <navPoint><navLabel><text>Label</text></navLabel><content src="…"/>
        for np in doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "navPoint")
        {
            let src = np
                .descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "content")
                .and_then(|n| n.attr_no_ns("src"));
            let label = np
                .descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "text")
                .and_then(|n| n.text());
            if let (Some(src), Some(label)) = (src, label) {
                insert_label(&mut labels, &base, src, label);
            }
        }

        // EPUB 3 nav document: <nav …><ol><li><a href="…">Label</a>
        for a in doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "a")
        {
            let Some(href) = a.attr_no_ns("href") else {
                continue;
            };
            let label: String = a
                .descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect();
            insert_label(&mut labels, &base, href, &label);
        }
    }
    labels
}

/// Record `label` for the container path `href` resolves to, keeping the first
/// label seen and ignoring empty ones. The fragment is dropped: a TOC entry that
/// points *into* a document still names that document.
fn insert_label(labels: &mut BTreeMap<String, String>, base: &str, href: &str, label: &str) {
    let label = collapse_ws(label);
    if label.is_empty() {
        return;
    }
    let path = resolve_href(base, href);
    labels.entry(path).or_insert(label);
}

/// The label the book gives one document, if any (the audit's entry point).
pub fn toc_label_for(ws: &Workspace, doc: &str) -> Option<String> {
    toc_labels(ws).get(doc).cloned()
}

/// The text of a document's first heading (`h1`–`h6`), collapsed to one line.
/// `None` when it won't parse, has no heading, or the heading is empty (a purely
/// decorative one, e.g. a heading holding only an image).
pub fn first_heading_text(text: &str) -> Option<String> {
    let doc = parse_xml(text)?;
    let h = doc.descendants().find(|n| {
        n.is_element() && matches!(n.tag_name().name(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
    })?;
    let s: String = h
        .descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
    let s = collapse_ws(&s);
    (!s.is_empty()).then_some(s)
}

/// Trim and collapse every run of whitespace to a single space — a title is one
/// line, and generated markup indents its headings across several.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The container path of the OPF (via `container.xml`).
fn opf_path(ws: &Workspace) -> Option<String> {
    opf_path_from_container(&ws.get_text("META-INF/container.xml")?)
}

/// The directory part of a container path, `""` for a top-level entry.
fn dir_of(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..=i].to_string(),
        None => String::new(),
    }
}

/// Resolve a document-relative `href` against `base` (a directory ending in `/`)
/// into a container path: drop any fragment/query, percent-decode, and normalize
/// `.`/`..` segments — the same resolution a reading system does.
fn resolve_href(base: &str, href: &str) -> String {
    let href = href.split(['#', '?']).next().unwrap_or("");
    let joined = format!("{base}{}", percent_decode(href));
    let mut out: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// Decode `%XX` escapes (a manifest href may legitimately spell a space `%20`).
/// Invalid escapes are left as written — we decode what we understand and never
/// mangle the rest.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Rewrite an existing quoted attribute's value inside one element's source,
/// preserving quote style and every other attribute. `None` if the attribute
/// isn't there in quoted form.
fn set_attr_value(element: &str, name: &str, value: &str) -> Option<String> {
    let lower = element.to_ascii_lowercase();
    let needle = format!("{name}=");
    let mut from = 0;
    let after = loop {
        let i = lower[from..].find(&needle)? + from;
        if is_attr_boundary(element, i) {
            break i + needle.len();
        }
        from = i + needle.len();
    };
    let quote = element[after..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let vstart = after + quote.len_utf8();
    let vend = vstart + element[vstart..].find(quote)?;
    Some(format!(
        "{}{}{}",
        &element[..vstart],
        escape_xml_attr(value),
        &element[vend..]
    ))
}

/// Insert a new attribute into an element's start tag, just before its closing
/// `/>` or `>`. `None` if the element's source has no closing bracket (it always
/// does — this keeps `apply` defensive).
fn insert_attr(element: &str, name: &str, value: &str) -> Option<String> {
    let end = element.find("/>").or_else(|| element.find('>'))?;
    let head = element[..end].trim_end();
    Some(format!(
        "{head} {name}=\"{}\"{}",
        escape_xml_attr(value),
        &element[end..]
    ))
}

/// XML-escape text content: only the three characters that can end it.
fn escape_xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// XML-escape a double-quoted attribute value.
fn escape_xml_attr(s: &str) -> String {
    escape_xml_text(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_leading_digit_uuid_gets_prefix() {
        assert_eq!(
            sanitize_ncname("51100e1e-b21d-4d41").as_deref(),
            Some("id_51100e1e-b21d-4d41")
        );
    }

    #[test]
    fn sanitize_brace_guid_strips_then_prefixes() {
        assert_eq!(
            sanitize_ncname("{0F5794B8-CFD9-448B}").as_deref(),
            Some("id_0F5794B8-CFD9-448B")
        );
    }

    #[test]
    fn sanitize_colon_stripped_keeps_letter_start() {
        assert_eq!(sanitize_ncname("np:1").as_deref(), Some("np1"));
    }

    #[test]
    fn sanitize_already_valid_is_unchanged() {
        assert_eq!(sanitize_ncname("chapter1").as_deref(), Some("chapter1"));
    }

    #[test]
    fn sanitize_declines_when_nothing_usable() {
        assert_eq!(sanitize_ncname("{}"), None);
        assert_eq!(sanitize_ncname(":"), None);
    }

    #[test]
    fn make_unique_suffixes_on_collision() {
        let mut used = HashSet::new();
        used.insert("id_x".to_string());
        assert_eq!(make_unique("id_x".to_string(), &used), "id_x-2");
    }

    #[test]
    fn attr_occurrences_ignores_data_id() {
        let text = r#"<a data-id="5abc" id="5abc"/>"#;
        assert_eq!(attr_occurrences(text, "5abc"), 1);
    }

    #[test]
    fn replace_id_attr_preserves_quotes_and_spares_data_id() {
        let text = r#"<a data-id="5abc" id="5abc"/>"#;
        let out = replace_id_attr(text, "5abc", "id_5abc").unwrap();
        assert_eq!(out, r#"<a data-id="5abc" id="id_5abc"/>"#);
    }

    #[test]
    fn replace_id_attr_single_quotes() {
        let text = "<navPoint id='5abc'>";
        let out = replace_id_attr(text, "5abc", "id_5abc").unwrap();
        assert_eq!(out, "<navPoint id='id_5abc'>");
    }

    #[test]
    fn set_attr_value_rewrites_only_that_attribute() {
        let item = r#"<item id="c1" href="Text/ch 1.xhtml" media-type="application/xhtml+xml"/>"#;
        let out = set_attr_value(item, "href", "Text/ch%201.xhtml").unwrap();
        assert_eq!(
            out,
            r#"<item id="c1" href="Text/ch%201.xhtml" media-type="application/xhtml+xml"/>"#
        );
    }

    #[test]
    fn set_attr_value_ignores_a_name_that_only_ends_with_the_attribute() {
        // `xlink:href=` must not be mistaken for `href=`.
        let el = r#"<item xlink:href="a b.xhtml" href="c d.xhtml"/>"#;
        let out = set_attr_value(el, "href", "c%20d.xhtml").unwrap();
        assert_eq!(out, r#"<item xlink:href="a b.xhtml" href="c%20d.xhtml"/>"#);
    }

    #[test]
    fn insert_attr_adds_before_the_closing_bracket() {
        let item = r#"<item id="c1" href="c1.xhtml"/>"#;
        assert_eq!(
            insert_attr(item, "properties", "scripted").unwrap(),
            r#"<item id="c1" href="c1.xhtml" properties="scripted"/>"#
        );
    }

    #[test]
    fn resolve_href_normalizes_relative_paths_and_drops_the_fragment() {
        assert_eq!(
            resolve_href("OEBPS/Text/", "../Styles/../Text/ch1.xhtml#p3"),
            "OEBPS/Text/ch1.xhtml"
        );
        assert_eq!(
            resolve_href("OEBPS/", "Text/ch%201.xhtml"),
            "OEBPS/Text/ch 1.xhtml"
        );
        assert_eq!(resolve_href("", "toc.ncx"), "toc.ncx");
    }

    #[test]
    fn percent_decode_leaves_an_invalid_escape_alone() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%zz"), "100%zz");
    }

    #[test]
    fn title_fill_replaces_an_empty_title_and_escapes_the_text() {
        let doc = "<html><head><title></title></head><body/></html>";
        let edit = plan_title_fill(doc, "Tom & Jerry <1>").unwrap();
        assert_eq!(
            apply_edits(doc, vec![edit]),
            "<html><head><title>Tom &amp; Jerry &lt;1&gt;</title></head><body/></html>"
        );
    }

    #[test]
    fn title_fill_declines_when_the_title_already_has_text() {
        // Never overwrite real content, even if a stale finding says otherwise.
        let doc = "<html><head><title>Chapter 1</title></head><body/></html>";
        assert!(plan_title_fill(doc, "Something Else").is_none());
    }

    #[test]
    fn first_heading_is_collapsed_to_one_line() {
        let doc = "<html><body><h2>\n  Bölüm\n  Bir\n</h2></body></html>";
        assert_eq!(first_heading_text(doc).as_deref(), Some("Bölüm Bir"));
    }

    #[test]
    fn first_heading_declines_on_a_decorative_heading() {
        // A heading holding only an image names nothing — decline, don't invent.
        let doc = r#"<html><body><h1><img src="t.jpg"/></h1></body></html>"#;
        assert_eq!(first_heading_text(doc), None);
    }

    fn normalize(text: &str) -> Option<String> {
        plan_encoding_normalization(text).map(|edits| apply_edits(text, edits))
    }

    #[test]
    fn declared_charset_extracts_token() {
        assert_eq!(
            declared_charset("text/html; charset=utf-8").as_deref(),
            Some("utf-8")
        );
        assert_eq!(
            declared_charset("http://www.w3.org/1999/xhtml; charset=utf-8").as_deref(),
            Some("utf-8")
        );
        assert_eq!(declared_charset("text/html").as_deref(), None);
    }

    #[test]
    fn rewrites_bogus_http_equiv_to_charset_meta() {
        let doc = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title><meta content="http://www.w3.org/1999/xhtml; charset=utf-8" http-equiv="Content-Type"/></head><body/></html>"#;
        let out = normalize(doc).unwrap();
        assert!(out.contains(r#"<meta charset="utf-8"/>"#));
        assert!(!out.to_ascii_lowercase().contains("http-equiv"));
    }

    #[test]
    fn collapses_two_encoding_metas_into_one() {
        let doc = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta content="http://www.w3.org/1999/xhtml; charset=utf-8" http-equiv="Content-Type"/><meta content="text/html;charset=utf-8" http-equiv="content-type"/></head><body/></html>"#;
        let out = normalize(doc).unwrap();
        assert_eq!(out.matches(r#"<meta charset="utf-8"/>"#).count(), 1);
        assert!(!out.to_ascii_lowercase().contains("http-equiv"));
    }

    #[test]
    fn keeps_existing_charset_meta_and_drops_http_equiv() {
        let doc = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta charset="utf-8"/><meta content="text/html;charset=utf-8" http-equiv="Content-Type"/></head><body/></html>"#;
        let out = normalize(doc).unwrap();
        assert_eq!(out.matches(r#"<meta charset="utf-8"/>"#).count(), 1);
        assert!(!out.to_ascii_lowercase().contains("http-equiv"));
    }

    #[test]
    fn declines_non_utf8_charset() {
        let doc = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta content="text/html; charset=iso-8859-1" http-equiv="Content-Type"/></head><body/></html>"#;
        assert!(plan_encoding_normalization(doc).is_none());
    }

    #[test]
    fn declines_unparseable_document() {
        assert!(plan_encoding_normalization("<html><head><meta http-equiv=Content-Type").is_none());
    }

    #[test]
    fn opf_path_read_from_container() {
        let c = r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        assert_eq!(
            opf_path_from_container(c).as_deref(),
            Some("OEBPS/content.opf")
        );
    }

    #[test]
    fn unique_id_resolves_the_referenced_identifier() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="pub-id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="other">wrong</dc:identifier><dc:identifier id="pub-id">urn:uuid:ABC</dc:identifier></metadata></package>"#;
        assert_eq!(unique_id_from_opf(opf).as_deref(), Some("urn:uuid:ABC"));
    }

    #[test]
    fn find_dtb_uid_reads_current_content() {
        let ncx = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><head><meta name="dtb:uid" content="OLD-UID"/></head></ncx>"#;
        let (_, old) = find_dtb_uid_meta(ncx).unwrap();
        assert_eq!(old, "OLD-UID");
    }

    #[test]
    fn set_content_attr_swaps_value_and_keeps_other_attrs() {
        let el = r#"<meta name="dtb:uid" content="OLD" scheme="uuid"/>"#;
        let out = set_content_attr(el, "NEW").unwrap();
        assert_eq!(out, r#"<meta name="dtb:uid" content="NEW" scheme="uuid"/>"#);
    }

    #[test]
    fn set_content_attr_single_quotes() {
        let el = "<meta name='dtb:uid' content='OLD'/>";
        assert_eq!(
            set_content_attr(el, "NEW").as_deref(),
            Some("<meta name='dtb:uid' content='NEW'/>")
        );
    }

    /// Apply the wrapping the way the fixer does, so these tests exercise the
    /// real planner rather than a paraphrase of it.
    fn wrap_body_text(doc: &str) -> Option<String> {
        let spans = plan_body_text_wrapping(doc)?;
        let edits = spans
            .into_iter()
            .map(|r| MetaEdit {
                replacement: format!("<div>{}</div>", &doc[r.clone()]),
                range: r,
            })
            .collect();
        Some(apply_edits(doc, edits))
    }

    #[test]
    fn bare_text_is_wrapped_and_surrounding_whitespace_stays_put() {
        let doc = "<html><body>\n\n\nBiRiNCi BÖLÜM\n<p>x</p></body></html>";
        assert_eq!(
            wrap_body_text(doc).unwrap(),
            "<html><body>\n\n\n<div>BiRiNCi BÖLÜM</div>\n<p>x</p></body></html>"
        );
    }

    /// The one that matters: `<body>` holds 7594 whitespace-only text nodes to
    /// 54 real ones on the corpus. Wrapping them would add thousands of empty
    /// `<div>`s per book.
    #[test]
    fn whitespace_between_elements_is_never_wrapped() {
        let doc = "<html><body>\n  <p>a</p>\n\n  <p>b</p>\n</body></html>";
        assert!(plan_body_text_wrapping(doc).unwrap().is_empty());
        assert_eq!(wrap_body_text(doc).unwrap(), doc);
    }

    /// `range()` is the source span but `text()` is decoded — measuring the trim
    /// against the decoded form would slice at the wrong offset here.
    #[test]
    fn entity_references_keep_their_source_width_and_survive_verbatim() {
        let doc = "<html><body>\n a &amp; b \n<p>x</p></body></html>";
        assert_eq!(
            wrap_body_text(doc).unwrap(),
            "<html><body>\n <div>a &amp; b</div> \n<p>x</p></body></html>"
        );
    }

    #[test]
    fn several_runs_in_one_body_are_all_wrapped() {
        let doc = "<html><body>one<p>x</p>two<p>y</p>three</body></html>";
        assert_eq!(
            wrap_body_text(doc).unwrap(),
            "<html><body><div>one</div><p>x</p><div>two</div><p>y</p><div>three</div></body></html>"
        );
    }

    #[test]
    fn text_nested_inside_a_block_is_not_our_business() {
        let doc = "<html><body><p>already wrapped</p></body></html>";
        assert!(plan_body_text_wrapping(doc).unwrap().is_empty());
    }

    #[test]
    fn a_document_without_a_body_declines() {
        assert!(plan_body_text_wrapping("<html><head/></html>").is_none());
    }

    #[test]
    fn a_document_that_does_not_parse_declines() {
        assert!(plan_body_text_wrapping("<html><body>unclosed").is_none());
    }

    #[test]
    fn whitespace_only_span_is_none_but_real_text_is_trimmed() {
        assert_eq!(trimmed_span(0..3, "   "), None);
        assert_eq!(trimmed_span(10..17, "\n abc \n"), Some(12..15));
    }
}
