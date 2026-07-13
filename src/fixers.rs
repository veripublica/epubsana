//! The fix registry: turn epubveri findings into [`ProposedFix`]es.
//!
//! Each fixer keys off an epubveri message `rule` (or an unambiguous `id`) and
//! builds a proposal, or declines (returns nothing) when it can't fix a finding
//! safely. v1 ships one fixer — HTML-entity mapping (`RSC-016`), the highest-ROI
//! defect from the feasibility spike — and the registry grows from there.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ops::Range;

use epubveri::report::{Report, Severity};

use crate::{entities, Change, Goal, ProposedFix, Tier, Workspace};

/// Build the ordered list of proposals for a detection [`Report`].
pub fn plan(report: &Report, ws: &Workspace, _goal: Goal) -> Vec<ProposedFix> {
    let mut fixes = Vec::new();
    fixes.extend(html_entities(report, ws));
    fixes.extend(ncx_ncnames(report, ws));
    fixes.extend(content_type_meta(report, ws));
    fixes.extend(ncx_dtb_uid(report, ws));
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
        if m.rule == Some("opf.content_document.invalid_content_type_meta") {
            if let Some(loc) = m.location.as_deref() {
                files.insert(loc.to_string());
            }
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
                if let Some(text) = ws.get_text(&file_for_apply) {
                    if let Some(edits) = plan_encoding_normalization(&text) {
                        ws.set_text(&file_for_apply, apply_edits(&text, edits));
                    }
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

fn plan_encoding_normalization(text: &str) -> Option<Vec<MetaEdit>> {
    let doc = parse_xml(text)?;

    // (byte range, is this a `charset=` meta?)
    let mut metas: Vec<(Range<usize>, bool)> = Vec::new();
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "meta")
    {
        let is_http_ct = n
            .attribute("http-equiv")
            .is_some_and(|v| v.eq_ignore_ascii_case("content-type"));
        let charset_attr = n.attribute("charset");
        if !is_http_ct && charset_attr.is_none() {
            continue; // not an encoding declaration
        }
        // Declared charset (from the `charset` attr, or `charset=` in `content`)
        // must be UTF-8; a non-UTF-8 declaration means we'd risk a re-encode.
        let declared = charset_attr
            .map(str::to_string)
            .or_else(|| n.attribute("content").and_then(declared_charset));
        if let Some(cs) = &declared {
            if !cs.eq_ignore_ascii_case("utf-8") {
                return None;
            }
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
        if m.id == "NCX-001" {
            if let Some(loc) = m.location.as_deref() {
                ncx_files.insert(loc.to_string());
            }
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
                if let Some((edit, _, _)) = compute_dtb_uid_edit(ws, &file_for_apply) {
                    if let Some(text) = ws.get_text(&file_for_apply) {
                        ws.set_text(&file_for_apply, apply_edits(&text, vec![edit]));
                    }
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
        .and_then(|n| n.attribute("full-path"))
        .map(str::to_string)
}

/// The value of the `dc:identifier` referenced by `package/@unique-identifier`.
fn unique_id_from_opf(opf: &str) -> Option<String> {
    let doc = parse_xml(opf)?;
    // Mirror epubveri's resolution exactly (opf.rs): trim both sides of the
    // id match, and concatenate ALL descendant text, so our value is byte-for-
    // byte what epubveri compares dtb:uid against.
    let uid_id = doc.root_element().attribute("unique-identifier")?.trim();
    let value: String = doc
        .descendants()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "identifier"
                && n.attribute("id").map(str::trim) == Some(uid_id)
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
        n.is_element() && n.tag_name().name() == "meta" && n.attribute("name") == Some("dtb:uid")
    })?;
    Some((
        meta.range(),
        meta.attribute("content").unwrap_or("").to_string(),
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
}
