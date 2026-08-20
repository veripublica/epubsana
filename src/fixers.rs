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
    fixes.extend(entity_missing_semicolon(report, ws));
    fixes.extend(ncx_ncnames(report, ws));
    fixes.extend(ncx_duplicate_ids(report, ws));
    fixes.extend(ncx_play_order(report, ws));
    fixes.extend(content_type_meta(report, ws));
    fixes.extend(ncx_dtb_uid(report, ws));
    fixes.extend(manifest_href_spaces(report, ws));
    fixes.extend(ncx_content_src_spaces(report, ws));
    fixes.extend(content_properties(report, ws));
    fixes.extend(empty_titles(report, ws));
    fixes.extend(bare_text_in_body(report, ws));
    fixes.extend(anchor_name_attrs(report, ws));
    fixes.extend(empty_lang_attrs(report, ws));
    fixes.extend(lang_xmllang_mismatch(report, ws));
    fixes.extend(doctype_html5(report, ws));
    fixes.extend(doctype_xhtml11(report, ws));
    fixes.extend(manifest_dangling_items(report, ws));
    fixes.extend(spine_dangling_itemrefs(report, ws));
    fixes.extend(spine_duplicate_itemrefs(report, ws));
    fixes.extend(guide_dangling_references(report, ws));
    fixes.extend(guide_duplicate_references(report, ws));
    fixes.extend(guide_dangling_fragments(report, ws));
    fixes.extend(content_document_invalid_ids(report, ws));
    fixes.extend(content_document_duplicate_ids(report, ws));
    fixes.extend(reference_wrong_path(report, ws));
    fixes.extend(ncx_src_wrong_path(report, ws));
    fixes.extend(package_identifier(report, ws));
    fixes.extend(nested_anchors(report, ws));
    fixes.extend(epub3_attrs_in_epub2_package(report, ws));
    fixes.extend(empty_dc_date(report, ws));
    fixes.extend(empty_metadata_element(report, ws));
    fixes.extend(non_preferred_media_type(report, ws));
    fixes.extend(font_face_missing_target(report, ws));
    fixes.extend(mimetype_packaging(report, ws));
    // Future fixers append here, in a sensible confirm order — and in
    // `handled_rules()` below.
    fixes
}

/// Every epubveri `rule` some fixer in the registry knows how to address.
///
/// **"Knows how to" is not "will".** A rule listed here may still be declined on
/// a given book — that is the normal case, not the exception, and the two must
/// never be conflated: a rule missing from this list is a *coverage* gap, while a
/// listed rule that proposes nothing is a *decision*. Reading a plan's output
/// alone cannot tell them apart (a fixer that declines everywhere looks exactly
/// like a fixer that does not exist), which is precisely the confusion that made
/// this list necessary.
///
/// **Three fixers dispatch on a bare `id`, and two of those ids have since grown
/// a rule.** `NCX-001` and `PKG-006` were rule-less when their fixers were
/// written; epubveri 0.9.11 named them `ncx.uid.package_identifier_mismatch` and
/// `ocf.mimetype.not_first_entry`. The dispatch still keys on the id, because
/// our floor is 0.9.7 where the slugs do not exist — but the rules belong in
/// this list, since a census that reads it would otherwise file two rules we
/// fix under *no fixer at all*, which is precisely the mislabelling this list
/// exists to prevent. `OPF-054` is still rule-less.
///
/// **Keep this in sync with [`plan`].** Nothing enforces it at compile time; the
/// census cross-checks at runtime and reports any rule a proposal addressed that
/// is missing here, so drift announces itself on the next shelf run.
pub fn handled_rules() -> &'static [&'static str] {
    &[
        "css.font_face.missing_target",
        "htm.doctype.epub2_unrecognized_public_id",
        "htm.doctype.epub3_obsolete_public_id",
        "htm.entity.missing_semicolon",
        "htm.entity.undeclared",
        "htm.epub2_dom.nested_anchor",
        "htm.obsolete_attribute",
        "ncx.ids.duplicate_id",
        "ncx.ids.invalid_ncname",
        "ncx.play_order.duplicate",
        "ncx.play_order.gap",
        "ncx.play_order.no_origin",
        "ncx.play_order.target_mismatch",
        "ncx.uid.package_identifier_mismatch",
        "ocf.mimetype.not_first_entry",
        "opf.content_document.duplicate_id",
        "opf.content_document.empty_title",
        "opf.content_document.invalid_content_type_meta",
        "opf.content_document.lang_xmllang_mismatch",
        "opf.content_document.property_used_undeclared",
        "opf.content_document.reference_missing_resource",
        // Two shapes inside it: stray text in <body>, and an empty lang.
        "opf.content_document.schema_violation",
        "opf.guide.duplicate_reference",
        "opf.guide.reference_fragment_not_defined",
        "opf.guide.reference_missing_resource",
        "opf.manifest_item.missing_resource",
        "opf.manifest_item.non_preferred_media_type",
        "opf.manifest_item.unencoded_space_in_href",
        "opf.metadata.empty_element",
        "opf.ncx.content_src_missing_resource",
        "opf.ncx.content_src_unencoded_space",
        "opf.package.opf_identifier_not_empty",
        "opf.package.schema_violation",
        "opf.package.unique_identifier_unresolved",
        "opf.spine.duplicate_itemref",
        "opf.spine.itemref_idref_not_in_manifest",
    ]
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

/// The XML-predefined entities: their denoted character is the bare delimiter
/// itself, so an unterminated one is closed with `;`, never substituted.
const PREDEFINED_ENTITIES: [&str; 5] = ["amp", "lt", "gt", "quot", "apos"];

/// The replacement for an unterminated `&name`, or `None` to decline: the denoted
/// character when we map the name, `&name;` (close it) for a predefined entity,
/// nothing for an unrecognized name (never guessed).
fn missing_semicolon_replacement(name: &str) -> Option<String> {
    if let Some(ch) = entities::lookup(name) {
        Some(ch.to_string())
    } else if PREDEFINED_ENTITIES.contains(&name) {
        Some(format!("&{name};"))
    } else {
        None
    }
}

/// Replace every unterminated `&name` in `text` with `replacement`. A match
/// counts only where the character right after the name is neither `;` (already
/// terminated) nor a name character (`&notin;` is not an unterminated `&not`), so
/// correct references and longer entities are never touched.
fn replace_unterminated_entity(text: &str, name: &str, replacement: &str) -> String {
    let needle = format!("&{name}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(&needle) {
        let after = &rest[i + needle.len()..];
        let next = after.chars().next();
        let terminated = next == Some(';');
        let name_continues = next.is_some_and(|c| c.is_ascii_alphanumeric());
        out.push_str(&rest[..i]);
        if terminated || name_continues {
            // Not our unterminated ref — keep it verbatim and move past the `&`
            // so we don't rescan and loop.
            out.push('&');
            rest = &rest[i + 1..];
        } else {
            out.push_str(replacement);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// `RSC-016` / `htm.entity.missing_semicolon`: a named entity reference lacking
/// its closing `;` (`&nbsp` for `&nbsp;`). A `&` not closed by `;` is not
/// well-formed XML, so this is **fatal** — the document does not open — the same
/// stakes as [`html_entities`], and the sibling that completes the `htm.entity`
/// family. epubveri reports the recognized name in `params[0]`; grouped per file.
///
/// A mapped entity is replaced by the character it denotes (closes and resolves
/// it at once, DTD or not); a predefined entity (`amp`/`lt`/`gt`/`quot`/`apos`),
/// whose character *is* the bare delimiter, is closed with `;` instead;
/// an unrecognized name is declined. `AutoSafe`.
fn entity_missing_semicolon(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // file -> (name -> occurrence count), only for names we can repair.
    let mut by_file: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("htm.entity.missing_semicolon") {
            continue;
        }
        let (Some(file), Some(name)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        if missing_semicolon_replacement(name).is_none() {
            continue; // unrecognized — never guessed
        }
        *by_file
            .entry(file.to_string())
            .or_default()
            .entry(name.clone())
            .or_insert(0) += 1;
    }

    let mut fixes = Vec::new();
    for (file, names) in by_file {
        if ws.get_text(&file).is_none() {
            continue;
        }
        let distinct = names.len();
        let total: usize = names.values().sum();

        let preview: Vec<Change> = names
            .iter()
            .map(|(name, count)| {
                let repl = missing_semicolon_replacement(name).unwrap_or_default();
                Change {
                    path: file.clone(),
                    note: format!("close &{name} → \"{repl}\" ({count}×)"),
                }
            })
            .collect();

        let repls: Vec<(String, String)> = names
            .keys()
            .map(|name| (name.clone(), missing_semicolon_replacement(name).unwrap()))
            .collect();
        let file_for_apply = file.clone();
        let summary = names.keys().cloned().collect::<Vec<_>>().join(", ");

        fixes.push(ProposedFix {
            fix_id: "fix.entity_missing_semicolon",
            addresses_id: "RSC-016".to_string(),
            addresses_rule: Some("htm.entity.missing_semicolon"),
            addresses_severity: addressed_severity(
                report,
                "RSC-016",
                Some("htm.entity.missing_semicolon"),
            ),
            tier: Tier::AutoSafe,
            title: format!(
                "Close {distinct} unterminated entity reference{} ({total}×) in {file} ({summary})",
                if distinct == 1 { "" } else { "s" },
            ),
            rationale:
                "A named entity reference without its closing ';' is not well-formed XML, so the \
                 document does not open. Each recognized name is replaced by the character it \
                 denotes (which needs no DTD), or — for the XML-predefined entities, whose \
                 character is the bare delimiter itself — closed with the missing ';'. Only the \
                 unterminated occurrences are touched; a correct reference is left alone."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(mut text) = ws.get_text(&file_for_apply) {
                    for (name, repl) in &repls {
                        text = replace_unterminated_entity(&text, name, repl);
                    }
                    ws.set_text(&file_for_apply, text);
                }
            }),
        });
    }
    fixes
}

/// The one correct EPUB 2 XHTML content-document DOCTYPE.
const CANONICAL_XHTML11_DOCTYPE: &str = "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \
     \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">";

/// The byte range of the document's `<!DOCTYPE …>` declaration, bounded by the
/// DOCTYPE's own closing `>` — after a `[ … ]` internal subset if one is present,
/// never a `>` or `[` that belongs to the body. (The bound-by-the-DOCTYPE
/// discipline is the lesson of epubveri's bracket bug: a footnote `[1]` in the
/// body must not be mistaken for an internal subset.) `None` if absent.
fn doctype_span(text: &str) -> Option<Range<usize>> {
    let start = text.find("<!DOCTYPE")?;
    let rest = &text[start..];
    let gt = rest.find('>')?;
    let end_rel = match rest.find('[') {
        // An internal subset opens before the first `>`: the declaration really
        // ends at the first `>` after the matching `]`.
        Some(b) if b < gt => {
            let close = b + rest[b..].find(']')?;
            close + rest[close..].find('>')?
        }
        _ => gt,
    };
    Some(start..start + end_rel + 1)
}

/// `HTM-004` / `htm.doctype.epub3_obsolete_public_id`: an EPUB 3 content document
/// whose DOCTYPE carries a `PUBLIC` identifier. HTML5 has exactly one legal
/// doctype, `<!DOCTYPE html>`, so any public/system identifier is obsolete —
/// reduce the whole declaration to that. A doctype declares no content, so this
/// changes nothing a reader sees. Declines when the DOCTYPE has an internal
/// subset (`[ … ]`) — those declarations (entities) may be in use and HTML5's
/// doctype cannot carry them. No `params`; the DOCTYPE is located by scan.
/// `AutoSafe`.
fn doctype_html5(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let files: BTreeSet<&str> = report
        .messages
        .iter()
        .filter(|m| m.rule == Some("htm.doctype.epub3_obsolete_public_id"))
        .filter_map(|m| m.location.as_deref())
        .collect();

    let mut fixes = Vec::new();
    for file in files {
        let Some(text) = ws.get_text(file) else {
            continue;
        };
        let Some(span) = doctype_span(&text) else {
            continue;
        };
        if text[span.clone()].contains('[') {
            continue; // internal subset — leave it for a human
        }

        let file_for_apply = file.to_string();
        fixes.push(ProposedFix {
            fix_id: "fix.doctype_html5",
            addresses_id: "HTM-004".to_string(),
            addresses_rule: Some("htm.doctype.epub3_obsolete_public_id"),
            addresses_severity: addressed_severity(
                report,
                "HTM-004",
                Some("htm.doctype.epub3_obsolete_public_id"),
            ),
            tier: Tier::AutoSafe,
            title: format!("Reduce the obsolete DOCTYPE in {file} to <!DOCTYPE html>"),
            rationale:
                "HTML5 has exactly one legal doctype, <!DOCTYPE html>; an EPUB 3 document's \
                 PUBLIC identifier is obsolete. A doctype declares no content, so reducing it \
                 changes nothing a reader sees and clears the finding. The document's markup is \
                 untouched."
                    .to_string(),
            preview: vec![Change {
                path: file.to_string(),
                note: "DOCTYPE → <!DOCTYPE html>".to_string(),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(span) = doctype_span(&text)
                    && !text[span.clone()].contains('[')
                {
                    let mut out = String::with_capacity(text.len());
                    out.push_str(&text[..span.start]);
                    out.push_str("<!DOCTYPE html>");
                    out.push_str(&text[span.end..]);
                    ws.set_text(&file_for_apply, out);
                }
            }),
        });
    }
    fixes
}

/// `HTM-004` / `htm.doctype.epub2_unrecognized_public_id`: an EPUB 2 content
/// document whose DOCTYPE is not one EPUB 2 recognizes (XHTML 1.1 or OEB 1.2).
///
/// **Deliberately narrow.** The recognized set is only XHTML 1.1, so this finding
/// also fires on a document declaring a *different, legitimate* DTD — XHTML 1.0, a
/// bare `<!DOCTYPE html>`, OEB. Relabeling those to 1.1 is not a safe rename (1.0
/// permits constructs 1.1 removed, e.g. `name=` anchors), and proving a document
/// is already valid 1.1 is the detector's job, not ours. So the fixer canonicalizes
/// **only** a malformed XHTML 1.1 identifier — one whose text names XHTML 1.1 (or
/// the `xhtml11.dtd` system id) but mistypes the exact recognized string — where
/// the author's intent is unambiguous. Anything else is declined and stays
/// reported. `ConfirmNeeded` (it edits the declared document type).
fn doctype_xhtml11(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let files: BTreeSet<&str> = report
        .messages
        .iter()
        .filter(|m| m.rule == Some("htm.doctype.epub2_unrecognized_public_id"))
        .filter_map(|m| m.location.as_deref())
        .collect();

    let mut fixes = Vec::new();
    for file in files {
        let Some(text) = ws.get_text(file) else {
            continue;
        };
        let Some(span) = doctype_span(&text) else {
            continue;
        };
        if !is_malformed_xhtml11(&text[span.clone()]) {
            continue; // a genuinely different DTD — never relabel it
        }

        let file_for_apply = file.to_string();
        fixes.push(ProposedFix {
            fix_id: "fix.doctype_xhtml11",
            addresses_id: "HTM-004".to_string(),
            addresses_rule: Some("htm.doctype.epub2_unrecognized_public_id"),
            addresses_severity: addressed_severity(
                report,
                "HTM-004",
                Some("htm.doctype.epub2_unrecognized_public_id"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!("Canonicalize the malformed XHTML 1.1 DOCTYPE in {file}"),
            rationale:
                "The DOCTYPE names XHTML 1.1 but mistypes the exact identifier EPUB 2 requires, so \
                 the author's intent is unambiguous and the canonical form is the one correct \
                 spelling. A document declaring a genuinely different DTD is declined instead, \
                 since relabeling it would assert a content model epubsana cannot verify."
                    .to_string(),
            preview: vec![Change {
                path: file.to_string(),
                note: "DOCTYPE → canonical XHTML 1.1".to_string(),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(span) = doctype_span(&text)
                    && is_malformed_xhtml11(&text[span.clone()])
                {
                    let mut out = String::with_capacity(text.len());
                    out.push_str(&text[..span.start]);
                    out.push_str(CANONICAL_XHTML11_DOCTYPE);
                    out.push_str(&text[span.end..]);
                    ws.set_text(&file_for_apply, out);
                }
            }),
        });
    }
    fixes
}

/// A DOCTYPE that clearly *intends* XHTML 1.1 but isn't the exact recognized
/// identifier: it names the 1.1 version, or points at the `xhtml11.dtd`, yet
/// (having been flagged) doesn't carry the canonical string. An internal subset
/// declines it — canonicalizing would drop `[ … ]` declarations that may be used.
fn is_malformed_xhtml11(doctype: &str) -> bool {
    if doctype.contains('[') {
        return false;
    }
    let names_11 =
        doctype.to_ascii_uppercase().contains("XHTML 1.1") || doctype.contains("xhtml11.dtd");
    names_11 && !doctype.contains("-//W3C//DTD XHTML 1.1//EN")
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

/// `RSC-005` / `ncx.ids.duplicate_id`: two or more NCX elements share an `id`.
/// Keep the first occurrence; rename every later one to a fresh unique id (the
/// value suffixed `-2`, `-3`, … until unique). NCX ids are not IDREF targets, so
/// no reference is rewritten. Disjoint from [`ncx_ncnames`] by construction — that
/// fixer only touches ids occurring exactly once, a duplicate occurs more than
/// once — so plan-once is sound. `ConfirmNeeded` (a visible id change).
fn ncx_duplicate_ids(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // file -> ordered, de-duplicated reported id values.
    let mut by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("ncx.ids.duplicate_id") {
            continue;
        }
        let (Some(file), Some(dup)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        let list = by_file.entry(file.to_string()).or_default();
        if !list.contains(dup) {
            list.push(dup.clone());
        }
    }

    let mut fixes = Vec::new();
    for (file, dups) in by_file {
        let Some(text) = ws.get_text(&file) else {
            continue;
        };
        let mut used = existing_ids(&text);
        // Plan the fixed new-id per later occurrence, so apply is deterministic
        // and robust to any earlier edit of this NCX.
        let mut plan: Vec<(String, Vec<String>)> = Vec::new();
        for dup in &dups {
            let occ = attr_occurrences(&text, dup);
            if occ < 2 {
                continue; // stale finding — nothing duplicated
            }
            let news: Vec<String> = (1..occ)
                .map(|_| {
                    let new = make_unique(dup.clone(), &used);
                    used.insert(new.clone());
                    new
                })
                .collect();
            plan.push((dup.clone(), news));
        }
        if plan.is_empty() {
            continue;
        }

        let preview: Vec<Change> = plan
            .iter()
            .map(|(dup, news)| Change {
                path: file.clone(),
                note: format!(
                    "rename {} duplicate NCX id(s) \"{dup}\" → {news:?}",
                    news.len()
                ),
            })
            .collect();
        let n: usize = plan.iter().map(|(_, news)| news.len()).sum();
        let plan_for_apply = plan.clone();
        let file_for_apply = file.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.ncx_duplicate_id",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("ncx.ids.duplicate_id"),
            addresses_severity: addressed_severity(report, "RSC-005", Some("ncx.ids.duplicate_id")),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Make {n} duplicate NCX id{} unique in {file}",
                if n == 1 { "" } else { "s" },
            ),
            rationale:
                "Two or more NCX elements share an id. The first keeps it; each later duplicate is \
                 renamed to a unique value. NCX ids are not referenced by IDREF anywhere in an \
                 EPUB, so this rewrites no reference and cannot introduce a dangling one."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(mut text) = ws.get_text(&file_for_apply) {
                    for (dup, news) in &plan_for_apply {
                        text = rename_later_id_occurrences(&text, dup, news);
                    }
                    ws.set_text(&file_for_apply, text);
                }
            }),
        });
    }
    fixes
}

/// Rename every boundary-checked `id="dup"` occurrence **after the first** to
/// `news[k]` in order; the first is left as-is. `news` has one entry per later
/// occurrence (see the planning in [`ncx_duplicate_ids`]).
fn rename_later_id_occurrences(text: &str, dup: &str, news: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pos = 0usize;
    let mut seen = 0usize;
    loop {
        // Earliest boundary-checked `id="dup"` / `id='dup'` at or after `pos`.
        let mut best: Option<(usize, char, usize)> = None;
        for quote in ['"', '\''] {
            let needle = format!("id={quote}{dup}{quote}");
            let mut from = pos;
            while let Some(rel) = text[from..].find(&needle) {
                let start = from + rel;
                if is_attr_boundary(text, start) {
                    if best.is_none_or(|(b, _, _)| start < b) {
                        best = Some((start, quote, needle.len()));
                    }
                    break;
                }
                from = start + needle.len();
            }
        }
        let Some((start, quote, len)) = best else {
            break;
        };
        out.push_str(&text[pos..start]);
        if seen == 0 {
            out.push_str(&text[start..start + len]); // keep the first
        } else if let Some(new) = news.get(seen - 1) {
            out.push_str(&format!("id={quote}{new}{quote}"));
        } else {
            out.push_str(&text[start..start + len]); // more occurrences than planned — leave it
        }
        seen += 1;
        pos = start + len;
    }
    out.push_str(&text[pos..]);
    out
}

/// One edit per element carrying both `lang` and `xml:lang` where the pair is one
/// epubveri reported and **exactly one side is empty**: the same element with the
/// empty side filled from the populated one.
///
/// A pair with both sides populated is skipped here rather than filtered by the
/// caller, so the decline lives next to the comparison that justifies it.
fn plan_lang_agreement_edits(text: &str, pairs: &BTreeSet<(String, String)>) -> Vec<MetaEdit> {
    const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
    let Some(doc) = parse_xml(text) else {
        return Vec::new();
    };
    let mut edits = Vec::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        let (Some(lang), Some(xml_lang)) = (n.attr_no_ns("lang"), n.attribute((XML_NS, "lang")))
        else {
            continue;
        };
        // epubveri trims both sides before comparing and before reporting, so
        // match on the trimmed values or a padded attribute never lines up.
        let (lang, xml_lang) = (lang.trim(), xml_lang.trim());
        if !pairs.contains(&(lang.to_string(), xml_lang.to_string())) {
            continue;
        }
        // Which attribute to write, and what to write into it. Both populated is
        // two real claims about the text's language and is left to a human.
        let (attr, value) = match (lang.is_empty(), xml_lang.is_empty()) {
            (true, false) => ("lang", xml_lang),
            (false, true) => ("xml:lang", lang),
            _ => continue,
        };
        let range = n.range();
        if let Some(replacement) = set_attr_value(&text[range.clone()], attr, value) {
            edits.push(MetaEdit { range, replacement });
        }
    }
    edits
}

/// `RSC-005` / `opf.content_document.lang_xmllang_mismatch`: one element carries
/// both `lang` and `xml:lang` and they disagree. When **exactly one is empty**,
/// the other's value is written into it. `AutoSafe`.
///
/// **Why the empty case is not a choice.** An empty attribute states no language
/// — HTML reads `lang=""` as *unknown*, which is the absence of a claim rather
/// than a competing one — so filling it from its populated sibling destroys
/// nothing and makes the element say once what it already said. The book's
/// declared language does not change. Same reasoning that made [`empty_lang_attrs`]
/// fill an empty root language instead of deleting it.
///
/// **It declines when both values are populated, and that is the whole judgement
/// here.** `lang="en"` against `xml:lang="fr"` is two real claims, and picking one
/// is an editorial statement about what language the text is in. We cannot know
/// that and will not guess it.
///
/// **EPUB 3 only, upstream.** epubcheck asserts the agreement in
/// `epub-xhtml-30.sch` and XHTML 1.1 declares the two attributes independently,
/// so epubveri never reports this on an EPUB 2 book and no version read is needed
/// here. (Contrast `fix.content_properties`, which does need one — there the rule
/// fires at both versions and only the *repair* is version-specific.)
///
/// **Measured (375 books, 2026-08-20):** 4 findings in 1 book, all of them
/// `lang="tr"` against an empty `xml:lang`. The both-populated shape does not
/// occur on this shelf, so the decline is carried by argument and a unit test.
fn lang_xmllang_mismatch(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // content-document path -> the (lang, xml:lang) pairs epubveri flagged in it.
    let mut by_doc: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.content_document.lang_xmllang_mismatch") {
            continue;
        }
        let (Some(doc), Some(lang), Some(xml_lang)) =
            (m.location.as_deref(), m.params.first(), m.params.get(1))
        else {
            continue;
        };
        by_doc
            .entry(doc.to_string())
            .or_default()
            .insert((lang.clone(), xml_lang.clone()));
    }

    let mut fixes = Vec::new();
    for (doc, pairs) in by_doc {
        let Some(text) = ws.get_text(&doc) else {
            continue;
        };
        let edits = plan_lang_agreement_edits(&text, &pairs);
        if edits.is_empty() {
            continue; // both sides populated, or nothing we could locate
        }

        let preview: Vec<Change> = pairs
            .iter()
            .filter_map(|(lang, xml_lang)| {
                let note = match (lang.is_empty(), xml_lang.is_empty()) {
                    (true, false) => format!("fill empty lang from xml:lang=\"{xml_lang}\""),
                    (false, true) => format!("fill empty xml:lang from lang=\"{lang}\""),
                    _ => return None,
                };
                Some(Change {
                    path: doc.clone(),
                    note,
                })
            })
            .collect();
        let n = edits.len();
        let doc_for_apply = doc.clone();
        let pairs_for_apply = pairs.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.lang_xmllang_mismatch",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("opf.content_document.lang_xmllang_mismatch"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("opf.content_document.lang_xmllang_mismatch"),
            ),
            tier: Tier::AutoSafe,
            title: format!(
                "Make lang and xml:lang agree on {n} element{} in {doc}",
                if n == 1 { "" } else { "s" }
            ),
            rationale: "One of the two attributes is empty, which states no language at all \
                 rather than a competing one, so it is filled from its populated sibling. The \
                 language the book declares does not change — the element stops contradicting \
                 itself. An element where both values are populated is two real claims about \
                 the text, and is left untouched."
                .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&doc_for_apply) {
                    let edits = plan_lang_agreement_edits(&text, &pairs_for_apply);
                    ws.set_text(&doc_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// `RSC-005` — the four `playOrder` faults, repaired by one correct assignment.
///
/// epubveri reports them separately and they interlock: `ncx.play_order.duplicate`
/// (different targets sharing a number), `ncx.play_order.target_mismatch` (one
/// target reached by elements carrying different numbers),
/// `ncx.play_order.gap` (a number with no predecessor) and
/// `ncx.play_order.no_origin` (nothing carries `playOrder="1"`, so the sequence
/// never starts). Satisfying any one of
/// them naively breaks another, so this renumbers the whole NCX the way the
/// format defines: **1-based, dense, in document order, and elements naming the
/// same target share the first number that target was given.**
///
/// **`no_origin` was absent from the dispatch list until 2026-08-20 and the
/// repair never changed.** A 1-based dense renumbering starts at 1 by
/// construction, so the fix had always covered the fault; it simply never ran on
/// a book whose *only* fault was the missing origin — two on the shelf, each a
/// single `<navPoint>` carrying `playOrder="0"`. Right logic, incomplete trigger,
/// and no test could see it because every test supplied one of the other three.
/// **When a family's repair is one function, audit the dispatch list against the
/// detector's rule list, not against the repair.**
///
/// **That last clause is why this fixer was rewritten.** It used to number every
/// `playOrder` by its position in the file, which is unique but target-blind — on
/// a book whose navigation reaches one position by two routes it would have
/// *created* `target_mismatch`. No shelf book had that shape, so nothing showed;
/// the defect was found by reading epubveri's rule, which skips a repeated number
/// when all its holders share a target ("one position, reached by several routes —
/// legitimate").
///
/// `playOrder` is only a hint; the real reading order is the spine, untouched.
/// `ConfirmNeeded` (rewrites values broadly).
fn ncx_play_order(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let files: BTreeSet<&str> = report
        .messages
        .iter()
        .filter(|m| {
            matches!(
                m.rule,
                Some("ncx.play_order.duplicate")
                    | Some("ncx.play_order.target_mismatch")
                    | Some("ncx.play_order.gap")
                    | Some("ncx.play_order.no_origin")
            )
        })
        .filter_map(|m| m.location.as_deref())
        .collect();

    let mut fixes = Vec::new();
    for file in files {
        let Some(text) = ws.get_text(file) else {
            continue;
        };
        let (renumbered, count) = renumber_play_order(&text);
        if count == 0 || renumbered == text {
            continue;
        }

        let file_for_apply = file.to_string();
        fixes.push(ProposedFix {
            fix_id: "fix.ncx_play_order",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("ncx.play_order.duplicate"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("ncx.play_order.duplicate"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!("Renumber {count} NCX playOrder values in {file}"),
            rationale:
                "The NCX's playOrder values are inconsistent — repeated across different targets, \
                 disagreeing about one target, or leaving a gap. playOrder is defined to mirror \
                 document order, so every value is reassigned densely from 1 in document order, \
                 with elements naming the same target sharing the number that target was first \
                 given. That is the assignment the format defines, and it satisfies all three \
                 conditions at once. playOrder is only a hint — the reading order a system \
                 follows is the spine, which is not touched."
                    .to_string(),
            preview: vec![Change {
                path: file.to_string(),
                note: format!(
                    "renumber {count} playOrder values densely by document order, \
                     same target → same number"
                ),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply) {
                    let (out, _) = renumber_play_order(&text);
                    ws.set_text(&file_for_apply, out);
                }
            }),
        });
    }
    fixes
}

/// Replace each `playOrder="…"` value with its 1-based position in document
/// order (text order = document order, since `playOrder` lives only in opening
/// tags). Returns the new text and how many values were renumbered. Boundary-
/// checked so a longer attribute name never matches.
fn renumber_play_order(text: &str) -> (String, usize) {
    let Some(doc) = parse_xml(text) else {
        return (text.to_string(), 0);
    };
    // Document order, every element that carries a playOrder.
    let mut assigned: BTreeMap<String, u32> = BTreeMap::new();
    let mut counter = 0u32;
    let mut edits: Vec<MetaEdit> = Vec::new();
    let mut n = 0usize;

    for node in doc.descendants().filter(|d| d.is_element()) {
        let Some(attr) = node.attributes().find(|a| a.name() == "playOrder") else {
            continue;
        };
        // The target this element navigates to, if it names one. An element with
        // no `<content src>` cannot share a position with anything, so it is
        // keyed uniquely by its own byte offset.
        let target = node
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "content")
            .and_then(|c| c.attr_no_ns("src"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| format!("\u{0}{}", node.range().start));

        let value = *assigned.entry(target).or_insert_with(|| {
            counter += 1;
            counter
        });

        if let Some(span) = attr_value_span(text, attr.range()) {
            n += 1;
            edits.push(MetaEdit {
                range: span,
                replacement: value.to_string(),
            });
        }
    }
    if edits.is_empty() {
        return (text.to_string(), 0);
    }
    (apply_edits(text, edits), n)
}

/// The span of an attribute's *value* given the whole `name="value"` range.
fn attr_value_span(text: &str, attr: Range<usize>) -> Option<Range<usize>> {
    let raw = text.get(attr.clone())?;
    let open = raw.find(['"', '\''])?;
    let quote = raw.as_bytes()[open] as char;
    let close = raw[open + 1..].find(quote)? + open + 1;
    Some((attr.start + open + 1)..(attr.start + close))
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
#[derive(Clone)]
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

/// A content document made parseable by declaring the DTD-only named entities it
/// uses (`&nbsp;` under an XHTML 1.1 DOCTYPE, which roxmltree can't resolve on its
/// own) — the same reach epubveri gained in issue #23, so our structural fixers
/// see the documents it now reports. Because the injection sits in the DOCTYPE
/// (before every content element), a node's byte range in the parsed `working`
/// text maps back to the **original** text by subtracting the inserted width.
struct PreparedDoc {
    working: String,
    inject_at: usize,
    inject_len: usize,
}

impl PreparedDoc {
    fn parse(&self) -> Option<roxmltree::Document<'_>> {
        parse_xml(&self.working)
    }

    /// Map a byte range in `working` back to the original text. Every content
    /// element is after the injection point, so the mapping is a constant shift.
    fn unshift(&self, r: Range<usize>) -> Range<usize> {
        if self.inject_len == 0 || r.start < self.inject_at {
            r
        } else {
            r.start - self.inject_len..r.end - self.inject_len
        }
    }
}

/// Prepare a content document for parsing. If it already parses, this is a no-op
/// (no injection, ranges map straight through). Otherwise it declares the mappable
/// named entities the document uses inside the DOCTYPE's internal subset, so the
/// parser can resolve them — but only ever *reads* the original for the fix; the
/// declarations exist solely so nodes can be located, never in the output.
fn prepare_content_doc(text: &str) -> PreparedDoc {
    let noop = |working: String| PreparedDoc {
        working,
        inject_at: 0,
        inject_len: 0,
    };
    if parse_xml(text).is_some() {
        return noop(text.to_string());
    }
    // The mappable, non-predefined entities actually used (raw-text scan).
    let mut names: BTreeSet<&str> = BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        let after = &rest[i + 1..];
        match after.find(';') {
            Some(j) if j > 0 && j < 12 && after[..j].chars().all(|c| c.is_ascii_alphanumeric()) => {
                let name = &after[..j];
                if !PREDEFINED_ENTITIES.contains(&name) && entities::lookup(name).is_some() {
                    names.insert(name);
                }
                rest = &after[j..];
            }
            _ => break,
        }
    }
    // No mappable entity, or no DOCTYPE to declare it in — nothing we can do.
    if names.is_empty() {
        return noop(text.to_string());
    }
    let Some(span) = doctype_span(text) else {
        return noop(text.to_string());
    };
    let decls: String = names
        .iter()
        .map(|name| {
            let value: String = entities::lookup(name)
                .unwrap_or("")
                .chars()
                .map(|c| format!("&#{};", c as u32))
                .collect();
            format!("<!ENTITY {name} \"{value}\">")
        })
        .collect();
    // Insert into an existing internal subset (before its `]`), or open one just
    // before the DOCTYPE's closing `>`.
    let doctype = &text[span.clone()];
    let (at, insert) = match doctype.rfind(']') {
        Some(close) => (span.start + close, decls),
        None => (span.end - 1, format!("[{decls}]")),
    };
    let mut working = String::with_capacity(text.len() + insert.len());
    working.push_str(&text[..at]);
    working.push_str(&insert);
    working.push_str(&text[at..]);
    // If the injection didn't actually make it parse (unexpected shape), fall back
    // to the original so a fixer simply declines rather than misbehaves.
    if parse_xml(&working).is_none() {
        return noop(text.to_string());
    }
    PreparedDoc {
        working,
        inject_at: at,
        inject_len: insert.len(),
    }
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
    let prepared = prepare_content_doc(text);
    let doc = prepared.parse()?;

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
        metas.push((prepared.unshift(n.range()), charset_attr.is_some()));
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

/// One edit per NCX `<content>` whose `src` is exactly one of `srcs`: the same
/// element with its `src`'s spaces percent-encoded. Elements we can't locate are
/// skipped (no edit), never guessed at.
///
/// Every element carrying the value is edited, not the first: a single document
/// is typically named by many `<navPoint>`s, epubveri reports one finding per
/// navPoint, and they are all the same edit.
fn plan_src_encoding(ncx: &str, srcs: &BTreeSet<String>) -> Vec<MetaEdit> {
    let Some(doc) = parse_xml(ncx) else {
        return Vec::new();
    };
    let mut edits = Vec::new();
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "content")
    {
        let Some(src) = n.attr_no_ns("src") else {
            continue;
        };
        if !srcs.contains(src) || !src.contains(' ') {
            continue;
        }
        let range = n.range();
        if let Some(replacement) =
            set_attr_value(&ncx[range.clone()], "src", &src.replace(' ', "%20"))
        {
            edits.push(MetaEdit { range, replacement });
        }
    }
    edits
}

/// `RSC-020` / `opf.ncx.content_src_unencoded_space`: a `<navPoint>`'s
/// `<content src>` contains a raw space. The NCX sibling of
/// [`manifest_href_spaces`], with the same argument and the same edit — a `src`
/// is a URL, a raw space is not a legal URL character, and `%20` resolves back
/// to exactly the same container entry. The **file keeps its name**. `AutoSafe`.
///
/// **Repairing only the manifest leaves the book invalid**, which is why this
/// exists as its own fixer rather than as an extension of the other. epubveri
/// measured the three states against epubcheck 5.3.0 on a book whose one content
/// document is named `a b.xhtml`: both references raw is invalid in both tools,
/// **manifest-only encoded is still invalid**, both encoded is valid in both. Until
/// epubveri 0.9.26 the second finding did not exist, so the manifest fix looked
/// complete; it was never complete, it was unobserved.
///
/// The finding is located in the **NCX**, not the package document — the one
/// structural difference from the manifest sibling.
///
/// `PKG-006` survives this fix and should: it is a *warning* about the space in
/// the ZIP entry name itself, and the verdict is valid with it present.
fn ncx_content_src_spaces(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // ncx path -> the srcs epubveri flagged in it (params[0]), deduplicated.
    let mut by_ncx: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.ncx.content_src_unencoded_space") {
            continue;
        }
        let (Some(ncx), Some(src)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        by_ncx
            .entry(ncx.to_string())
            .or_default()
            .insert(src.clone());
    }

    let mut fixes = Vec::new();
    for (ncx, srcs) in by_ncx {
        let Some(text) = ws.get_text(&ncx) else {
            continue;
        };
        let edits = plan_src_encoding(&text, &srcs);
        if edits.is_empty() {
            continue; // nothing we could locate — decline rather than guess
        }

        let preview: Vec<Change> = srcs
            .iter()
            .filter(|s| s.contains(' '))
            .map(|s| Change {
                path: ncx.clone(),
                note: format!("encode src \"{s}\" → \"{}\"", s.replace(' ', "%20")),
            })
            .collect();
        let n = edits.len();
        let ncx_for_apply = ncx.clone();
        let srcs_for_apply = srcs.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.ncx_content_src_spaces",
            addresses_id: "RSC-020".to_string(),
            addresses_rule: Some("opf.ncx.content_src_unencoded_space"),
            addresses_severity: addressed_severity(
                report,
                "RSC-020",
                Some("opf.ncx.content_src_unencoded_space"),
            ),
            tier: Tier::AutoSafe,
            title: format!(
                "Percent-encode {n} navigation target{} containing spaces in {ncx}",
                if n == 1 { "" } else { "s" }
            ),
            rationale: "A `<content src>` is a URL, and a raw space is not a legal URL character. \
                 Each flagged space becomes `%20`, which resolves to the very same file — the \
                 entry's name in the container is not changed. The same document is usually \
                 named by several navigation points; every one carrying a flagged value is \
                 encoded, and nothing else in the src is touched."
                .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&ncx_for_apply) {
                    let edits = plan_src_encoding(&text, &srcs_for_apply);
                    ws.set_text(&ncx_for_apply, apply_edits(&text, edits));
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

/// `RSC-005` / `opf.content_document.schema_violation`, stray text directly in
/// `<body>`: an EPUB 2 content document with text sitting where XHTML 1.1 wants
/// block-level content. (EPUB 3 is HTML5 and allows it, so this only ever fires
/// on EPUB 2 — the scope comes from the grammar rather than from a version test
/// here.)
///
/// **This used to be its own rule, `htm.epub2_dom.bare_text_in_body`.** epubveri
/// deleted it when the RELAX NG grammar started reporting the same thing (its
/// dedicated EPUB 2 DOM check was duplicating the grammar), so the finding now
/// arrives inside `schema_violation` — one rule covering many unrelated
/// violations. Matching it therefore needs more than the rule name:
///
/// - `params[0]` is the **containing element's** name, and we act only on
///   `body`. Stray text in an `<ol>` is a real finding too, but its correct
///   wrapper is an `<li>`, which asserts the text is a list item — a judgement,
///   not a determinate repair. We decline everything that is not `body`.
/// - `params[0]` alone cannot separate this from the family's other shapes
///   (`element "body" is not allowed here` carries the same param), so the
///   message prefix discriminates. That is a coupling to English text and it is
///   the only discriminator the finding offers. It fails in the safe direction:
///   if epubveri ever rewords the message this fixer goes **quiet**, declining
///   rather than editing the wrong thing.
///
/// The finding gives us the file; we parse the document and find the text nodes
/// ourselves, as `content_type_meta` does. So this fixer never needed the
/// per-run `position`/`element_path` that epubveri restored in 0.9.7 — the
/// re-locate-by-predicate strategy `docs/API.md` chose keeps us independent of
/// how precisely a finding is anchored.
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
        if (is_stray_text_in_body(m)
            || is_misplaced_inline_element(m)
            || is_incomplete_container(m))
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
            addresses_rule: Some("opf.content_document.schema_violation"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("opf.content_document.schema_violation"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Wrap {n} run{} of non-block content in <div> in {doc}",
                if n == 1 { "" } else { "s" }
            ),
            rationale:
                "XHTML 1.1 requires `<body>` to hold block-level content, so text and inline \
                 elements sitting directly in it are invalid in EPUB 2. Nothing is altered — a \
                 `<div>` is placed around each run and nothing else is touched. A run is taken \
                 whole, text and inline elements together, so a line that rendered as one block \
                 still does. `<div>` rather than `<p>` because it claims nothing about what the \
                 content is, and because a reading system already lays it out in an anonymous \
                 block — which is what a `<div>` is — so the page does not move. An element XHTML \
                 1.1 does not have at all, such as `<figure>`, ends the run and is left alone: \
                 wrapping it would move its violation rather than clear it."
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

/// An attribute's span, extended backwards over the whitespace that separated it
/// from its neighbour — the span to delete when removing an attribute.
///
/// `roxmltree` gives `name="value"` exactly, so deleting that alone would leave
/// `<a href="x"  >`: legal, but it edits the tag's shape for no reason. Taking
/// the run of whitespace in front of it instead leaves the element looking as if
/// the attribute had never been written. The tag name always precedes the first
/// attribute, so this can never eat the `<a` itself.
fn attr_span_with_leading_space(text: &str, attr: Range<usize>) -> Range<usize> {
    let start = text[..attr.start]
        .trim_end_matches(|c: char| c.is_whitespace())
        .len();
    start..attr.end
}

/// Is this finding "an inline element sits where block content is required"?
///
/// Two conditions, and neither is the element's position — the message never
/// says which container it is in. That is fine, because the fixer re-locates by
/// predicate and only ever wraps children of `<body>`; an `<a>` misplaced inside
/// an `<ol>` matches here and is then simply never found, which is the safe
/// direction.
///
/// - `params[0]` is the offending element, and it must be one XHTML 1.1 actually
///   has. `figure`, `section` and `figcaption` arrive through this exact message
///   and are **excluded**: the grammar does not know them at all, so a `<div>`
///   around one would move the violation rather than clear it.
/// - `div` must be among the elements the finding lists as expected. That is the
///   detector telling us a `<div>` belongs at this position, rather than us
///   assuming it — and if it is absent, the objection is to something other than
///   block-level placement.
fn is_misplaced_inline_element(m: &epubveri::report::Message) -> bool {
    m.rule == Some("opf.content_document.schema_violation")
        && m.text.starts_with("element ")
        && m.params
            .first()
            .is_some_and(|e| XHTML11_INLINE.contains(&e.as_str()))
        && m.params.iter().skip(1).any(|p| p == "div")
}

/// The containers whose XHTML 1.1 content model **requires block content and
/// admits `<div>`** — the ones a neutral wrapper actually repairs.
///
/// `<ol>`/`<ul>` want an `<li>` and `<head>` wants a `<title>`; those wrappers
/// assert what the content *is*, which is a judgement rather than a determinate
/// repair, so they are declined. On the 125-book shelf this list is not a
/// compromise but the whole population: stray text is reported in exactly these
/// two containers and nowhere else.
const WRAPPABLE_CONTAINERS: &[&str] = &["blockquote", "body"];

/// Is this finding "the container has non-block content where the grammar wants
/// block content"?
///
/// Two messages describe that one defect from opposite ends, and both are
/// matched. A `<blockquote>` holding only text draws *stray text is not allowed
/// directly in "blockquote"* **and** *element "blockquote" has incomplete
/// content*, because its model requires at least one block child; a container
/// holding only an inline element draws just the second. Wrapping the run clears
/// whichever of them fired.
fn is_incomplete_container(m: &epubveri::report::Message) -> bool {
    m.rule == Some("opf.content_document.schema_violation")
        && m.text.contains("has incomplete content")
        && m.params
            .first()
            .is_some_and(|c| WRAPPABLE_CONTAINERS.contains(&c.as_str()))
}

/// Is this finding "stray text sits directly in a container that wants blocks"?
///
/// Two conditions, because `opf.content_document.schema_violation` is one rule
/// over a whole grammar: the message shape identifies the *kind* of violation
/// (only a text-node blame reads "stray text …"), and `params[0]` identifies the
/// container we are willing to repair. Both are required — the prefix alone
/// would match stray text in an `<ol>`, and the param alone would match any
/// other violation that happens to name `body`.
fn is_stray_text_in_body(m: &epubveri::report::Message) -> bool {
    m.rule == Some("opf.content_document.schema_violation")
        && m.params
            .first()
            .is_some_and(|c| WRAPPABLE_CONTAINERS.contains(&c.as_str()))
        && m.text.starts_with("stray text is not allowed directly in")
}

/// The spans to wrap: every maximal run of non-block content sitting directly
/// inside a [`WRAPPABLE_CONTAINERS`] element. `None` (decline) if the document
/// doesn't parse or holds no such container; an empty vec means there was
/// nothing to wrap.
///
/// The containers never overlap. A `<blockquote>` is a block element, so it ends
/// a run in whatever holds it, and its own children are walked separately.
fn plan_body_text_wrapping(text: &str) -> Option<Vec<Range<usize>>> {
    let prepared = prepare_content_doc(text);
    let doc = prepared.parse()?;
    let containers: Vec<_> = doc
        .descendants()
        .filter(|n| n.is_element() && WRAPPABLE_CONTAINERS.contains(&n.tag_name().name()))
        .collect();
    if containers.is_empty() {
        return None;
    }
    let mut spans: Vec<Range<usize>> = Vec::new();
    for container in containers {
        spans.extend(runs_in(&prepared, text, container));
    }
    spans.sort_by_key(|r| r.start);
    Some(spans)
}

/// The maximal runs of non-block content among `container`'s direct children.
fn runs_in(prepared: &PreparedDoc, text: &str, container: roxmltree::Node) -> Vec<Range<usize>> {
    let kids: Vec<_> = container.children().collect();
    let mut spans: Vec<Range<usize>> = Vec::new();
    let mut i = 0;
    while i < kids.len() {
        let BodyChild::Content(start) = classify_body_child(prepared, text, kids[i]) else {
            i += 1;
            continue;
        };
        // Extend over every following run member, stepping across the
        // whitespace between them so one rendered line becomes one <div>.
        let (mut end, mut last) = (start.end, i);
        let mut j = i + 1;
        while j < kids.len() {
            match classify_body_child(prepared, text, kids[j]) {
                BodyChild::Content(span) => {
                    end = span.end;
                    last = j;
                    j += 1;
                }
                BodyChild::Whitespace => j += 1,
                BodyChild::Barrier => break,
            }
        }
        spans.push(start.start..end);
        i = last + 1;
    }
    spans
}

/// XHTML 1.1's Inline content set — the reference standard's own list, which is
/// why it can be stated rather than guessed at. An element outside it ends a run
/// (see [`BodyChild::Barrier`]).
const XHTML11_INLINE: &[&str] = &[
    "a", "abbr", "acronym", "b", "bdo", "big", "br", "button", "cite", "code", "dfn", "em", "i",
    "img", "input", "kbd", "label", "map", "object", "q", "samp", "select", "small", "span",
    "strong", "sub", "sup", "textarea", "tt", "var",
];

/// What a child of `<body>` means to the run-builder.
enum BodyChild {
    /// Non-block content to wrap: stray text, or an inline element.
    Content(Range<usize>),
    /// Whitespace between run members — stepped over, and kept inside the
    /// wrapper so the source's own line breaks survive.
    Whitespace,
    /// Anything else. A block-level element is already valid here; an element
    /// XHTML 1.1 does not have at all (`figure`, `section`, `figcaption`) is
    /// **not** repairable by wrapping — the grammar would still not know it, so
    /// the violation would move rather than clear. Both end the run.
    Barrier,
}

fn classify_body_child(prepared: &PreparedDoc, text: &str, node: roxmltree::Node) -> BodyChild {
    if node.is_text() {
        let range = prepared.unshift(node.range());
        // The node's own source, so entity references keep their real width.
        let Some(raw) = text.get(range.clone()) else {
            return BodyChild::Whitespace;
        };
        return match trimmed_span(range, raw) {
            Some(span) => BodyChild::Content(span),
            None => BodyChild::Whitespace,
        };
    }
    if node.is_element() && XHTML11_INLINE.contains(&node.tag_name().name()) {
        return BodyChild::Content(prepared.unshift(node.range()));
    }
    BodyChild::Barrier
}

/// `RSC-005` / `htm.obsolete_attribute`, the legacy `<a name="…">` anchor: an
/// attribute XHTML 1.1 removed and epubcheck rejects. `params[0]` is the
/// attribute's name, and this fixer handles exactly one member of a family that
/// also carries `<br clear>` and other presentational leftovers — see
/// `docs/FIXERS.md` for why only the anchor has a determinate repair.
///
/// Drops `name` **only where the element already carries an `id` with the same
/// value**, i.e. where the two attributes say the same thing and every
/// `#fragment` targeting the anchor resolves through the `id` regardless. That
/// makes it a deletion which loses nothing at all, hence `AutoSafe`.
///
/// Deliberately does *not* rename `name` → `id` when there is no `id`: an `id`
/// must be a valid NCName and unique in the document, and a legacy `name` is
/// under neither constraint, so the rename can manufacture a new finding.
fn anchor_name_attrs(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut docs: BTreeSet<String> = BTreeSet::new();
    for m in &report.messages {
        if m.rule == Some("htm.obsolete_attribute")
            && m.params.first().is_some_and(|p| p == "name")
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
        let Some(spans) = plan_anchor_name_drops(&text) else {
            continue; // won't parse — decline
        };
        if spans.is_empty() {
            continue; // no <a> matches the shape we repair
        }

        let n = spans.len();
        let preview: Vec<Change> = spans
            .iter()
            .take(8)
            .map(|r| Change {
                path: doc.clone(),
                note: format!("drop redundant attribute:{}", &text[r.clone()]),
            })
            .collect();
        let doc_for_apply = doc.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.anchor_name",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("htm.obsolete_attribute"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("htm.obsolete_attribute"),
            ),
            tier: Tier::AutoSafe,
            title: format!(
                "Drop {n} legacy <a name> attribute{} in {doc}",
                if n == 1 { "" } else { "s" }
            ),
            rationale:
                "`name` on `<a>` is how a link target was declared before `id` existed; XHTML 1.1 \
                 removed it. Each of these elements already carries an `id` with the identical \
                 value, so the anchor is declared the modern way too and every `#fragment` \
                 pointing at it resolves through that `id`. Deleting the duplicate declaration \
                 loses nothing: no text, no other attribute, and nothing outside the attribute's \
                 own span is touched, and nothing that linked to the anchor moves."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&doc_for_apply)
                    && let Some(spans) = plan_anchor_name_drops(&text)
                {
                    let edits = spans
                        .into_iter()
                        .map(|range| MetaEdit {
                            range,
                            replacement: String::new(),
                        })
                        .collect();
                    ws.set_text(&doc_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// The spans to delete: every `<a>`'s `name` attribute whose value the element's
/// own `id` already carries. `None` (decline) if the document doesn't parse.
fn plan_anchor_name_drops(text: &str) -> Option<Vec<Range<usize>>> {
    let prepared = prepare_content_doc(text);
    let doc = prepared.parse()?;
    let mut spans = Vec::new();
    for node in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "a")
    {
        let Some(name) = node.attr_no_ns("name") else {
            continue;
        };
        // Not `id != name` — an absent id is its own decline, spelled out here so
        // the two cases can't be collapsed by accident later.
        if node.attr_no_ns("id") != Some(name) {
            continue;
        }
        let Some(attr) = node
            .attributes()
            .find(|a| a.namespace().is_none() && a.name() == "name")
        else {
            continue;
        };
        spans.push(attr_span_with_leading_space(
            text,
            prepared.unshift(attr.range()),
        ));
    }
    Some(spans)
}

/// Is this finding "an empty `lang` / `xml:lang`"?
///
/// Same two-part match as the stray-text fixer, for the same reason —
/// `schema_violation` spans a whole grammar. Here `params` carries both the
/// attribute and its value, so the emptiness is checked from the finding itself
/// rather than inferred: a *malformed* tag (`en_US`) is a different defect and
/// must not reach the fixer.
fn is_empty_lang(m: &epubveri::report::Message) -> bool {
    m.rule == Some("opf.content_document.schema_violation")
        && m.text.starts_with("value of attribute")
        && m.params
            .first()
            .is_some_and(|p| p == "lang" || p == "xml:lang")
        && m.params.get(1).is_some_and(|v| v.is_empty())
}

/// `RSC-005` / `opf.content_document.schema_violation`, an empty `lang` or
/// `xml:lang`: EPUB 2's grammar types them as a language tag and the empty string
/// is not one. (EPUB 3 is HTML5, where `lang=""` legally means *undetermined* —
/// so this only arises on EPUB 2, by the grammar rather than a version test.)
///
/// `ConfirmNeeded`, and the reason is worth keeping in view: the deletion looks
/// inert but is not. `<p lang="">` inside `<html lang="tr">` declares
/// *undetermined* today; with the attribute gone it inherits `tr`, which a
/// reading system acts on (hyphenation, TTS voice, CJK font selection). XHTML 1.1
/// has no valid spelling for "undetermined", so the choice is between an invalid
/// document and inheriting the parent's language — a decision about the book,
/// which is the caller's to make.
fn empty_lang_attrs(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut docs: BTreeSet<String> = BTreeSet::new();
    for m in &report.messages {
        if is_empty_lang(m)
            && let Some(loc) = m.location.as_deref()
        {
            docs.insert(loc.to_string());
        }
    }

    // Read once for the whole run: the language every filled attribute takes.
    let book_lang = sole_book_language(ws);

    let mut fixes = Vec::new();
    for doc in docs {
        let Some(text) = ws.get_text(&doc) else {
            continue;
        };
        let Some(edits) = plan_empty_lang_edits(&text, book_lang.as_deref()) else {
            continue;
        };
        if edits.is_empty() {
            continue;
        }

        let n = edits.len();
        let filled = edits.iter().filter(|e| !e.replacement.is_empty()).count();
        let preview: Vec<Change> = edits
            .iter()
            .take(8)
            .map(|e| Change {
                path: doc.clone(),
                note: if e.replacement.is_empty() {
                    format!("delete empty attribute:{}", text[e.range.clone()].trim())
                } else {
                    format!("set {}", e.replacement)
                },
            })
            .collect();
        let doc_for_apply = doc.clone();
        let lang_for_apply = book_lang.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.empty_lang",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("opf.content_document.schema_violation"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("opf.content_document.schema_violation"),
            ),
            tier: Tier::ConfirmNeeded,
            title: match (filled, n - filled) {
                (f, 0) => format!(
                    "Set {f} empty lang/xml:lang attribute{} to the book's language in {doc}",
                    if f == 1 { "" } else { "s" }
                ),
                (0, d) => format!(
                    "Delete {d} empty lang/xml:lang attribute{} in {doc}",
                    if d == 1 { "" } else { "s" }
                ),
                (f, d) => format!("Set {f} and delete {d} empty lang/xml:lang attributes in {doc}"),
            },
            rationale:
                "An empty language tag names no language, and EPUB 2's grammar does not allow it \
                 — XHTML 1.1 has no valid way to spell HTML5's \"undetermined\". On the root \
                 <html> the attribute is filled with the book's own <dc:language> rather than \
                 deleted: there is no ancestor to inherit from, so deleting would leave the \
                 document stating no language at all. The value is read out of the book, never \
                 invented, and is used only when the package declares exactly one well-formed \
                 language. Anywhere else the attribute is deleted and the element inherits its \
                 parent's language, which a reading system uses for hyphenation, text-to-speech \
                 and font selection. Nothing else in the document is touched."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&doc_for_apply)
                    && let Some(edits) = plan_empty_lang_edits(&text, lang_for_apply.as_deref())
                {
                    ws.set_text(&doc_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// Is `tag` shaped like a language tag? A **deliberately narrow** BCP-47 check:
/// a 2- or 3-letter primary subtag (ISO 639), then any number of alphanumeric
/// subtags. `tr`, `en-US`, `zh-Hant-TW` pass; `en_US` and `turkish` do not.
///
/// BCP-47 does allow 4-8 letter primary subtags, but they are reserved or
/// registered and do not occur in real EPUB metadata — whereas someone writing
/// the language's *name* into `<dc:language>` does. Being narrow costs nothing
/// and catches that, which matters because this decides whether a value is
/// written into a document: an ill-formed tag would trade an invalid empty
/// attribute for an invalid non-empty one, which is not a repair.
fn is_language_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// The one language the package declares, if it declares exactly one usable one.
///
/// `None` — and so a fall back to deleting — when the book declares none, more
/// than one (which is the *document's* root language is then editorial), an
/// empty one, or something that is not a language tag.
fn sole_book_language(ws: &Workspace) -> Option<String> {
    let opf = ws
        .names()
        .find(|n| n.ends_with(".opf"))
        .and_then(|n| ws.get_text(n))?;
    let doc = parse_xml(&opf)?;
    let metadata = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "metadata")?;
    let mut found: Option<String> = None;
    for n in metadata
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "language")
    {
        let text: String = n
            .descendants()
            .filter(|t| t.is_text())
            .filter_map(|t| t.text())
            .collect();
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        if found.is_some() {
            return None; // more than one: not ours to choose between
        }
        found = Some(text);
    }
    found.filter(|t| is_language_tag(t))
}

/// The edits for every empty-valued `lang`, plain or `xml:`-prefixed.
///
/// **On the root `<html>` the attribute is filled rather than deleted**, when the
/// book declares a single usable `<dc:language>`: there is no ancestor to inherit
/// from, so deleting would leave the document stating no language at all, while
/// filling states the one the book itself declares — read out of the book, the
/// way `empty_title` reads a title out of its table of contents. Everywhere else
/// the attribute is deleted and the element inherits from its parent.
///
/// `None` (decline) if the document doesn't parse.
///
/// One predicate covers both spellings: `roxmltree` reports the local name, so
/// `lang` and `xml:lang` are both `"lang"` and differ only in namespace — and
/// both are equally invalid when empty, so neither is special-cased.
fn plan_empty_lang_edits(text: &str, book_lang: Option<&str>) -> Option<Vec<MetaEdit>> {
    let prepared = prepare_content_doc(text);
    let doc = prepared.parse()?;
    let mut edits = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        let on_root = node.tag_name().name() == "html" && node.parent_element().is_none();
        for attr in node
            .attributes()
            .filter(|a| a.name() == "lang" && a.value().is_empty())
        {
            let range = prepared.unshift(attr.range());
            match book_lang.filter(|_| on_root) {
                // Keep the attribute, its spelling (`lang` vs `xml:lang`) and its
                // quote character; only the empty value changes.
                Some(lang) => {
                    let raw = &text[range.clone()];
                    let Some(eq) = raw.find('=') else { continue };
                    let quote = raw[eq + 1..].chars().next().unwrap_or('"');
                    edits.push(MetaEdit {
                        range,
                        replacement: format!("{}{quote}{lang}{quote}", &raw[..=eq]),
                    });
                }
                None => edits.push(MetaEdit {
                    range: attr_span_with_leading_space(text, range),
                    replacement: String::new(),
                }),
            }
        }
    }
    edits.sort_by_key(|e| e.range.start);
    Some(edits)
}

/// The `<itemref>` children of the package's `<spine>`, in document order.
fn spine_itemrefs<'a, 'i>(doc: &'a roxmltree::Document<'i>) -> Vec<roxmltree::Node<'a, 'i>> {
    doc.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "spine")
        .map(|s| {
            s.children()
                .filter(|n| n.is_element() && n.tag_name().name() == "itemref")
                .collect()
        })
        .unwrap_or_default()
}

/// Every `id` declared by a manifest `<item>`.
fn manifest_ids<'a>(doc: &'a roxmltree::Document<'_>) -> HashSet<&'a str> {
    doc.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
        .filter_map(|n| n.attr_no_ns("id"))
        .collect()
}

/// The manifest ids that declare the publication's navigation document.
///
/// `properties` is a space-separated token list, so this matches the `nav`
/// **token** — `properties="mathml"` is not a navigation document, and neither
/// is one whose value merely contains those three letters.
fn nav_item_ids(opf: &str) -> BTreeSet<String> {
    let Some(doc) = parse_xml(opf) else {
        return BTreeSet::new();
    };
    doc.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
        .filter(|n| {
            n.attr_no_ns("properties")
                .is_some_and(|p| p.split_whitespace().any(|t| t == "nav"))
        })
        .filter_map(|n| n.attr_no_ns("id").map(String::from))
        .collect()
}

/// The manifest ids epubveri reported as declaring a missing resource, **minus
/// the navigation document**, which `manifest_dangling_items` declines to drop.
///
/// The subtraction is what keeps the shared spine guard honest: the guard asks
/// "would a reading order survive every deletion these fixers could propose?",
/// so counting a deletion that will never be proposed would make it decline
/// runs that are perfectly safe.
fn dangling_item_ids<'r>(report: &'r Report, opf: &str) -> BTreeSet<&'r str> {
    let nav = nav_item_ids(opf);
    report
        .messages
        .iter()
        .filter(|m| m.rule == Some("opf.manifest_item.missing_resource"))
        .filter_map(|m| m.params.first().map(String::as_str))
        .filter(|id| !nav.contains(*id))
        .collect()
}

/// Would the book still have a reading order if **every** spine deletion this
/// pair of fixers could propose were approved?
///
/// The guard is shared, and computed against the whole book rather than one fix
/// at a time, because that is the only way it holds: two dangling items with one
/// spine entry each pass an individual check and empty the spine together. A
/// spine-less EPUB is not a repaired book — it trades these findings for a
/// differently broken book — so when nothing would survive, both fixers decline
/// everything and leave it to a human.
///
/// An `<itemref>` dies if its `idref` names a dangling manifest item (the
/// cascade) or names nothing in the manifest at all (a pre-existing `OPF-049`).
/// Those two sets are disjoint by construction — see `spine_dangling_itemrefs`.
fn spine_survives_dangling_drops(opf: &str, dangling: &BTreeSet<&str>) -> bool {
    let Some(doc) = parse_xml(opf) else {
        return false;
    };
    let ids = manifest_ids(&doc);
    spine_itemrefs(&doc).iter().any(|ir| {
        // An itemref with no `idref` at all is a different defect and not one we
        // can count on as a survivor.
        ir.attr_no_ns("idref")
            .is_some_and(|idref| !dangling.contains(idref) && ids.contains(idref))
    })
}

/// `RSC-001` / `opf.manifest_item.missing_resource`: a manifest `<item>` declares
/// a resource the container doesn't hold. The declaration is simply false, and
/// nothing in the book records what it was meant to point at — so the entry
/// cannot be repaired *into* anything. Drop it, or keep the error; there is no
/// third option a human would pick, which is what makes the fix determinate.
///
/// **The cascade travels with it, in the same proposal.** Dropping the item
/// orphans every `<itemref>` naming it (an `OPF-049` epubsana would have created
/// itself) and any legacy `<meta name="cover">` pointing at it. Those are not
/// separate decisions and are deliberately not offered as separate choices: a
/// user who approved the item and declined the spine entry would be left with a
/// worse book than they started with.
///
/// We never re-resolve the href — epubveri reports the `id` in `params[0]` and
/// the fixer finds the element by it. So "is this href remote rather than a
/// container path?" never arises here: that is epubveri's call (its `RSC-001`
/// site is guarded by `!is_external`), and a second opinion about what counts as
/// missing would make epubsana a second detector.
///
/// `ConfirmNeeded`: it is a deletion that can shorten the reading order and can
/// remove the book's cover declaration — both visible in a reading system's UI.
fn manifest_dangling_items(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // id -> the missing href, for the proposal text.
    let mut items: BTreeMap<String, String> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.manifest_item.missing_resource") {
            continue;
        }
        let (Some(id), Some(href)) = (m.params.first(), m.params.get(1)) else {
            continue;
        };
        items.insert(id.clone(), href.clone());
    }
    if items.is_empty() {
        return Vec::new();
    }
    let Some(opf_path) = opf_path(ws) else {
        return Vec::new();
    };
    let Some(opf) = ws.get_text(&opf_path) else {
        return Vec::new();
    };
    if !spine_survives_dangling_drops(&opf, &dangling_item_ids(report, &opf)) {
        return Vec::new();
    }
    let nav = nav_item_ids(&opf);

    let mut fixes = Vec::new();
    for (id, href) in items {
        // The navigation document is not droppable. A publication that declares
        // one must have it, so deleting the item clears `RSC-001` and produces
        // `opf.package.missing_nav_document` in its place — the book is still
        // invalid, and now it has no table of contents either. Measured on a
        // real book, where it was the only finding epubsana introduced across
        // the whole shelf.
        //
        // This is the spine guard's principle applied one level down: a repair
        // that trades one error for another is not a repair. Note what it does
        // *not* do — it does not ask which EPUB version this is. In an EPUB 2
        // book the `nav` property is itself invalid, and a second defect on the
        // same element is a reason for a human to look at it, not a licence for
        // us to delete it faster.
        if nav.contains(&id) {
            continue;
        }
        let Some((_, spine_drops, cover_meta)) = compute_dangling_item_edits(&opf, &id) else {
            continue;
        };

        let mut preview = vec![Change {
            path: opf_path.clone(),
            note: format!("manifest: drop item id=\"{id}\" (href=\"{href}\", missing)"),
        }];
        if spine_drops > 0 {
            preview.push(Change {
                path: opf_path.clone(),
                note: format!("spine: drop {spine_drops} itemref(s) naming \"{id}\""),
            });
        }
        if cover_meta {
            preview.push(Change {
                path: opf_path.clone(),
                note: format!("metadata: drop <meta name=\"cover\"> naming \"{id}\""),
            });
        }

        let opf_for_apply = opf_path.clone();
        let id_for_apply = id.clone();
        fixes.push(ProposedFix {
            fix_id: "fix.manifest_dangling_item",
            addresses_id: "RSC-001".to_string(),
            addresses_rule: Some("opf.manifest_item.missing_resource"),
            addresses_severity: addressed_severity(
                report,
                "RSC-001",
                Some("opf.manifest_item.missing_resource"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!("Drop the manifest item \"{id}\" — its resource \"{href}\" is missing"),
            rationale:
                "A manifest item claims a resource is part of the publication; the bytes are not \
                 in the container, so the claim is false and nothing in the book records what it \
                 was meant to point at. Every reference that named the item goes with it in the \
                 same edit — the spine entries it orphans (a position no reading system could \
                 render) and any legacy cover meta pointing at it. Nothing readable is lost: the \
                 resource was already gone before the fix."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&opf_for_apply)
                    && let Some((edits, _, _)) = compute_dangling_item_edits(&text, &id_for_apply)
                {
                    ws.set_text(&opf_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// The edits that drop the manifest `<item>` with `id` **and every reference
/// that named it**: the spine `<itemref>`s, and the legacy `<meta name="cover">`.
/// Also returns how many spine entries and whether a cover meta went with it, for
/// the preview. `None` (decline) when the OPF won't parse or no item has that id.
///
/// The ranges are distinct elements and so never overlap. Deleting exactly the
/// element's own bytes leaves the surrounding whitespace alone, which is the
/// convention every surgical fixer here follows.
fn compute_dangling_item_edits(opf: &str, id: &str) -> Option<(Vec<MetaEdit>, usize, bool)> {
    let doc = parse_xml(opf)?;

    let item = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
        .find(|n| n.attr_no_ns("id") == Some(id))?;

    let mut edits = vec![MetaEdit {
        range: item.range(),
        replacement: String::new(),
    }];

    let mut spine_drops = 0usize;
    for ir in spine_itemrefs(&doc) {
        if ir.attr_no_ns("idref") == Some(id) {
            edits.push(MetaEdit {
                range: ir.range(),
                replacement: String::new(),
            });
            spine_drops += 1;
        }
    }

    let mut cover_meta = false;
    for m in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "meta")
    {
        if m.attr_no_ns("name") == Some("cover") && m.attr_no_ns("content") == Some(id) {
            edits.push(MetaEdit {
                range: m.range(),
                replacement: String::new(),
            });
            cover_meta = true;
        }
    }

    Some((edits, spine_drops, cover_meta))
}

/// `OPF-049` / `opf.spine.itemref_idref_not_in_manifest`: a `<spine>` entry names
/// a manifest id that doesn't exist. The entry is inert — no item, so no
/// document, so nothing to render at that position. As with its sibling there is
/// no information anywhere about what it meant to name, so drop it or keep the
/// error. Deletion only; the reading order of everything remaining is unchanged.
///
/// **Why this cannot collide with `fix.manifest_dangling_item`,** which drops the
/// itemrefs it orphans itself — a real worry given that epubsana plans every fix
/// once, from the original report, and never re-plans. This fixer only ever sees
/// an `OPF-049` *from that original report*, i.e. an `idref` already absent from
/// the manifest before any fix ran; the cascade only ever touches `idref`s whose
/// item was present at plan time (the item exists — its file is missing). The two
/// sets are disjoint by construction, so plan-once is sound here rather than
/// merely lucky. Anyone adding re-planning should re-check that argument.
///
/// `ConfirmNeeded`: a deletion from the reading order, and deletions get looked at.
fn spine_dangling_itemrefs(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let idrefs: BTreeSet<String> = report
        .messages
        .iter()
        .filter(|m| m.rule == Some("opf.spine.itemref_idref_not_in_manifest"))
        .filter_map(|m| m.params.first().cloned())
        .collect();
    if idrefs.is_empty() {
        return Vec::new();
    }
    let Some(opf_path) = opf_path(ws) else {
        return Vec::new();
    };
    let Some(opf) = ws.get_text(&opf_path) else {
        return Vec::new();
    };
    if !spine_survives_dangling_drops(&opf, &dangling_item_ids(report, &opf)) {
        return Vec::new();
    }

    let mut fixes = Vec::new();
    for idref in idrefs {
        let Some(edits) = compute_dangling_itemref_edits(&opf, &idref) else {
            continue;
        };
        let n = edits.len();

        let opf_for_apply = opf_path.clone();
        let idref_for_apply = idref.clone();
        fixes.push(ProposedFix {
            fix_id: "fix.spine_dangling_itemref",
            addresses_id: "OPF-049".to_string(),
            addresses_rule: Some("opf.spine.itemref_idref_not_in_manifest"),
            addresses_severity: addressed_severity(
                report,
                "OPF-049",
                Some("opf.spine.itemref_idref_not_in_manifest"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!("Drop the spine itemref \"{idref}\" — no manifest item declares it"),
            rationale:
                "A spine itemref whose idref resolves to nothing is a pointer to a hole: there is \
                 no manifest item, therefore no document, therefore nothing a reading system can \
                 render at that position. It cannot be repaired into anything, because nothing in \
                 the book records what it was supposed to name. Every other spine entry keeps its \
                 place."
                    .to_string(),
            preview: vec![Change {
                path: opf_path.clone(),
                note: format!("spine: drop {n} itemref(s) with idref=\"{idref}\""),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&opf_for_apply)
                    && let Some(edits) = compute_dangling_itemref_edits(&text, &idref_for_apply)
                {
                    ws.set_text(&opf_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// The edits that drop every spine `<itemref>` carrying `idref`. `None`
/// (decline) when the OPF won't parse or no itemref carries it.
fn compute_dangling_itemref_edits(opf: &str, idref: &str) -> Option<Vec<MetaEdit>> {
    let doc = parse_xml(opf)?;
    let edits: Vec<MetaEdit> = spine_itemrefs(&doc)
        .iter()
        .filter(|ir| ir.attr_no_ns("idref") == Some(idref))
        .map(|ir| MetaEdit {
            range: ir.range(),
            replacement: String::new(),
        })
        .collect();
    (!edits.is_empty()).then_some(edits)
}

/// `opf.spine.duplicate_itemref`: the spine lists the same manifest item twice,
/// so a chapter appears twice in the reading order. Keep the **first**
/// occurrence, drop the later ones — the duplicate carries no information the
/// first doesn't already carry (same `idref`, same document), and the first is
/// where the document actually belongs in the sequence.
///
/// **Dispatches on the `rule`, not the `id`, and that is load-bearing.** epubveri
/// reports the identical condition as `OPF-034` in EPUB 2 but `RSC-005` in EPUB 3
/// (version-scoped, matching each epubcheck fixture). A fixer keyed on `OPF-034`
/// would silently do nothing on every EPUB 3 book — which is precisely what the
/// `rule` sub-code exists to prevent. The proposal inherits `addresses_id` from
/// the message rather than hard-coding one.
///
/// Needs no empty-spine guard, unlike its dangling siblings: the occurrence it
/// keeps is by definition still there.
///
/// `ConfirmNeeded`: a deletion, and one a reader sees — a chapter stops
/// appearing twice.
fn spine_duplicate_itemrefs(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // idref -> the id epubveri filed it under (OPF-034 or RSC-005).
    let mut dupes: BTreeMap<String, String> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.spine.duplicate_itemref") {
            continue;
        }
        let Some(idref) = m.params.first() else {
            continue;
        };
        dupes.insert(idref.clone(), m.id.to_string());
    }
    if dupes.is_empty() {
        return Vec::new();
    }
    let Some(opf_path) = opf_path(ws) else {
        return Vec::new();
    };
    let Some(opf) = ws.get_text(&opf_path) else {
        return Vec::new();
    };

    let mut fixes = Vec::new();
    for (idref, id) in dupes {
        let Some(edits) = compute_duplicate_itemref_edits(&opf, &idref) else {
            continue;
        };
        let n = edits.len();

        let opf_for_apply = opf_path.clone();
        let idref_for_apply = idref.clone();
        let addresses_id = id.clone();
        fixes.push(ProposedFix {
            fix_id: "fix.spine_duplicate_itemref",
            addresses_id: id.clone(),
            addresses_rule: Some("opf.spine.duplicate_itemref"),
            addresses_severity: addressed_severity(
                report,
                &addresses_id,
                Some("opf.spine.duplicate_itemref"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Drop {n} repeat spine entr{} for \"{idref}\" — keep the first",
                if n == 1 { "y" } else { "ies" }
            ),
            rationale:
                "The spine lists this manifest item more than once, so the document appears twice \
                 in the reading order. A later entry carries no information the first doesn't \
                 already carry — same idref, same document — and the first occurrence is where \
                 the document actually belongs in the sequence, so dropping the repeats removes a \
                 repetition, not a position. Nothing else in the spine moves."
                    .to_string(),
            preview: vec![Change {
                path: opf_path.clone(),
                note: format!("spine: drop {n} repeat itemref(s) with idref=\"{idref}\""),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&opf_for_apply)
                    && let Some(edits) = compute_duplicate_itemref_edits(&text, &idref_for_apply)
                {
                    ws.set_text(&opf_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// A spine entry's `linear`, normalized: absent means `yes`, so a bare
/// `<itemref idref="x"/>` and `<itemref idref="x" linear="yes"/>` are the same
/// entry and one may be dropped for the other.
fn linear_of(ir: &roxmltree::Node<'_, '_>) -> String {
    ir.attr_no_ns("linear")
        .map(str::trim)
        .unwrap_or("yes")
        .to_ascii_lowercase()
}

/// The edits that drop every spine `<itemref>` for `idref` **after the first**.
///
/// `None` (decline) when the OPF won't parse, fewer than two entries carry the
/// `idref` (a stale finding deletes nothing), or a repeat is not really a
/// duplicate:
///
/// - **its `linear` disagrees with the first's** — the book is saying "in the
///   reading order *and* reachable out-of-line", a real authored intent that
///   deleting would destroy. If any repeat disagrees the whole group is
///   declined rather than half-repaired;
/// - **it carries an `id` the package refines** — an `<itemref id="x">` can be
///   the target of a `<meta refines="#x">`, and dropping it would orphan that
///   metadata: a finding epubsana would have created itself.
fn compute_duplicate_itemref_edits(opf: &str, idref: &str) -> Option<Vec<MetaEdit>> {
    let doc = parse_xml(opf)?;
    let all = spine_itemrefs(&doc);
    let mut hits = all
        .iter()
        .filter(|ir| ir.attr_no_ns("idref") == Some(idref));

    let first = hits.next()?;
    let repeats: Vec<_> = hits.collect();
    if repeats.is_empty() {
        return None; // stale finding — nothing is duplicated
    }

    let keep_linear = linear_of(first);
    for r in &repeats {
        if linear_of(r) != keep_linear {
            return None; // deliberate: one linear, one not
        }
        if let Some(id) = r.attr_no_ns("id")
            && opf.contains(&format!("refines=\"#{id}\""))
        {
            return None; // dropping it would orphan a <meta refines>
        }
    }

    Some(
        repeats
            .iter()
            .map(|r| MetaEdit {
                range: r.range(),
                replacement: String::new(),
            })
            .collect(),
    )
}

/// The `<reference>` children of the OPF's `<guide>`, in document order.
fn guide_references<'a, 'i>(doc: &'a roxmltree::Document<'i>) -> Vec<roxmltree::Node<'a, 'i>> {
    doc.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "guide")
        .map(|g| {
            g.children()
                .filter(|n| n.is_element() && n.tag_name().name() == "reference")
                .collect()
        })
        .unwrap_or_default()
}

/// `RSC-007` / `opf.guide.reference_missing_resource`: a `<guide>` reference whose
/// `href` resolves to no resource in the container (on the corpus, a wrong
/// extension). It cannot be repaired *into* anything — nothing records what file
/// it meant — so drop it, as with a dangling manifest item or spine itemref.
/// epubveri reports the `href` in `params[0]`; we match on it and never re-resolve
/// paths (that is the detector's call). If dropping empties the `<guide>`, drop the
/// `<guide>` too — an empty guide is invalid and the element is optional.
/// `ConfirmNeeded` (a deletion).
fn guide_dangling_references(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut by_file: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.guide.reference_missing_resource") {
            continue;
        }
        let (Some(file), Some(href)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        by_file
            .entry(file.to_string())
            .or_default()
            .insert(href.clone());
    }

    let mut fixes = Vec::new();
    for (file, hrefs) in by_file {
        let Some(text) = ws.get_text(&file) else {
            continue;
        };
        let Some((_edits, dropped_guide, n)) = compute_guide_dangling_edits(&text, &hrefs) else {
            continue;
        };

        let file_for_apply = file.clone();
        let hrefs_for_apply = hrefs.clone();
        let listed = hrefs.iter().cloned().collect::<Vec<_>>().join(", ");
        fixes.push(ProposedFix {
            fix_id: "fix.guide_dangling_reference",
            addresses_id: "RSC-007".to_string(),
            addresses_rule: Some("opf.guide.reference_missing_resource"),
            addresses_severity: addressed_severity(
                report,
                "RSC-007",
                Some("opf.guide.reference_missing_resource"),
            ),
            tier: Tier::ConfirmNeeded,
            title: if dropped_guide {
                format!("Drop the <guide> in {file} — all its references are missing ({listed})")
            } else {
                format!("Drop {n} missing guide reference(s) in {file} ({listed})")
            },
            rationale:
                "A guide reference points at a resource that does not exist in the container, so \
                 it names a landmark no reading system can reach and nothing records what file it \
                 meant. Dropping it removes a pointer to a hole; references that still resolve keep \
                 their place. If none remain, the empty <guide> (invalid, and optional) is dropped \
                 too."
                    .to_string(),
            preview: vec![Change {
                path: file.clone(),
                note: if dropped_guide {
                    "drop the entire <guide> (no reference resolves)".to_string()
                } else {
                    format!("drop {n} <reference> element(s): {listed}")
                },
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some((edits, _, _)) =
                        compute_guide_dangling_edits(&text, &hrefs_for_apply)
                {
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// Edits dropping every `<reference>` whose `href` is in `hrefs` — or the whole
/// `<guide>` if that would empty it. Returns (edits, dropped_whole_guide, count).
/// `None` (decline) if the OPF won't parse or no reference carries a listed href.
fn compute_guide_dangling_edits(
    opf: &str,
    hrefs: &BTreeSet<String>,
) -> Option<(Vec<MetaEdit>, bool, usize)> {
    let doc = parse_xml(opf)?;
    let refs = guide_references(&doc);
    let to_drop: Vec<_> = refs
        .iter()
        .filter(|r| r.attr_no_ns("href").is_some_and(|h| hrefs.contains(h)))
        .collect();
    if to_drop.is_empty() {
        return None;
    }
    // Would every reference be dropped? Then remove the <guide> element instead.
    if to_drop.len() == refs.len() {
        let guide = doc
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "guide")?;
        return Some((
            vec![MetaEdit {
                range: guide.range(),
                replacement: String::new(),
            }],
            true,
            to_drop.len(),
        ));
    }
    let n = to_drop.len();
    let edits = to_drop
        .iter()
        .map(|r| MetaEdit {
            range: r.range(),
            replacement: String::new(),
        })
        .collect();
    Some((edits, false, n))
}

/// `RSC-017` / `opf.guide.duplicate_reference`: two or more `<guide>` references
/// share the **same `type` and `href`** — a redundant repeat carrying no
/// information the first doesn't. Keep the first of each identical pair, drop the
/// later ones. Cannot empty the guide (a first is always kept). `ConfirmNeeded`.
fn guide_duplicate_references(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let files: BTreeSet<&str> = report
        .messages
        .iter()
        .filter(|m| m.rule == Some("opf.guide.duplicate_reference"))
        .filter_map(|m| m.location.as_deref())
        .collect();

    let mut fixes = Vec::new();
    for file in files {
        let Some(text) = ws.get_text(file) else {
            continue;
        };
        let Some(edits) = compute_guide_duplicate_edits(&text) else {
            continue;
        };
        let n = edits.len();

        let file_for_apply = file.to_string();
        fixes.push(ProposedFix {
            fix_id: "fix.guide_duplicate_reference",
            addresses_id: "RSC-017".to_string(),
            addresses_rule: Some("opf.guide.duplicate_reference"),
            addresses_severity: addressed_severity(
                report,
                "RSC-017",
                Some("opf.guide.duplicate_reference"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!("Drop {n} duplicate guide reference(s) in {file}"),
            rationale:
                "Two or more guide references share the same type and href, so the later ones name \
                 the same landmark at the same target as the first and carry no new information. \
                 The first of each pair is kept; the redundant repeats are dropped."
                    .to_string(),
            preview: vec![Change {
                path: file.to_string(),
                note: format!("drop {n} duplicate <reference> element(s) (same type + href)"),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(edits) = compute_guide_duplicate_edits(&text)
                {
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// Edits dropping every `<guide>` reference after the first of each identical
/// `(type, href)` pair. `None` (decline) if the OPF won't parse or nothing repeats.
fn compute_guide_duplicate_edits(opf: &str) -> Option<Vec<MetaEdit>> {
    let doc = parse_xml(opf)?;
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut edits = Vec::new();
    for r in guide_references(&doc) {
        // Only references with a `type` are considered (matching epubveri).
        let (Some(ty), Some(href)) = (r.attr_no_ns("type"), r.attr_no_ns("href")) else {
            continue;
        };
        let key = (ty.to_string(), href.to_string());
        if !seen.insert(key) {
            edits.push(MetaEdit {
                range: r.range(),
                replacement: String::new(),
            });
        }
    }
    (!edits.is_empty()).then_some(edits)
}

/// `RSC-012` / `opf.guide.reference_fragment_not_defined` (epubveri 0.9.16+): a
/// `<guide>` reference's `#fragment` resolves to no `id` in a target document
/// that **does** exist. Drop the fragment and keep the path: the reference goes
/// on naming the same document and stops claiming a position inside it.
///
/// This is the one member of the family that deletes nothing reachable. A
/// fragment resolving to no `id` already takes a reading system to the top of the
/// document — exactly where the fragment-less href lands — so the edit writes
/// down the behaviour the book already has, and the author's real choice (*which*
/// document is the landmark) is untouched. Dropping the whole reference, as the
/// two sibling fixers do, would throw away working navigation; retargeting the
/// fragment at some other `id` would be inventing.
///
/// `params[0]` is the fragment and `params[1]` the resolved target path. Both are
/// matched, so a reference is only edited when it is the one epubveri blamed.
/// `ConfirmNeeded`.
fn guide_dangling_fragments(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // file -> the (fragment, resolved target) pairs flagged in it.
    let mut by_file: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.guide.reference_fragment_not_defined") {
            continue;
        }
        let (Some(file), Some(frag), Some(target)) =
            (m.location.as_deref(), m.params.first(), m.params.get(1))
        else {
            continue;
        };
        by_file
            .entry(file.to_string())
            .or_default()
            .insert((frag.clone(), target.clone()));
    }

    let mut fixes = Vec::new();
    for (file, flagged) in by_file {
        let Some(text) = ws.get_text(&file) else {
            continue;
        };
        let Some(rewrites) = compute_guide_fragment_edits(&text, &file, &flagged) else {
            continue;
        };
        let n = rewrites.len();
        let listed = rewrites
            .iter()
            .map(|(from, to, _)| format!("{from} → {to}"))
            .collect::<Vec<_>>()
            .join(", ");

        let file_for_apply = file.clone();
        let flagged_for_apply = flagged.clone();
        fixes.push(ProposedFix {
            fix_id: "fix.guide_dangling_fragment",
            addresses_id: "RSC-012".to_string(),
            addresses_rule: Some("opf.guide.reference_fragment_not_defined"),
            addresses_severity: addressed_severity(
                report,
                "RSC-012",
                Some("opf.guide.reference_fragment_not_defined"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!("Drop {n} dangling guide fragment(s) in {file} ({listed})"),
            rationale:
                "A guide reference points into a document that exists, at a fragment that document \
                 does not define — typically an anchor left behind by a conversion. It already \
                 takes a reader to the top of that document, because there is nothing else for it \
                 to resolve to, so dropping the fragment changes no behaviour: it makes the file \
                 say what already happens. The path, and with it the landmark's target document, \
                 is kept exactly as written. Nothing in the book records the position the fragment \
                 meant, so it is not repointed — that would be a guess."
                    .to_string(),
            preview: rewrites
                .iter()
                .take(6)
                .map(|(from, to, _)| Change {
                    path: file.clone(),
                    note: format!("rewrite href {from} → {to}"),
                })
                .collect(),
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(rewrites) =
                        compute_guide_fragment_edits(&text, &file_for_apply, &flagged_for_apply)
                {
                    let edits = rewrites.into_iter().map(|(_, _, edit)| edit).collect();
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// The href rewrites clearing every flagged dangling fragment in one OPF, as
/// `(href before, href after, edit)`. `None` (decline) if the OPF won't parse or
/// no reference matches a flagged `(fragment, target)` pair.
///
/// **The collision guard is the reason this returns the whole post-edit guide's
/// worth of state rather than one edit at a time.** Dropping a fragment can make
/// a reference identical to another one — clearing an `RSC-012` by creating an
/// `RSC-017` (`opf.guide.duplicate_reference`), which leaves the book no better.
/// A reference whose post-edit `(type, href)` pair is already claimed is left
/// alone; every other flagged reference in the same guide is still repaired. The
/// pairs are collected over the whole guide **after** the edits, so two flagged
/// references that would collide with *each other* are both declined rather than
/// silently merged.
#[allow(clippy::type_complexity)]
fn compute_guide_fragment_edits(
    opf: &str,
    opf_path: &str,
    flagged: &BTreeSet<(String, String)>,
) -> Option<Vec<(String, String, MetaEdit)>> {
    let doc = parse_xml(opf)?;
    let base = dir_of(opf_path);
    let refs = guide_references(&doc);

    // Every reference that epubveri blamed, with the href it would become.
    let mut candidates: Vec<(roxmltree::Node, String, String)> = Vec::new();
    for r in &refs {
        let Some(href) = r.attr_no_ns("href") else {
            continue;
        };
        let Some((path, frag)) = href.split_once('#') else {
            continue;
        };
        if !flagged.contains(&(frag.to_string(), resolve_href(&base, href))) {
            continue;
        }
        candidates.push((*r, href.to_string(), path.to_string()));
    }
    if candidates.is_empty() {
        return None;
    }

    // The `(type, href)` pairs the guide would hold once every candidate is
    // rewritten — the set a collision is measured against.
    let dropping: BTreeSet<u32> = candidates.iter().map(|(r, _, _)| r.id().get()).collect();
    let mut occupied: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in &refs {
        let ty = r.attr_no_ns("type").unwrap_or("").to_string();
        let href = match r.attr_no_ns("href") {
            Some(h) if dropping.contains(&r.id().get()) => {
                h.split('#').next().unwrap_or(h).to_string()
            }
            Some(h) => h.to_string(),
            None => continue,
        };
        *occupied.entry((ty, href)).or_insert(0) += 1;
    }

    let mut out = Vec::new();
    for (r, before, after) in candidates {
        let ty = r.attr_no_ns("type").unwrap_or("").to_string();
        // >1 holder of the post-edit pair means this rewrite would collide.
        if occupied.get(&(ty, after.clone())).copied().unwrap_or(0) > 1 {
            continue;
        }
        let Some(rewritten) = set_attr_value(&opf[r.range()], "href", &after) else {
            continue;
        };
        out.push((
            before,
            after,
            MetaEdit {
                range: r.range(),
                replacement: rewritten,
            },
        ));
    }
    (!out.is_empty()).then_some(out)
}

/// True if `c` may appear inside an id/fragment, so `#09` is not seen inside
/// `#0099ff` (a CSS colour) and `#ch1` is not seen inside `#ch10`.
fn is_frag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

/// The byte span of the *value* of the single boundary `id="bad"` occurrence.
/// The caller checks uniqueness first; this returns the first boundary match.
fn id_attr_value_span(text: &str, bad: &str) -> Option<Range<usize>> {
    for quote in ['"', '\''] {
        let needle = format!("id={quote}{bad}{quote}");
        let mut from = 0;
        while let Some(rel) = text[from..].find(needle.as_str()) {
            let start = from + rel;
            if is_attr_boundary(text, start) {
                let vstart = start + format!("id={quote}").len();
                return Some(vstart..vstart + bad.len());
            }
            from = start + needle.len();
        }
    }
    None
}

/// Every `#value` occurrence in `text`, as (span of the `#`, span of the value).
/// Bounded by [`is_frag_char`] so a longer id that merely starts with `value`
/// is never matched.
fn fragment_spans(text: &str, value: &str) -> Vec<(usize, Range<usize>)> {
    let needle = format!("#{value}");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find(needle.as_str()) {
        let hash = from + rel;
        let end = hash + needle.len();
        let next_ok = text[end..].chars().next().is_none_or(|c| !is_frag_char(c));
        if next_ok {
            out.push((hash, (hash + 1)..end));
        }
        from = hash + needle.len();
    }
    out
}

/// What the reference at `hash` points at: the **path part** of the attribute
/// value the fragment sits in (`""` for a bare `#frag`, i.e. this same file).
///
/// `None` means *unclassifiable*, and the caller must then decline the whole
/// rename. That is the safety net for everything this fixer cannot reason about:
/// a `#frag` in a CSS selector or stylesheet, in script, or in ordinary prose —
/// none of which is a quoted attribute value, so none of which can be rewritten
/// with confidence. The walk back is bounded by the characters that cannot occur
/// inside an attribute value, so it can never wander into a neighbouring tag.
fn reference_path(text: &str, hash: usize) -> Option<String> {
    let before = &text[..hash];
    let quote = before
        .char_indices()
        .rev()
        .take(512)
        .find(|(_, c)| matches!(c, '"' | '\'' | '<' | '>' | '\n'))
        .and_then(|(i, c)| matches!(c, '"' | '\'').then_some((i, c)))?;
    let (qpos, qchar) = quote;
    // The value must END at the fragment: anything after it (a query string, a
    // second fragment) is a shape this fixer does not claim to understand.
    let after = &text[hash..];
    let close = after.find(qchar)?;
    let tail = &after[..close];
    if tail.contains(['"', '\'', '<', '>']) {
        return None;
    }
    Some(text[qpos + 1..hash].to_string())
}

/// Whether `name` is a kind of file that can hold a fragment reference at all.
///
/// This is a **correctness** filter, not an optimization. `Workspace::get_text`
/// will hand back the bytes of a JPEG as a string, and a cover image reliably
/// contains a byte pair spelling `#1` somewhere; that occurrence is not a
/// reference, cannot be classified, and — since an unclassifiable occurrence
/// makes this fixer decline — one cover image was enough to abandon ten
/// perfectly repairable ids on a real book. Only markup, styles and script can
/// point at a fragment, so only those are scanned.
fn can_reference_a_fragment(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        ".xhtml", ".html", ".htm", ".xml", ".ncx", ".opf", ".svg", ".css", ".js",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

/// `path` resolved against the directory of `from`, as a container-absolute
/// name. `None` if it escapes the container or is percent-encoded (we do not
/// guess at an encoding we would then have to re-emit).
fn resolve_against(from: &str, path: &str) -> Option<String> {
    if path.contains('%') {
        return None;
    }
    let mut parts: Vec<&str> = match from.rfind('/') {
        Some(i) => from[..i].split('/').collect(),
        None => Vec::new(),
    };
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// `RSC-005` / `opf.content_document.schema_violation`, *value of attribute
/// "id" is invalid*: a content document carries an `id` that is not a valid XML
/// NCName. On the 94-book shelf this is **one** defect wearing one shape — all
/// 312 findings are an id that starts with a digit — so the repair is the same
/// sanitize-and-rename the NCX fixer does.
///
/// **What makes this different from `ncx_ncnames`, and why it is the first
/// cross-file fixer.** That fixer could rename freely because NCX ids are not
/// reference targets. These are: 191 of the 312 are pointed at from somewhere —
/// 181 from the NCX, 150 from other content documents, 2 from the OPF. A rename
/// that does not move every reference with it trades this finding for a dangling
/// fragment, which is the self-inflicted-regression class the house rules care
/// most about.
///
/// **The hazard is measured, not hypothetical.** Six bad values on the shelf are
/// carried by 6–12 *different documents of the same book*, so a global search for
/// `#value` would rewrite links that legitimately target another document's
/// identically-named id. Every reference is therefore resolved: the path part of
/// the attribute value it sits in is resolved against the **referring** file's
/// own directory (the mistake `frag_diag` made once and `docs/API.md` records)
/// and rewritten only when it lands on this document.
///
/// **Anything it cannot resolve, it declines** — see [`reference_path`]. A
/// `#value` in a stylesheet, in script or in prose is not a quoted attribute
/// value, and rather than guess, the whole rename for that id is abandoned.
///
/// `ConfirmNeeded`: it renames an anchor and rewrites links across files.
fn content_document_invalid_ids(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut by_doc: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.content_document.schema_violation")
            || !m.text.starts_with("value of attribute")
            || m.params.first().map(String::as_str) != Some("id")
        {
            continue;
        }
        let (Some(doc), Some(value)) = (m.location.as_deref(), m.params.get(1)) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        by_doc
            .entry(doc.to_string())
            .or_default()
            .insert(value.clone());
    }

    let mut fixes = Vec::new();
    for (doc, values) in by_doc {
        let Some(plan) = plan_id_renames(ws, &doc, &values) else {
            continue;
        };
        let renamed = plan.renames.len();
        let listed = plan
            .renames
            .iter()
            .take(3)
            .map(|(old, new)| format!("{old} → {new}"))
            .collect::<Vec<_>>()
            .join(", ");
        let refs: usize = plan
            .edits
            .iter()
            .map(|(f, e)| {
                if *f == doc {
                    e.len() - renamed
                } else {
                    e.len()
                }
            })
            .sum();

        let preview = plan
            .edits
            .keys()
            .map(|f| Change {
                path: f.clone(),
                note: if *f == doc {
                    format!("rename {renamed} invalid id(s): {listed}")
                } else {
                    "rewrite the references that pointed at them".to_string()
                },
            })
            .collect();

        let doc_for_apply = doc.clone();
        let values_for_apply = values.clone();
        fixes.push(ProposedFix {
            fix_id: "fix.content_document_invalid_id",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("opf.content_document.schema_violation"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("opf.content_document.schema_violation"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Rename {renamed} invalid id(s) in {doc}, moving {refs} reference(s) with them"
            ),
            rationale: "An XML id must be an NCName, and these are not — on this shelf they \
                 start with a digit. Each is sanitized to the nearest valid, unique name, and \
                 every reference that pointed at it is rewritten in the same edit: fragments \
                 inside this document, links from other documents, and the NCX. References are \
                 resolved against the referring file's own directory, so a link meaning another \
                 document's identically-named id is left alone. If any occurrence cannot be \
                 resolved — a fragment in a stylesheet, in script or in prose — that id is not \
                 renamed at all."
                .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(plan) = plan_id_renames(ws, &doc_for_apply, &values_for_apply) {
                    for (file, edits) in plan.edits {
                        if let Some(text) = ws.get_text(&file) {
                            ws.set_text(&file, apply_edits(&text, edits));
                        }
                    }
                }
            }),
        });
    }
    fixes
}

/// A rename plan: the (old, new) pairs and every edit they imply, by file.
struct IdRenamePlan {
    renames: Vec<(String, String)>,
    edits: BTreeMap<String, Vec<MetaEdit>>,
}

/// Build the rename plan for `doc`'s invalid ids, or `None` (decline) when
/// nothing can be renamed safely.
///
/// An individual id is skipped — leaving its finding in place — when it cannot
/// be sanitized, when it is not carried by exactly one `id=` attribute (two
/// elements sharing an id is a *different* defect and not this fixer's to guess
/// at), or when any reference to it cannot be resolved.
fn plan_id_renames(ws: &Workspace, doc: &str, values: &BTreeSet<String>) -> Option<IdRenamePlan> {
    let doc_text = ws.get_text(doc)?;
    let files: Vec<(String, String)> = ws
        .names()
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|n| can_reference_a_fragment(n))
        .filter_map(|n| ws.get_text(&n).map(|t| (n, t)))
        .collect();

    let mut used = existing_ids(&doc_text);
    let mut renames = Vec::new();
    let mut edits: BTreeMap<String, Vec<MetaEdit>> = BTreeMap::new();

    for old in values {
        if attr_occurrences(&doc_text, old) != 1 {
            continue;
        }
        let Some(base) = sanitize_ncname(old) else {
            continue;
        };
        let new = make_unique(base, &used);
        let Some(id_span) = id_attr_value_span(&doc_text, old) else {
            continue;
        };

        // Every reference to this id, or a reason to abandon it.
        let mut ref_edits: Vec<(String, MetaEdit)> = Vec::new();
        let mut resolvable = true;
        for (file, text) in &files {
            for (hash, span) in fragment_spans(text, old) {
                let Some(path) = reference_path(text, hash) else {
                    resolvable = false;
                    break;
                };
                let target = if path.is_empty() {
                    file.clone()
                } else {
                    match resolve_against(file, &path) {
                        Some(t) => t,
                        None => {
                            resolvable = false;
                            break;
                        }
                    }
                };
                if target == doc {
                    ref_edits.push((
                        file.clone(),
                        MetaEdit {
                            range: span,
                            replacement: new.clone(),
                        },
                    ));
                }
            }
            if !resolvable {
                break;
            }
        }
        if !resolvable {
            continue;
        }

        used.insert(new.clone());
        renames.push((old.clone(), new.clone()));
        edits.entry(doc.to_string()).or_default().push(MetaEdit {
            range: id_span,
            replacement: new,
        });
        for (file, edit) in ref_edits {
            edits.entry(file).or_default().push(edit);
        }
    }

    (!renames.is_empty()).then_some(IdRenamePlan { renames, edits })
}

/// `RSC-005` / `opf.package.schema_violation`: an attribute EPUB 3 introduced,
/// sitting on a package document that declares `version="2.0"`.
///
/// **Not to be confused with its content-document twin.** `attribute "X" is not
/// allowed here` under `opf.content_document.schema_violation` is the largest and
/// least repairable surface epubveri reports; `docs/COVERAGE.md` says to stay
/// away from it. This is a different rule over the package document's much
/// smaller vocabulary — 4 findings on the shelf, not thousands — so the **rule
/// name**, never the message text, is what selects this fixer.
///
/// **Deleted only after the redundancy is verified in that book**, which is what
/// makes it `AutoSafe`:
///
/// - `properties="cover-image"` on a manifest item, when the package also
///   carries `<meta name="cover" content="…">` naming that same item's `id`. The
///   cover is then already declared the way EPUB 2 declares it; the attribute
///   repeats it.
/// - `page-progression-direction="ltr"`, which is EPUB 3's own default and so
///   asserts nothing anywhere.
///
/// Everything else declines — another `properties` token (EPUB 2 has no
/// equivalent, so dropping it would discard a real claim), a `cover-image` with
/// no matching or a mismatched `<meta name="cover">` (then the attribute is the
/// *only* cover declaration), and `rtl` (authored information EPUB 2 has nowhere
/// to put, which is a reason to leave the book alone rather than erase it).
fn epub3_attrs_in_epub2_package(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut by_file: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.package.schema_violation") || !m.text.starts_with("attribute ") {
            continue;
        }
        let (Some(file), Some(attr)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        // NB the two spellings differ: since epubveri 0.9.19 an OPF-namespaced
        // name arrives here prefixed (`opf:file-as`), while the lookup below is
        // roxmltree's `attr.name()`, which is the local name. Harmless today —
        // the only shapes this fixer accepts are unprefixed — but a prefixed
        // attribute would never match, so resolve the name properly before
        // adding one.
        by_file
            .entry(file.to_string())
            .or_default()
            .insert(attr.clone());
    }

    let mut fixes = Vec::new();
    for (file, attrs) in by_file {
        let Some(text) = ws.get_text(&file) else {
            continue;
        };
        let Some(dropped) = compute_epub3_attr_edits(&text, &attrs) else {
            continue;
        };
        let n = dropped.len();
        let listed = dropped
            .iter()
            .map(|d| d.name.clone())
            .collect::<Vec<_>>()
            .join(", ");

        let file_for_apply = file.clone();
        let attrs_for_apply = attrs.clone();
        fixes.push(ProposedFix {
            fix_id: "fix.epub3_attr_in_epub2_package",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("opf.package.schema_violation"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("opf.package.schema_violation"),
            ),
            tier: Tier::AutoSafe,
            title: format!("Delete {n} EPUB 3 attribute(s) in {file} ({listed})"),
            rationale: "The package document declares EPUB 2, and these attributes were \
                 introduced by EPUB 3. Each is deleted only after checking that it says nothing \
                 this book does not already say: a properties=\"cover-image\" whose cover is \
                 also declared by <meta name=\"cover\"> on the same item, or a \
                 page-progression-direction of \"ltr\", which is the default everywhere. Any \
                 other value carries information EPUB 2 cannot express, and is left alone."
                .to_string(),
            preview: dropped
                .iter()
                .map(|d| Change {
                    path: file.clone(),
                    note: format!("delete {} ({})", text[d.edit.range.clone()].trim(), d.why),
                })
                .collect(),
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(dropped) = compute_epub3_attr_edits(&text, &attrs_for_apply)
                {
                    let edits = dropped.into_iter().map(|d| d.edit).collect();
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// One attribute this fixer is willing to delete, and the reason it is safe.
struct DroppableAttr {
    name: String,
    why: &'static str,
    edit: MetaEdit,
}

/// Every reported attribute that is verifiably redundant *in this package*.
/// `None` (decline) if it won't parse or nothing qualifies.
fn compute_epub3_attr_edits(opf: &str, attrs: &BTreeSet<String>) -> Option<Vec<DroppableAttr>> {
    let doc = parse_xml(opf)?;
    // The id the legacy EPUB 2 cover declaration points at, if there is one.
    let legacy_cover: Option<&str> = doc
        .descendants()
        .find(|n| {
            n.is_element() && n.tag_name().name() == "meta" && n.attr_no_ns("name") == Some("cover")
        })
        .and_then(|n| n.attr_no_ns("content"));

    let mut out = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        for attr in node.attributes() {
            let name = attr.name();
            if !attrs.contains(name) {
                continue;
            }
            let why = match (node.tag_name().name(), name, attr.value().trim()) {
                // The cover is already declared the way EPUB 2 declares it, on
                // this very item — so the EPUB 3 attribute only repeats it.
                ("item", "properties", "cover-image")
                    if legacy_cover.is_some() && legacy_cover == node.attr_no_ns("id") =>
                {
                    "the cover is already declared by <meta name=\"cover\"> on this item"
                }
                // EPUB 3's own default: the attribute asserts nothing anywhere.
                ("spine", "page-progression-direction", "ltr") => {
                    "\"ltr\" is the default reading direction"
                }
                _ => continue,
            };
            out.push(DroppableAttr {
                name: name.to_string(),
                why,
                edit: MetaEdit {
                    range: attr_span_with_leading_space(opf, attr.range()),
                    replacement: String::new(),
                },
            });
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `RSC-005` / `opf.content_document.duplicate_id`: two or more elements in one
/// content document carry the same `id`, which XML forbids. The **first**
/// occurrence keeps it; every later one is renamed to a unique value.
///
/// **Why no reference moves, which is the whole argument here.** Content-document
/// ids *are* reference targets, unlike the NCX ids of [`ncx_duplicate_ids`], so
/// "rename and touch nothing else" has to be justified rather than assumed. It
/// holds because a fragment into a document with a duplicated id already resolves
/// to the **first** element in tree order carrying it — that is what every
/// conforming processor does, and what a reader has been seeing. Keeping the
/// first therefore leaves every `#fragment` pointing at the element it already
/// pointed at. Renaming the first and moving references would be the riskier
/// repair for no gain.
///
/// On the shelf the point is moot twice over: none of the 21 duplicated ids is
/// referenced from anywhere in its book. The reasoning carries this fixer, not
/// the corpus.
///
/// **Disjoint from [`content_document_invalid_ids`] by construction.** That fixer
/// renames an id it can prove occurs exactly once and declines a duplicated one,
/// because which element a reference meant would then be a guess; this one takes
/// the other case. New names are built from the *sanitized* stem, so a value that
/// is both duplicated and not a valid NCName cannot yield more invalid names than
/// it found.
fn content_document_duplicate_ids(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // file -> ordered, de-duplicated reported id values.
    let mut by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.content_document.duplicate_id") {
            continue;
        }
        let (Some(file), Some(dup)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        let list = by_file.entry(file.to_string()).or_default();
        if !list.contains(dup) {
            list.push(dup.clone());
        }
    }

    let mut fixes = Vec::new();
    for (file, dups) in by_file {
        let Some(text) = ws.get_text(&file) else {
            continue;
        };
        let mut used = existing_ids(&text);
        // Plan the fixed new-id per later occurrence, so apply is deterministic
        // and robust to any earlier edit of this document.
        let mut plan: Vec<(String, Vec<String>)> = Vec::new();
        for dup in &dups {
            let occ = attr_occurrences(&text, dup);
            if occ < 2 {
                continue; // stale finding — nothing duplicated
            }
            let stem = sanitize_ncname(dup).unwrap_or_else(|| dup.clone());
            let news: Vec<String> = (1..occ)
                .map(|_| {
                    let new = make_unique(stem.clone(), &used);
                    used.insert(new.clone());
                    new
                })
                .collect();
            plan.push((dup.clone(), news));
        }
        if plan.is_empty() {
            continue;
        }

        let preview: Vec<Change> = plan
            .iter()
            .map(|(dup, news)| Change {
                path: file.clone(),
                note: format!("rename {} duplicate id(s) \"{dup}\" → {news:?}", news.len()),
            })
            .collect();
        let n: usize = plan.iter().map(|(_, news)| news.len()).sum();
        let plan_for_apply = plan.clone();
        let file_for_apply = file.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.content_document_duplicate_id",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("opf.content_document.duplicate_id"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("opf.content_document.duplicate_id"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Make {n} duplicate id{} unique in {file}",
                if n == 1 { "" } else { "s" },
            ),
            rationale:
                "Two or more elements in this document share an id, which XML forbids. The first \
                 occurrence keeps it and each later one is renamed to a unique value. No reference \
                 is rewritten, and none needs to be: a fragment into a document with a duplicated \
                 id already resolves to the first element carrying it, so keeping the first leaves \
                 every link pointing exactly where it pointed before."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(mut text) = ws.get_text(&file_for_apply) {
                    for (dup, news) in &plan_for_apply {
                        text = rename_later_id_occurrences(&text, dup, news);
                    }
                    ws.set_text(&file_for_apply, text);
                }
            }),
        });
    }
    fixes
}

/// `RSC-007` / `opf.content_document.reference_missing_resource`: a link whose
/// path does not resolve, but whose target is still in the container under the
/// same name — a book restructured after it was authored, with the old prefix
/// left behind (`../Text/DiPNOTLAR.xhtml#a8` where the file now sits beside the
/// referring document).
///
/// **This rule was closed as human-only on 2026-08-06 and reopened by the
/// corpus.** On the 94-book shelf every case was a scheme-less bare hostname or
/// placeholder junk, neither repairable. The 115-book shelf brought this shape,
/// which is determinate: the reference names a file by basename, exactly one
/// entry in the container has that basename, so that entry is the file it meant.
///
/// **The fragment is a guard and a corroboration at once.** Rewriting a path
/// could trade this finding for a dangling `RSC-012`, so the fragment must
/// already exist in the chosen target. That check also does evidential work — a
/// same-named file that merely happened to be elsewhere would not carry `#a8`.
///
/// Everything else declines: a basename matching nothing (the file is genuinely
/// absent) or several entries (which one it meant is a guess), an external URL, a
/// bare hostname, placeholder junk, a percent-encoded path, and a reference that
/// cannot be found as a quoted attribute value in the document.
fn reference_wrong_path(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut by_doc: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.content_document.reference_missing_resource") {
            continue;
        }
        let (Some(doc), Some(raw)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        by_doc
            .entry(doc.to_string())
            .or_default()
            .insert(raw.clone());
    }

    let mut fixes = Vec::new();
    for (doc, raws) in by_doc {
        let Some(text) = ws.get_text(&doc) else {
            continue;
        };
        let names: Vec<String> = ws.names().cloned().collect();
        let mut repoints: Vec<(String, String)> = Vec::new();
        for raw in &raws {
            let Some(fixed) = repointed_reference(ws, &names, &doc, raw) else {
                continue;
            };
            if quoted_attr_span(&text, raw).is_none() {
                continue; // not visible as an attribute value — go quiet
            }
            repoints.push((raw.clone(), fixed));
        }
        if repoints.is_empty() {
            continue;
        }

        let n = repoints.len();
        let preview: Vec<Change> = repoints
            .iter()
            .take(6)
            .map(|(from, to)| Change {
                path: doc.clone(),
                note: format!("repoint {from} → {to}"),
            })
            .collect();
        let doc_for_apply = doc.clone();
        let repoints_for_apply = repoints.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.reference_wrong_path",
            addresses_id: "RSC-007".to_string(),
            addresses_rule: Some("opf.content_document.reference_missing_resource"),
            addresses_severity: addressed_severity(
                report,
                "RSC-007",
                Some("opf.content_document.reference_missing_resource"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Repoint {n} stale reference{} in {doc}",
                if n == 1 { "" } else { "s" }
            ),
            rationale:
                "These links name a file the container still holds, by a path that no longer \
                 resolves — the book was restructured after it was written. Exactly one entry \
                 carries each name, so the target is not a guess, and the fragment already exists \
                 in that entry, so the link resolves after the repair rather than dangling. Only \
                 the path is rewritten; the fragment and everything else are untouched. A name \
                 matching nothing, or more than one entry, is left alone."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&doc_for_apply) {
                    let mut edits = Vec::new();
                    for (from, to) in &repoints_for_apply {
                        if let Some(span) = quoted_attr_span(&text, from) {
                            edits.push(MetaEdit {
                                range: span,
                                replacement: to.clone(),
                            });
                        }
                    }
                    ws.set_text(&doc_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// The corrected reference for `raw` as written in `doc`, or `None` to decline.
/// `RSC-007` / `opf.ncx.content_src_missing_resource`: a `<navPoint>`'s
/// `<content src>` names a file that is not at that path, but the container
/// still holds it under the same name.
///
/// **The NCX member of a family already closed at four other sites**, and it
/// shares [`repointed_reference`] with [`reference_wrong_path`] rather than
/// re-deriving the rule — one basename match, the fragment must already exist in
/// the chosen target, everything else declines. Sharing the decision is the
/// point: two copies of "which entry did it mean" would drift, and the guards are
/// the whole safety argument.
///
/// **Closed on 2026-08-12 and re-opened by the corpus on 2026-08-21, which is the
/// second time this has happened in this file.** `ncx_src_probe` measured all
/// seven findings on the 157-book shelf and every one pointed at a file simply
/// absent from the book — nothing to repair toward, so the rule was left alone.
/// At 385 books there are 46 findings across 5 books and **exactly one** has the
/// determinate shape: a Calibre book whose NCX says `OEBPS/Text/titlepage.xhtml`
/// while the file sits at the container root. The same reopening happened to
/// `opf.content_document.reference_missing_resource` on 2026-08-07.
///
/// `ConfirmNeeded`, matching its sibling: rewriting a navigation path is a
/// visible change to the book's table of contents.
fn ncx_src_wrong_path(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut by_ncx: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("opf.ncx.content_src_missing_resource") {
            continue;
        }
        let (Some(ncx), Some(raw)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        by_ncx
            .entry(ncx.to_string())
            .or_default()
            .insert(raw.clone());
    }

    let mut fixes = Vec::new();
    for (ncx, raws) in by_ncx {
        let Some(text) = ws.get_text(&ncx) else {
            continue;
        };
        let names: Vec<String> = ws.names().cloned().collect();
        let mut repoints: Vec<(String, String)> = Vec::new();
        for raw in &raws {
            let Some(fixed) = repointed_reference(ws, &names, &ncx, raw) else {
                continue;
            };
            if quoted_attr_span(&text, raw).is_none() {
                continue; // not visible as an attribute value — go quiet
            }
            repoints.push((raw.clone(), fixed));
        }
        if repoints.is_empty() {
            continue;
        }

        let n = repoints.len();
        let preview: Vec<Change> = repoints
            .iter()
            .take(6)
            .map(|(from, to)| Change {
                path: ncx.clone(),
                note: format!("repoint {from} → {to}"),
            })
            .collect();
        let ncx_for_apply = ncx.clone();
        let repoints_for_apply = repoints.clone();

        fixes.push(ProposedFix {
            fix_id: "fix.ncx_src_wrong_path",
            addresses_id: "RSC-007".to_string(),
            addresses_rule: Some("opf.ncx.content_src_missing_resource"),
            addresses_severity: addressed_severity(
                report,
                "RSC-007",
                Some("opf.ncx.content_src_missing_resource"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Repoint {n} navigation target{} in {ncx}",
                if n == 1 { "" } else { "s" }
            ),
            rationale:
                "These navigation entries name a file the container still holds, by a path that \
                 no longer resolves — the book was restructured after its table of contents was \
                 written. Exactly one entry carries each name, so the target is not a guess, and \
                 where a fragment is present it already exists in that entry, so the entry \
                 resolves after the repair rather than dangling. Only the path is rewritten. A \
                 name matching nothing, or more than one entry, is left alone."
                    .to_string(),
            preview,
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&ncx_for_apply) {
                    let mut edits = Vec::new();
                    for (from, to) in &repoints_for_apply {
                        if let Some(span) = quoted_attr_span(&text, from) {
                            edits.push(MetaEdit {
                                range: span,
                                replacement: to.clone(),
                            });
                        }
                    }
                    ws.set_text(&ncx_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

fn repointed_reference(ws: &Workspace, names: &[String], doc: &str, raw: &str) -> Option<String> {
    let (path, fragment) = match raw.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (raw, None),
    };
    if path.is_empty()
        || path.contains('%')
        || path.contains(':')
        || path.starts_with("www.")
        || path.chars().all(|c| c == 'X')
    {
        return None;
    }
    let base = path.rsplit('/').next()?;
    let mut matches = names.iter().filter(|n| n.rsplit('/').next() == Some(base));
    let target = matches.next()?;
    if matches.next().is_some() {
        return None; // several entries share the name — which one is a guess
    }
    // Clearing this finding by creating a dangling fragment is not a repair.
    if let Some(frag) = fragment.filter(|f| !f.is_empty()) {
        let text = ws.get_text(target)?;
        let anchored = [
            format!("id=\"{frag}\""),
            format!("id='{frag}'"),
            format!("name=\"{frag}\""),
            format!("name='{frag}'"),
        ];
        if !anchored.iter().any(|a| text.contains(a.as_str())) {
            return None;
        }
    }
    let rel = relative_path(doc, target)?;
    Some(match fragment {
        Some(f) => format!("{rel}#{f}"),
        None => rel,
    })
}

/// `target` expressed relative to the directory holding `from`, both being
/// container-absolute entry names.
fn relative_path(from: &str, target: &str) -> Option<String> {
    let from_dir: Vec<&str> = from.split('/').collect();
    let from_dir = &from_dir[..from_dir.len().checked_sub(1)?];
    let target_parts: Vec<&str> = target.split('/').collect();
    let shared = from_dir
        .iter()
        .zip(target_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out: Vec<&str> = vec![".."; from_dir.len() - shared];
    out.extend_from_slice(&target_parts[shared..]);
    (!out.is_empty()).then(|| out.join("/"))
}

/// The span of `value` where it is a whole quoted attribute value (`="value"`),
/// so a reference is never rewritten from inside a longer string. `None` if it
/// does not appear that way.
fn quoted_attr_span(text: &str, value: &str) -> Option<Range<usize>> {
    for quote in ['"', '\''] {
        let needle = format!("={quote}{value}{quote}");
        if let Some(at) = text.find(needle.as_str()) {
            let start = at + 2; // past `=` and the opening quote
            return Some(start..start + value.len());
        }
    }
    None
}

/// `OPF-030` / `RSC-005`: the package declares which identifier is canonical and
/// that declaration lands on nothing usable — either no `<dc:identifier>` carries
/// the named id (`opf.package.unique_identifier_unresolved`), or the one that
/// does is empty (`opf.package.opf_identifier_not_empty`). Two rules, one defect
/// at two stages; on the shelf they hit disjoint sets of five books each.
///
/// **Nothing is invented.** The value is already in the book, written by its
/// producer; the id is already in the book, written in `unique-identifier`. The
/// repair only attaches the one to the other — the same principle as
/// [`empty_titles`], which moves a TOC label the author already wrote.
///
/// **Why not the alternatives.** Copying a sibling's value *into* the empty
/// element would leave two identifiers asserting the same string; repointing
/// `unique-identifier` elsewhere would be choosing which of the book's identities
/// is canonical. Moving the declared id onto the sole real identifier does
/// neither.
///
/// **It carries the NCX with it, in the same proposal.** Making the package
/// identifier resolvable is what first lets epubveri compare the NCX's `dtb:uid`
/// against it — so on three shelf books the repair *unmasked* a pre-existing
/// mismatch and produced `NCX-001` where there had been none. A whole-shelf audit
/// caught that; no unit test could have, because the edit was correct and the
/// *book* ended up worse. So the `dtb:uid` is synced to the same value in the
/// same edit, on the pattern [`manifest_dangling_items`] already sets: approving
/// half of this would leave a finding epubsana created itself.
///
/// **It declines on 7 of the 10 shelf books**, and that is the point of it: four
/// carry both a UUID and an ISBN with no `id` on either, where which is canonical
/// is an editorial decision (the attribute's *name* hints, and hints are not
/// evidence); two carry no `<dc:identifier>` at all, where the repair would have
/// to generate one.
fn package_identifier(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let mut files: BTreeMap<String, &'static str> = BTreeMap::new();
    for m in &report.messages {
        let stage = match m.rule {
            Some("opf.package.unique_identifier_unresolved") => "OPF-030",
            Some("opf.package.opf_identifier_not_empty") => "RSC-005",
            _ => continue,
        };
        if let Some(f) = m.location.as_deref() {
            files.insert(f.to_string(), stage);
        }
    }

    let mut fixes = Vec::new();
    for (file, id_code) in files {
        let Some(text) = ws.get_text(&file) else {
            continue;
        };
        let Some(plan) = plan_package_identifier(&text, ws) else {
            continue;
        };
        let note = match plan.drop_empty {
            Some(_) => format!(
                "give id=\"{}\" to the identifier holding {:?}, and drop the empty one",
                plan.uid, plan.value
            ),
            None => format!(
                "give id=\"{}\" to the identifier holding {:?}",
                plan.uid, plan.value
            ),
        };
        let rule = if id_code == "OPF-030" {
            "opf.package.unique_identifier_unresolved"
        } else {
            "opf.package.opf_identifier_not_empty"
        };

        let file_for_apply = file.clone();
        fixes.push(ProposedFix {
            fix_id: "fix.package_identifier",
            addresses_id: id_code.to_string(),
            addresses_rule: Some(rule),
            addresses_severity: addressed_severity(report, id_code, Some(rule)),
            tier: Tier::ConfirmNeeded,
            title: format!("Point the package's unique-identifier at a real identifier in {file}"),
            rationale:
                "The package says which identifier is canonical, and that declaration lands on \
                 nothing a reading system can use — either no dc:identifier carries the named id, \
                 or the one that does is empty. The book holds exactly one identifier that could \
                 be meant, so the declared id is attached to it and, where an empty element is \
                 left over, that element is dropped. Nothing is invented: the value was already \
                 in the book and the id was already in the package. Where a book carries two \
                 candidates — a UUID and an ISBN, say — which is canonical is an editorial \
                 decision and this declines."
                    .to_string(),
            preview: {
                let mut p = vec![Change {
                    path: file.clone(),
                    note,
                }];
                p.extend(plan.ncx.iter().map(|(ncx, _)| Change {
                    path: ncx.clone(),
                    note: format!("sync dtb:uid to {:?} so the NCX still agrees", plan.value),
                }));
                p
            },
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(plan) = plan_package_identifier(&text, ws)
                {
                    let mut edits = vec![MetaEdit {
                        range: plan.insert_at..plan.insert_at,
                        replacement: format!(" id=\"{}\"", plan.uid),
                    }];
                    if let Some(drop) = plan.drop_empty {
                        edits.push(MetaEdit {
                            range: drop,
                            replacement: String::new(),
                        });
                    }
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                    for (ncx, edit) in plan.ncx {
                        if let Some(ncx_text) = ws.get_text(&ncx) {
                            ws.set_text(&ncx, apply_edits(&ncx_text, vec![edit]));
                        }
                    }
                }
            }),
        });
    }
    fixes
}

/// Where to attach the declared id, which empty element (if any) to drop, and
/// the NCX edits that must travel with it.
struct IdentifierPlan {
    uid: String,
    value: String,
    /// Byte offset just past the target element's name, where ` id="…"` goes.
    insert_at: usize,
    drop_empty: Option<Range<usize>>,
    /// `dtb:uid` syncs, by NCX path. Empty when the book has no NCX or it
    /// already agrees.
    ncx: Vec<(String, MetaEdit)>,
}

/// The `dtb:uid` edits needed so every NCX in the container agrees with `value`.
///
/// Read [`package_identifier`] for why this is not a separate fix: the package
/// repair is what makes the comparison possible at all, so leaving the NCX behind
/// trades one finding for another.
fn ncx_uid_syncs(ws: &Workspace, value: &str) -> Vec<(String, MetaEdit)> {
    let mut out = Vec::new();
    for name in ws.names().cloned().collect::<Vec<_>>() {
        if !name.to_ascii_lowercase().ends_with(".ncx") {
            continue;
        }
        let Some(text) = ws.get_text(&name) else {
            continue;
        };
        let Some((range, old)) = find_dtb_uid_meta(&text) else {
            continue;
        };
        if old.trim() == value {
            continue;
        }
        let Some(new_element) = set_content_attr(&text[range.clone()], value) else {
            continue;
        };
        out.push((
            name,
            MetaEdit {
                range,
                replacement: new_element,
            },
        ));
    }
    out
}

/// `None` (decline) unless the package names a `unique-identifier` and the book
/// holds **exactly one** `<dc:identifier>` that is a candidate for it: no `id` of
/// its own, and a non-empty value.
fn plan_package_identifier(opf: &str, ws: &Workspace) -> Option<IdentifierPlan> {
    let doc = parse_xml(opf)?;
    let package = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "package")?;
    let uid = package.attr_no_ns("unique-identifier")?.trim().to_string();
    if uid.is_empty() {
        return None;
    }
    let metadata = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "metadata")?;
    let identifiers: Vec<_> = metadata
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "identifier")
        .collect();

    let text_of = |n: &roxmltree::Node| -> String {
        n.descendants()
            .filter(|t| t.is_text())
            .filter_map(|t| t.text())
            .collect::<String>()
            .trim()
            .to_string()
    };

    // The element the declaration currently lands on, if any.
    let declared = identifiers
        .iter()
        .find(|n| n.attr_no_ns("id").map(str::trim) == Some(uid.as_str()));
    let drop_empty = match declared {
        // Already resolved and non-empty: nothing to do (or a stale finding).
        Some(n) if !text_of(n).is_empty() => return None,
        Some(n) => Some(with_leading_whitespace(opf, n.range())),
        None => None,
    };

    // Candidates: no id of their own, a real value, and not the empty element.
    let mut candidates = identifiers
        .iter()
        .filter(|n| n.attr_no_ns("id").is_none() && !text_of(n).is_empty());
    let target = candidates.next()?;
    if candidates.next().is_some() {
        return None; // which identity is canonical is not ours to choose
    }

    let value = text_of(target);
    let ncx = ncx_uid_syncs(ws, &value);
    Some(IdentifierPlan {
        uid,
        value,
        insert_at: element_name_end(opf, target.range().start)?,
        drop_empty,
        ncx,
    })
}

/// The byte offset just past an element's name in its start tag, where a new
/// attribute can be inserted. `<dc:identifier …` → the offset after `identifier`.
fn element_name_end(text: &str, tag_start: usize) -> Option<usize> {
    let rest = text.get(tag_start..)?;
    let mut it = rest.char_indices();
    if it.next()?.1 != '<' {
        return None;
    }
    for (i, c) in it {
        if !(c.is_alphanumeric() || matches!(c, ':' | '-' | '_' | '.')) {
            return Some(tag_start + i);
        }
    }
    None
}

/// `RSC-005` / `htm.epub2_dom.nested_anchor`: an `<a>` containing another `<a>`,
/// which XHTML forbids. On the shelf every case is one shape — a footnote
/// reference whose **outer** anchor carries no `href`:
///
/// ```text
/// <a id="bookmark1"><sup><a href="#footnote1">1</a></sup></a>
/// ```
///
/// The outer element is not a link; it is an **anchor target**, the legacy way of
/// naming a position from before every element could carry an `id`. So the repair
/// is to unwrap it and move the `id` to its single element child — `#bookmark1`
/// still resolves, to an element at the same place in the same rendered line, and
/// nothing is deleted but a wrapper that held no information of its own.
///
/// Declines when the outer anchor has an `href` (it is then a real link, and
/// which of two nested links to keep is not ours to decide), when it carries any
/// attribute besides `id` (they would be lost, and re-attaching them to a
/// different element asserts they apply to it), when the child already has an
/// `id`, or when the anchor wraps more than that one child.
fn nested_anchors(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let docs: BTreeSet<&str> = report
        .messages
        .iter()
        .filter(|m| m.rule == Some("htm.epub2_dom.nested_anchor"))
        .filter_map(|m| m.location.as_deref())
        .collect();

    let mut fixes = Vec::new();
    for doc in docs {
        let Some(text) = ws.get_text(doc) else {
            continue;
        };
        let Some(edits) = plan_nested_anchor_unwraps(&text) else {
            continue;
        };
        let n = edits.len();

        let doc_for_apply = doc.to_string();
        fixes.push(ProposedFix {
            fix_id: "fix.nested_anchor",
            addresses_id: "RSC-005".to_string(),
            addresses_rule: Some("htm.epub2_dom.nested_anchor"),
            addresses_severity: addressed_severity(
                report,
                "RSC-005",
                Some("htm.epub2_dom.nested_anchor"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Unwrap {n} anchor target{} wrapped around a link in {doc}",
                if n == 1 { "" } else { "s" }
            ),
            rationale:
                "An <a> cannot contain another <a>. Here the outer one carries no href — it is \
                 not a link but an anchor target, the legacy way of naming a position before \
                 every element could hold an id. It is unwrapped and its id moves to its single \
                 child, so the fragment still resolves, to an element in the same place on the \
                 page. An outer anchor that is a real link, or that carries anything besides an \
                 id, is left alone."
                    .to_string(),
            preview: vec![Change {
                path: doc.to_string(),
                note: format!("unwrap {n} <a id=\"…\"> wrapper(s), moving the id to the child"),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&doc_for_apply)
                    && let Some(edits) = plan_nested_anchor_unwraps(&text)
                {
                    ws.set_text(&doc_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// Replace each qualifying outer `<a>` with its child, `id` moved across.
/// `None` (decline) if the document won't parse or nothing qualifies.
fn plan_nested_anchor_unwraps(text: &str) -> Option<Vec<MetaEdit>> {
    let prepared = prepare_content_doc(text);
    let doc = prepared.parse()?;
    let mut edits = Vec::new();

    for outer in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "a")
    {
        // Only anchors that actually contain another anchor are in scope.
        if !outer
            .descendants()
            .skip(1) // `descendants()` yields the node itself first
            .any(|d| d.is_element() && d.tag_name().name() == "a")
        {
            continue;
        }
        // A link, not a target: not ours to unwrap. And `id` must be the only
        // attribute, or unwrapping loses something.
        let mut attrs = outer.attributes();
        let Some(id_attr) = attrs.next().filter(|a| a.name() == "id") else {
            continue;
        };
        if attrs.next().is_some() {
            continue;
        }
        // Exactly one element child, and nothing else but whitespace.
        let mut child = None;
        let mut extra = false;
        for c in outer.children() {
            if c.is_element() {
                if child.is_some() {
                    extra = true;
                }
                child = Some(c);
            } else if c.is_text() && !c.text().unwrap_or("").trim().is_empty() {
                extra = true;
            }
        }
        let (Some(child), false) = (child, extra) else {
            continue;
        };
        if child.attribute("id").is_some() {
            continue;
        }

        let outer_span = prepared.unshift(outer.range());
        let child_span = prepared.unshift(child.range());
        let child_src = text.get(child_span.clone())?;
        let insert_at = element_name_end(child_src, 0)?;
        let mut replacement = String::with_capacity(child_src.len() + 16);
        replacement.push_str(&child_src[..insert_at]);
        replacement.push_str(&format!(" id=\"{}\"", id_attr.value()));
        replacement.push_str(&child_src[insert_at..]);
        edits.push(MetaEdit {
            range: outer_span,
            replacement,
        });
    }
    (!edits.is_empty()).then_some(edits)
}

/// `OPF-054`: a `<dc:date>` that holds no date at all. Drop the element.
///
/// **Dispatches on the bare `id`** — this site has no `rule` and needs none: it
/// says one thing, and the finding is only a trigger. We re-find the element in
/// the package document ourselves, the same re-locate-by-predicate strategy the
/// other structural fixers use, so no `params` are required either.
///
/// **The id is narrower than it looks, and the trigger is wider.** epubveri runs
/// one check (`is_valid_dc_date`) and splits by version: `OPF-054`/Error on EPUB
/// 2, `OPF-053`/**Warning** on EPUB 3 — so this only ever moves the validity
/// line on EPUB 2, and the scope comes from the detector rather than from a
/// version test here. The check is *not* "is it empty" but "is it a valid W3C-DTF
/// date", which reports an empty value and a malformed one under the same id and
/// the same message text.
///
/// That distinction is this fixer's whole content, because the two need opposite
/// treatment. An empty element states nothing and `dc:date` is optional in EPUB 2
/// (only `title`, `identifier` and `language` are required), so dropping it loses
/// nothing. A malformed but non-empty value — `2022-09-08)`, `March 2019` — is a
/// real authored date: dropping it destroys information the book has, and
/// repairing it means deciding which characters are stray, or parsing natural
/// language. **Every non-empty value is declined**, and the finding survives the
/// repair. Filling an empty one from a catalogue is not on the table either: the
/// date is not in the container, so writing one would be asserting a fact about
/// the world (see `docs/FIXERS.md`).
///
/// `ConfirmNeeded`: it deletes authored markup, and an empty `<dc:date>` is a
/// statement that a date was *meant* to be here.
fn empty_dc_date(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let files: BTreeSet<&str> = report
        .messages
        .iter()
        .filter(|m| m.id == "OPF-054")
        .filter_map(|m| m.location.as_deref())
        .collect();

    let mut fixes = Vec::new();
    for file in files {
        let Some(text) = ws.get_text(file) else {
            continue;
        };
        let Some(edits) = compute_empty_dc_date_edits(&text) else {
            continue;
        };
        let n = edits.len();

        let file_for_apply = file.to_string();
        fixes.push(ProposedFix {
            fix_id: "fix.empty_dc_date",
            addresses_id: "OPF-054".to_string(),
            addresses_rule: None,
            addresses_severity: addressed_severity(report, "OPF-054", None),
            tier: Tier::ConfirmNeeded,
            title: format!("Drop {n} empty <dc:date> element(s) in {file}"),
            rationale: "A <dc:date> with no content states no date: there is nothing in it to \
                 lose, and dc:date is optional, so an absent one is valid. Only elements that are \
                 empty or whitespace-only are dropped — a malformed but non-empty date carries a \
                 real authored value and is left exactly as it is, finding and all."
                .to_string(),
            preview: vec![Change {
                path: file.to_string(),
                note: format!("drop {n} empty <dc:date> element(s)"),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(edits) = compute_empty_dc_date_edits(&text)
                {
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// Edits dropping every empty `<dc:date>`, with the whitespace that preceded it.
/// `None` (decline) if the package document won't parse or no date is empty.
///
/// `<metadata>`'s children named `date` are the candidates, matched exactly as
/// epubveri matches them, so the two never disagree about which element a finding
/// is about.
fn compute_empty_dc_date_edits(opf: &str) -> Option<Vec<MetaEdit>> {
    let doc = parse_xml(opf)?;
    let metadata = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "metadata")?;
    // An `id` a `<meta refines="#…">` targets: dropping its element would orphan
    // the refinement, trading this finding for another.
    let refined: BTreeSet<&str> = doc
        .descendants()
        .filter(|n| n.is_element())
        .filter_map(|n| n.attribute("refines"))
        .filter_map(|r| r.strip_prefix('#'))
        .collect();

    let mut edits = Vec::new();
    for n in metadata
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "date")
    {
        let text: String = n
            .descendants()
            .filter(|t| t.is_text())
            .filter_map(|t| t.text())
            .collect();
        if !text.trim().is_empty() {
            continue; // a real date, however malformed — never ours to delete
        }
        if n.attribute("id").is_some_and(|id| refined.contains(id)) {
            continue;
        }
        edits.push(MetaEdit {
            range: with_leading_whitespace(opf, n.range()),
            replacement: String::new(),
        });
    }
    (!edits.is_empty()).then_some(edits)
}

/// `OPF-072` / `opf.metadata.empty_element`: an optional Dublin Core element in
/// `<metadata>` with no content. The sibling of [`empty_dc_date`], generalised to
/// the rest of the optional DC set, and safe for the identical reason: an empty
/// element states nothing, so there is no value in it to lose.
///
/// **Most of the safety lives upstream, deliberately.** epubveri emits this rule
/// on EPUB 2 only and excludes `identifier`, `date`, `title` and `language` by
/// name, so the three required elements can never reach here and `dc:date` —
/// which has its own rule and its own fixer — cannot draw two proposals for one
/// element. The exclusion is re-stated below anyway: a fixer that deletes things
/// and depends silently on an upstream list breaks quietly the day that list
/// moves.
///
/// `ConfirmNeeded`, matching its sibling: the edit is a deletion, however empty
/// the thing deleted.
fn empty_metadata_element(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let files: BTreeSet<&str> = report
        .messages
        .iter()
        .filter(|m| m.rule == Some("opf.metadata.empty_element"))
        .filter_map(|m| m.location.as_deref())
        .collect();

    let mut fixes = Vec::new();
    for file in files {
        let Some(text) = ws.get_text(file) else {
            continue;
        };
        let Some(edits) = compute_empty_metadata_edits(&text) else {
            continue;
        };
        let n = edits.len();

        let file_for_apply = file.to_string();
        fixes.push(ProposedFix {
            fix_id: "fix.empty_metadata_element",
            addresses_id: "OPF-072".to_string(),
            addresses_rule: Some("opf.metadata.empty_element"),
            addresses_severity: addressed_severity(
                report,
                "OPF-072",
                Some("opf.metadata.empty_element"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!("Drop {n} empty optional metadata element(s) in {file}"),
            rationale:
                "An empty Dublin Core element states nothing — there is no value in it to lose \
                 — and every element this can reach is optional, so its absence is valid. The \
                 required three (dc:title, dc:identifier, dc:language) are never touched, and an \
                 element a <meta refines=\"#id\"> points at is left alone so the refinement is \
                 not orphaned."
                    .to_string(),
            preview: vec![Change {
                path: file.to_string(),
                note: format!("drop {n} empty optional <dc:*> element(s)"),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(edits) = compute_empty_metadata_edits(&text)
                {
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// `RSC-007` / `css.font_face.missing_target`: a `@font-face` rule sources a font
/// file the publication does not contain.
///
/// Deletes the whole rule. The font does not load today and cannot — the file is
/// absent — so anything using that family already falls back to a substitute,
/// and removing a rule that never applied changes nothing a reader sees.
///
/// **Declines a rule holding more than one `url(`**: a second source may be
/// present and working, and choosing which line to cut would be editing CSS
/// rather than deleting a dead rule. Every affected rule on the shelf holds
/// exactly one, which is what makes the whole-rule deletion determinate.
///
/// No CSS parser is involved and none is wanted: this finds an at-rule's braces
/// and nothing more. It does not open the rest of the `css.*` family, which is
/// open-ended in a way this member is not.
fn font_face_missing_target(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    // file -> the urls reported missing in it
    let mut by_file: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in &report.messages {
        if m.rule != Some("css.font_face.missing_target") {
            continue;
        }
        let (Some(file), Some(url)) = (m.location.as_deref(), m.params.first()) else {
            continue;
        };
        by_file
            .entry(file.to_string())
            .or_default()
            .insert(url.clone());
    }

    let mut fixes = Vec::new();
    for (file, urls) in by_file {
        let Some(text) = ws.get_text(&file) else {
            continue;
        };
        let Some(edits) = plan_font_face_drops(&text, &urls) else {
            continue;
        };
        let n = edits.len();

        let file_for_apply = file.clone();
        let urls_for_apply = urls.clone();
        fixes.push(ProposedFix {
            fix_id: "fix.font_face_missing_target",
            addresses_id: "RSC-007".to_string(),
            addresses_rule: Some("css.font_face.missing_target"),
            addresses_severity: addressed_severity(
                report,
                "RSC-007",
                Some("css.font_face.missing_target"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!(
                "Drop {n} @font-face rule{} sourcing a missing font in {file}",
                if n == 1 { "" } else { "s" }
            ),
            rationale:
                "The font file is not in the book, so the rule cannot load anything and never \
                 has: text using that family already falls back to whatever the reading system \
                 substitutes, and deleting a rule that never applied changes nothing a reader \
                 sees. A rule holding more than one url() is left alone — a second source may be \
                 present and working, and choosing between them would be editing the stylesheet \
                 rather than removing a dead declaration."
                    .to_string(),
            preview: vec![Change {
                path: file.clone(),
                note: format!(
                    "drop {n} @font-face rule(s): {}",
                    urls.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(edits) = plan_font_face_drops(&text, &urls_for_apply)
                {
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// The `@font-face` rules to delete: those whose single `url(` names one of
/// `urls`. `None` (decline) when nothing is deletable.
///
/// Brace matching rather than parsing. A rule with a nested `{`, or no closing
/// `}`, is left alone: a stylesheet shaped that way is not one to rewrite by
/// looking for delimiters.
fn plan_font_face_drops(css: &str, urls: &BTreeSet<String>) -> Option<Vec<MetaEdit>> {
    let mut edits: Vec<MetaEdit> = Vec::new();
    let mut from = 0usize;
    while let Some(at) = css[from..].find("@font-face") {
        let start = from + at;
        let Some(open_rel) = css[start..].find('{') else {
            break;
        };
        let open = start + open_rel;
        let Some(close_rel) = css[open + 1..].find('}') else {
            break;
        };
        let close = open + 1 + close_rel;
        let body = &css[open + 1..close];
        from = close + 1;

        // A nested `{` means the closing brace we found is not this rule's.
        if body.contains('{') {
            continue;
        }
        // Exactly one source, and it is one epubveri reported missing.
        if body.matches("url(").count() != 1 {
            continue;
        }
        if !urls.iter().any(|u| body.contains(u.as_str())) {
            continue;
        }
        edits.push(MetaEdit {
            range: with_leading_whitespace(css, start..close + 1),
            replacement: String::new(),
        });
    }
    (!edits.is_empty()).then_some(edits)
}

/// Superseded Core Media Type names, and the current name for the same format.
///
/// **This table is ours and epubveri has no equivalent** — it holds a *set* of
/// non-preferred types (`epubveri/src/cmt.rs`), so it can say a name is
/// superseded but not what supersedes it. Every target here was checked against
/// its `PREFERRED` list: a target missing from that list would give a fix that
/// does not clear its own finding.
///
/// `application/font-sfnt` is **deliberately absent**. SFNT is the container
/// both TrueType and OpenType use, so the name does not say which the file is,
/// and deciding would mean reading the font's version tag — inferring a
/// declaration from binary content rather than renaming one. It is the only
/// genuinely ambiguous member of the set.
const PREFERRED_MEDIA_TYPE: [(&str, &str); 5] = [
    ("application/vnd.ms-opentype", "font/otf"),
    ("application/x-font-ttf", "font/ttf"),
    ("application/font-woff", "font/woff"),
    ("application/ecmascript", "application/javascript"),
    ("text/javascript", "application/javascript"),
];

/// `OPF-090` / `opf.manifest_item.non_preferred_media_type`: a manifest item
/// declares a valid Core Media Type under a name the spec has superseded.
///
/// Renames the declaration to the current name for the *same* format. Nothing is
/// asserted about the bytes on disk — which is what separates this from
/// `declared_media_type_mismatch`, where the declaration and the file genuinely
/// disagree and choosing between them is not ours.
///
/// `ConfirmNeeded`: the edit is small and provably safe, but it rests on a table
/// this crate owns rather than on anything the detector told us.
fn non_preferred_media_type(report: &Report, ws: &Workspace) -> Vec<ProposedFix> {
    let files: BTreeSet<&str> = report
        .messages
        .iter()
        .filter(|m| m.rule == Some("opf.manifest_item.non_preferred_media_type"))
        .filter_map(|m| m.location.as_deref())
        .collect();

    let mut fixes = Vec::new();
    for file in files {
        let Some(text) = ws.get_text(file) else {
            continue;
        };
        let Some(edits) = compute_media_type_edits(&text) else {
            continue;
        };
        let n = edits.len();

        let file_for_apply = file.to_string();
        fixes.push(ProposedFix {
            fix_id: "fix.non_preferred_media_type",
            addresses_id: "OPF-090".to_string(),
            addresses_rule: Some("opf.manifest_item.non_preferred_media_type"),
            addresses_severity: addressed_severity(
                report,
                "OPF-090",
                Some("opf.manifest_item.non_preferred_media_type"),
            ),
            tier: Tier::ConfirmNeeded,
            title: format!("Rename {n} superseded media-type declaration(s) in {file}"),
            rationale:
                "Both names denote the same format, so this renames a declaration and asserts \
                 nothing new about the file itself. application/font-sfnt is never touched: SFNT \
                 is the container TrueType and OpenType share, so the name does not say which \
                 the file is, and deciding would mean reading the font's own bytes."
                    .to_string(),
            preview: vec![Change {
                path: file.to_string(),
                note: format!("rename {n} superseded media-type declaration(s)"),
            }],
            apply_fn: Box::new(move |ws: &mut Workspace| {
                if let Some(text) = ws.get_text(&file_for_apply)
                    && let Some(edits) = compute_media_type_edits(&text)
                {
                    ws.set_text(&file_for_apply, apply_edits(&text, edits));
                }
            }),
        });
    }
    fixes
}

/// Edits renaming every superseded `media-type` on a `<manifest>` item. `None`
/// (decline) if the package document won't parse or nothing is renameable.
fn compute_media_type_edits(opf: &str) -> Option<Vec<MetaEdit>> {
    let doc = parse_xml(opf)?;
    let manifest = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "manifest")?;

    let mut edits = Vec::new();
    for n in manifest
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
    {
        let Some(mt) = n.attribute("media-type") else {
            continue;
        };
        // Parameters are stripped before matching, as epubveri strips them —
        // but the replacement takes the whole value, so a `; charset=…` goes
        // with the old name rather than being carried onto the new one.
        let base = mt.split(';').next().unwrap_or(mt).trim();
        let Some((_, current)) = PREFERRED_MEDIA_TYPE.iter().find(|(old, _)| *old == base) else {
            continue;
        };
        let Some(attr) = n.attribute_node("media-type") else {
            continue;
        };
        edits.push(MetaEdit {
            range: attr.range(),
            replacement: format!("media-type=\"{current}\""),
        });
    }
    (!edits.is_empty()).then_some(edits)
}

/// The Dublin Core elements namespace. Matched exactly as epubveri matches it
/// (`epubveri/src/opf.rs:1217`) so the two never disagree about which element a
/// finding is about — the same principle [`compute_empty_dc_date_edits`] states.
const DC_ELEMENTS_NS: &str = "http://purl.org/dc/elements/1.1/";

/// Names this fixer will never delete, mirroring epubveri's own exclusion list.
///
/// `title`/`identifier`/`language` are **required** in EPUB 2 — deleting an empty
/// one would trade "it is empty" for "it is missing". `date` is excluded because
/// it belongs to [`empty_dc_date`]; two fixers proposing on one element would be
/// two edits to the same range.
const NEVER_DROPPED_METADATA: [&str; 4] = ["identifier", "date", "title", "language"];

/// Edits dropping every empty optional DC element, with the whitespace that
/// preceded it. `None` (decline) if the package document won't parse or nothing
/// is droppable.
fn compute_empty_metadata_edits(opf: &str) -> Option<Vec<MetaEdit>> {
    let doc = parse_xml(opf)?;
    let metadata = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "metadata")?;
    let refined: BTreeSet<&str> = doc
        .descendants()
        .filter(|n| n.is_element())
        .filter_map(|n| n.attribute("refines"))
        .filter_map(|r| r.strip_prefix('#'))
        .collect();

    let mut edits = Vec::new();
    for n in metadata.children().filter(|n| {
        n.is_element()
            && n.tag_name().namespace() == Some(DC_ELEMENTS_NS)
            && !NEVER_DROPPED_METADATA.contains(&n.tag_name().name())
    }) {
        let text: String = n
            .descendants()
            .filter(|t| t.is_text())
            .filter_map(|t| t.text())
            .collect();
        if !text.trim().is_empty() {
            continue; // it says something — never ours to delete
        }
        if n.attribute("id").is_some_and(|id| refined.contains(id)) {
            continue;
        }
        edits.push(MetaEdit {
            range: with_leading_whitespace(opf, n.range()),
            replacement: String::new(),
        });
    }
    (!edits.is_empty()).then_some(edits)
}

/// `range` extended back over the whitespace that preceded it, so dropping an
/// element on its own line doesn't leave the blank line behind.
fn with_leading_whitespace(text: &str, range: Range<usize>) -> Range<usize> {
    let start = text[..range.start].trim_end().len();
    start..range.end
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
///
/// **Declines outright on an EPUB 2 package, and that is the whole point of the
/// version read.** `properties` is an EPUB 3 attribute; OPS 2.0.1's manifest has
/// no such concept, so writing one repairs the reported defect and creates an
/// RSC-005 in its place — the exact attribute [`epub3_attrs_in_epub2_package`]
/// exists to *remove*. Measured on the 375-book shelf (2026-08-20, epubveri
/// 0.9.26): one book carried the shape, `version="2.0"` with
/// `url(res:///system/fonts/…)` in a stylesheet, and this fixer was the shelf's
/// only regression.
///
/// **The finding itself is correct, and that was checked rather than assumed.**
/// It looked like an EPUB-3-rule-leaking-into-EPUB-2 false positive — the
/// neighbouring OPF-014 sites in epubveri are guarded by `is_epub3` and the
/// stylesheet one is not — so it was raised upstream. epubveri measured both
/// versions against epubcheck 5.3.0 on one book differing only in `version`:
/// **epubcheck reports OPF-014 at 2.0 as well**, so the site is correctly
/// ungated and will not change. The neighbour differs because RSC-031 is
/// *advice* ("use https"), which aims at the wrong half of an EPUB 2 book's
/// problem; a missing declaration is not advice.
///
/// That makes this decline a statement about the **edit**, not about the
/// finding: the defect is real, and EPUB 2 simply has nowhere to record the
/// answer. Declining is the only repair available, and it would be right even if
/// the rule were reported differently tomorrow.
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
    // EPUB 2 has no `properties` attribute to add the token to. See the note above.
    if ws.get_text(&opf_path).is_some_and(|t| is_epub2_package(&t)) {
        return Vec::new();
    }

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
    let prepared = prepare_content_doc(text);
    let doc = prepared.parse()?;
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
        range: prepared.unshift(node.range()),
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
        let prepared = prepare_content_doc(&text);
        let Some(doc) = prepared.parse() else {
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
    let prepared = prepare_content_doc(text);
    let doc = prepared.parse()?;
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

/// Whether the package document declares EPUB 2 (`version="2.0"`, or the `1.x`
/// OEBPS spellings that predate it).
///
/// This exists so a fixer can refuse to write an attribute the target version
/// has no concept of. An absent or unparseable `version` reads as **not** EPUB 2:
/// the callers use it to decline, and declining on a package we cannot read
/// would be guessing in the direction that removes a repair.
fn is_epub2_package(opf: &str) -> bool {
    parse_xml(opf)
        .as_ref()
        .and_then(|d| d.root_element().attr_no_ns("version"))
        .is_some_and(|v| v.trim().starts_with('2') || v.trim().starts_with('1'))
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

    /// The parse gap: an EPUB 2 doc with an XHTML 1.1 DOCTYPE and `&nbsp;` that
    /// roxmltree can't parse on its own. The fixer must locate the empty <title>
    /// via entity-declared parsing and edit the ORIGINAL text at the right bytes.
    const NBSP_DOC: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title></title></head>\
<body><p>a&nbsp;b &mdash; c</p></body></html>";

    #[test]
    fn prepared_doc_makes_an_nbsp_document_parse() {
        assert!(
            parse_xml(NBSP_DOC).is_none(),
            "precondition: raw doc does not parse"
        );
        let prepared = prepare_content_doc(NBSP_DOC);
        assert!(prepared.inject_len > 0, "entities were declared");
        assert!(prepared.parse().is_some(), "now it parses");
    }

    #[test]
    fn title_fill_lands_correctly_through_the_parse_gap() {
        let edit = plan_title_fill(NBSP_DOC, "Chapter One").expect("fix");
        // The range is in ORIGINAL coordinates: applying it to NBSP_DOC must
        // replace exactly the empty <title>, nothing shifted.
        let out = apply_edits(NBSP_DOC, vec![edit]);
        assert!(
            out.contains("<title>Chapter One</title>"),
            "title filled: {out}"
        );
        assert!(
            out.contains("a&nbsp;b &mdash; c"),
            "body entities untouched"
        );
        assert!(out.contains("xhtml11.dtd"), "DOCTYPE untouched");
        assert!(
            !out.contains("<!ENTITY"),
            "no injected declarations leak into output"
        );
        // And the result is a well-formed EPUB 2 doc again (parses with the DTD entities declared).
        assert!(prepare_content_doc(&out).parse().is_some());
    }

    #[test]
    fn prepared_doc_is_a_noop_when_the_document_already_parses() {
        let ok = "<html><head><title></title></head><body><p>hi</p></body></html>";
        let prepared = prepare_content_doc(ok);
        assert_eq!(prepared.inject_len, 0);
        assert_eq!(
            prepared.unshift(5..9),
            5..9,
            "no shift when nothing injected"
        );
    }

    #[test]
    fn first_heading_reads_through_the_parse_gap() {
        let doc = "<?xml version=\"1.0\"?>\n\
<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Title&nbsp;Here</h1></body></html>";
        assert!(parse_xml(doc).is_none());
        // collapse_ws normalizes the resolved nbsp to a plain space.
        assert_eq!(first_heading_text(doc).as_deref(), Some("Title Here"));
    }

    const GUIDE_OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <guide>
    <reference type="cover" title="Cover" href="cover.xhtml"/>
    <reference type="text" title="Text" href="gone.html"/>
    <reference type="text" title="Text" href="ch1.xhtml"/>
    <reference type="cover" title="Cover" href="cover.xhtml"/>
  </guide>
</package>"#;

    fn unwrap_anchors(body: &str) -> Option<String> {
        let doc = format!("<html><body>{body}</body></html>");
        plan_nested_anchor_unwraps(&doc).map(|e| apply_edits(&doc, e))
    }

    /// The shelf's shape: a footnote reference whose outer anchor is a target,
    /// not a link.
    #[test]
    fn an_anchor_target_wrapped_around_a_link_is_unwrapped() {
        let out = unwrap_anchors(
            r##"<p><a id="bookmark1"><sup><a href="#footnote1">1</a></sup></a></p>"##,
        )
        .unwrap();
        assert!(
            out.contains(r##"<sup id="bookmark1"><a href="#footnote1">1</a></sup>"##),
            "{out}"
        );
    }

    /// Which of two nested links to keep is not ours to decide.
    #[test]
    fn an_outer_anchor_that_is_a_real_link_declines() {
        assert!(unwrap_anchors(r##"<p><a href="a.xhtml"><a href="#f">1</a></a></p>"##).is_none());
    }

    #[test]
    fn an_outer_anchor_carrying_more_than_an_id_declines() {
        // The class would be lost, and re-attaching it to <sup> asserts it
        // applies there.
        assert!(
            unwrap_anchors(r##"<p><a id="b" class="note"><sup><a href="#f">1</a></sup></a></p>"##)
                .is_none()
        );
    }

    #[test]
    fn a_child_that_already_has_an_id_declines() {
        assert!(
            unwrap_anchors(r##"<p><a id="b"><sup id="s"><a href="#f">1</a></sup></a></p>"##)
                .is_none()
        );
    }

    #[test]
    fn an_anchor_wrapping_more_than_one_child_declines() {
        assert!(
            unwrap_anchors(r##"<p><a id="b"><sup><a href="#f">1</a></sup><em>x</em></a></p>"##)
                .is_none()
        );
        assert!(
            unwrap_anchors(r##"<p><a id="b">text<sup><a href="#f">1</a></sup></a></p>"##).is_none()
        );
    }

    /// The id can also land on the inner anchor itself, which then carries both.
    #[test]
    fn an_anchor_directly_wrapping_an_anchor_merges_the_two() {
        let out = unwrap_anchors(r##"<p><a id="b"><a href="#f">1</a></a></p>"##).unwrap();
        assert!(out.contains(r##"<a id="b" href="#f">1</a>"##), "{out}");
    }

    #[test]
    fn an_anchor_with_no_nested_anchor_is_untouched() {
        assert!(unwrap_anchors(r##"<p><a id="b"><sup>1</sup></a></p>"##).is_none());
    }

    /// A package with the given `unique-identifier` and `<dc:identifier>` list,
    /// each entry an `(id, value)` pair where an empty id means none.
    fn ident_package(uid: &str, idents: &[(&str, &str)]) -> String {
        let body: String = idents
            .iter()
            .map(|(id, v)| {
                let attr = if id.is_empty() {
                    String::new()
                } else {
                    format!(" id=\"{id}\"")
                };
                if v.is_empty() {
                    format!("    <dc:identifier{attr}/>\n")
                } else {
                    format!("    <dc:identifier{attr}>{v}</dc:identifier>\n")
                }
            })
            .collect();
        format!(
            r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="{uid}">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
{body}    <dc:title>A Book</dc:title>
  </metadata>
</package>"#
        )
    }

    fn ident_plan(opf: &str, extra: &[(&str, &str)]) -> Option<String> {
        let mut files = vec![("OEBPS/package.opf", opf)];
        files.extend_from_slice(extra);
        let ws = container(&files);
        let plan = plan_package_identifier(opf, &ws)?;
        let mut edits = vec![MetaEdit {
            range: plan.insert_at..plan.insert_at,
            replacement: format!(" id=\"{}\"", plan.uid),
        }];
        if let Some(d) = plan.drop_empty {
            edits.push(MetaEdit {
                range: d,
                replacement: String::new(),
            });
        }
        Some(apply_edits(opf, edits))
    }

    #[test]
    fn the_declared_id_is_attached_to_the_one_real_identifier() {
        let opf = ident_package("uuid", &[("", "urn:uuid:abc")]);
        let out = ident_plan(&opf, &[]).unwrap();
        assert!(
            out.contains(r#"<dc:identifier id="uuid">urn:uuid:abc</dc:identifier>"#),
            "{out}"
        );
    }

    /// A UUID and an ISBN are both legitimate publication identifiers, and which
    /// is canonical is an editorial decision.
    #[test]
    fn two_candidate_identifiers_decline() {
        let opf = ident_package("uuid", &[("", "urn:uuid:abc"), ("", "978-1-2345-6789-0")]);
        assert!(ident_plan(&opf, &[]).is_none());
    }

    #[test]
    fn no_identifier_at_all_declines_rather_than_generating_one() {
        let opf = ident_package("uuid_id", &[]);
        assert!(ident_plan(&opf, &[]).is_none());
    }

    #[test]
    fn an_empty_declared_identifier_hands_its_id_over_and_is_dropped() {
        let opf = ident_package("bookid", &[("bookid", ""), ("", "urn:uuid:abc")]);
        let out = ident_plan(&opf, &[]).unwrap();
        assert!(
            out.contains(r#"<dc:identifier id="bookid">urn:uuid:abc</dc:identifier>"#),
            "the real identifier takes the declared id: {out}"
        );
        assert!(
            !out.contains(r#"<dc:identifier id="bookid"/>"#),
            "and the empty element is gone: {out}"
        );
    }

    #[test]
    fn an_empty_declared_identifier_with_two_siblings_declines() {
        let opf = ident_package(
            "bookid",
            &[("bookid", ""), ("", "urn:uuid:abc"), ("", "9786050918120")],
        );
        assert!(ident_plan(&opf, &[]).is_none());
    }

    /// Attaching the id to an empty element would clear OPF-030 and raise the
    /// empty-identifier finding in its place.
    #[test]
    fn a_candidate_that_is_itself_empty_declines() {
        let opf = ident_package("uuid", &[("", "")]);
        assert!(ident_plan(&opf, &[]).is_none());
    }

    #[test]
    fn an_identifier_that_already_resolves_is_left_alone() {
        let opf = ident_package("uuid", &[("uuid", "urn:uuid:abc"), ("", "isbn")]);
        assert!(ident_plan(&opf, &[]).is_none());
    }

    /// The repair is what first lets the NCX be compared against the package, so
    /// the `dtb:uid` travels in the same proposal — approving half would leave a
    /// finding epubsana created itself.
    #[test]
    fn the_ncx_uid_is_synced_in_the_same_proposal() {
        let opf = ident_package("uuid", &[("", "urn:uuid:abc")]);
        let ncx = "<ncx><head><meta name=\"dtb:uid\" content=\"stale\"/></head></ncx>";
        let ws = container(&[("OEBPS/package.opf", &opf), ("OEBPS/toc.ncx", ncx)]);
        let plan = plan_package_identifier(&opf, &ws).unwrap();
        assert_eq!(plan.ncx.len(), 1, "the NCX must come along");
        let (name, edit) = &plan.ncx[0];
        let out = apply_edits(ncx, vec![edit.clone()]);
        assert_eq!(name, "OEBPS/toc.ncx");
        assert!(out.contains("urn:uuid:abc"), "{out}");
    }

    #[test]
    fn an_ncx_that_already_agrees_produces_no_edit() {
        let opf = ident_package("uuid", &[("", "urn:uuid:abc")]);
        let ncx = "<ncx><head><meta name=\"dtb:uid\" content=\"urn:uuid:abc\"/></head></ncx>";
        let ws = container(&[("OEBPS/package.opf", &opf), ("OEBPS/toc.ncx", ncx)]);
        assert!(plan_package_identifier(&opf, &ws).unwrap().ncx.is_empty());
    }

    #[test]
    fn an_attribute_is_inserted_just_past_the_element_name() {
        assert_eq!(
            element_name_end("<dc:identifier>x</dc:identifier>", 0),
            Some(14)
        );
        assert_eq!(element_name_end("<identifier/>", 0), Some(11));
        assert_eq!(element_name_end("<dc:identifier a=\"1\"/>", 0), Some(14));
    }

    #[test]
    fn a_relative_path_is_computed_from_the_referring_documents_directory() {
        assert_eq!(
            relative_path("1/Bolum013.xhtml", "1/DiPNOTLAR.xhtml").as_deref(),
            Some("DiPNOTLAR.xhtml")
        );
        assert_eq!(
            relative_path("OEBPS/text/ch1.xhtml", "OEBPS/images/x.svg").as_deref(),
            Some("../images/x.svg")
        );
        assert_eq!(
            relative_path("ch1.xhtml", "OEBPS/text/ch2.xhtml").as_deref(),
            Some("OEBPS/text/ch2.xhtml")
        );
    }

    #[test]
    fn a_reference_is_only_matched_as_a_whole_attribute_value() {
        let t = r##"<a href="../Text/a.xhtml#k1">x</a><a href='../Text/a.xhtml#k2'>y</a>"##;
        let span = quoted_attr_span(t, "../Text/a.xhtml#k1").unwrap();
        assert_eq!(&t[span], "../Text/a.xhtml#k1");
        assert!(
            quoted_attr_span(t, "../Text/a.xhtml#k2").is_some(),
            "single quotes too"
        );
        // A prefix of a longer value must not match: the value ends at `#k1`.
        assert!(quoted_attr_span(t, "../Text/a.xhtml").is_none());
    }

    /// The book the shelf brought: a document links one directory up to a file
    /// that now sits beside it.
    #[test]
    fn a_stale_path_is_repointed_and_the_fragment_carried_across() {
        let ws = container(&[
            (
                "1/Bolum013.xhtml",
                r##"<html><body><a href="../Text/NOTES.xhtml#a8">note</a></body></html>"##,
            ),
            (
                "1/NOTES.xhtml",
                r#"<html><body><p id="a8">the note</p></body></html>"#,
            ),
        ]);
        let names: Vec<String> = ws.names().cloned().collect();
        assert_eq!(
            repointed_reference(&ws, &names, "1/Bolum013.xhtml", "../Text/NOTES.xhtml#a8")
                .as_deref(),
            Some("NOTES.xhtml#a8")
        );
    }

    /// Clearing RSC-007 by creating a dangling RSC-012 is not a repair.
    #[test]
    fn a_fragment_missing_from_the_target_declines() {
        let ws = container(&[
            ("1/ch.xhtml", "<html><body/></html>"),
            (
                "1/NOTES.xhtml",
                r#"<html><body><p id="other">x</p></body></html>"#,
            ),
        ]);
        let names: Vec<String> = ws.names().cloned().collect();
        assert!(repointed_reference(&ws, &names, "1/ch.xhtml", "../Text/NOTES.xhtml#a8").is_none());
    }

    #[test]
    fn an_ambiguous_basename_declines() {
        let ws = container(&[
            ("1/ch.xhtml", "<html><body/></html>"),
            ("1/NOTES.xhtml", r#"<html><body id="a8"/></html>"#),
            ("2/NOTES.xhtml", r#"<html><body id="a8"/></html>"#),
        ]);
        let names: Vec<String> = ws.names().cloned().collect();
        assert!(
            repointed_reference(&ws, &names, "1/ch.xhtml", "../Text/NOTES.xhtml#a8").is_none(),
            "which NOTES.xhtml it meant is a guess"
        );
    }

    #[test]
    fn a_basename_matching_nothing_declines() {
        let ws = container(&[("1/ch.xhtml", "<html><body/></html>")]);
        let names: Vec<String> = ws.names().cloned().collect();
        assert!(repointed_reference(&ws, &names, "1/ch.xhtml", "gone.xhtml").is_none());
    }

    /// The shapes that closed this rule the first time, still declined.
    #[test]
    fn external_and_junk_references_decline() {
        let ws = container(&[
            ("1/ch.xhtml", "<html><body/></html>"),
            ("1/copenhagen.htm", "<html><body/></html>"),
        ]);
        let names: Vec<String> = ws.names().cloned().collect();
        for raw in [
            "https://example.com/copenhagen.htm",
            "www.mfa.gov.tr/grupa/copenhagen.htm",
            "XXXXXXXXXXXXXXXX",
            "1/copen%20hagen.htm",
        ] {
            assert!(
                repointed_reference(&ws, &names, "1/ch.xhtml", raw).is_none(),
                "{raw} must decline"
            );
        }
    }

    /// Plan and apply the duplicate-id renames the way the fixer does.
    fn dedup_ids(doc: &str, dups: &[&str]) -> String {
        let mut used = existing_ids(doc);
        let mut out = doc.to_string();
        for dup in dups {
            let occ = attr_occurrences(doc, dup);
            if occ < 2 {
                continue;
            }
            let stem = sanitize_ncname(dup).unwrap_or_else(|| (*dup).to_string());
            let news: Vec<String> = (1..occ)
                .map(|_| {
                    let new = make_unique(stem.clone(), &used);
                    used.insert(new.clone());
                    new
                })
                .collect();
            out = rename_later_id_occurrences(&out, dup, &news);
        }
        out
    }

    #[test]
    fn the_first_occurrence_keeps_the_id_and_later_ones_are_renamed() {
        let doc =
            r#"<html><body><p id="a">one</p><p id="a">two</p><p id="a">three</p></body></html>"#;
        let out = dedup_ids(doc, &["a"]);
        assert_eq!(
            out,
            r#"<html><body><p id="a">one</p><p id="a-2">two</p><p id="a-3">three</p></body></html>"#
        );
    }

    /// The load-bearing claim of the spec: a fragment already resolves to the
    /// first element carrying the id, so keeping the first means every link
    /// still points where it pointed.
    #[test]
    fn a_reference_to_the_duplicated_id_still_resolves_to_the_same_element() {
        let doc = r##"<html><body><a href="#a">go</a><p id="a">first</p><p id="a">second</p></body></html>"##;
        let out = dedup_ids(doc, &["a"]);
        assert!(
            out.contains(r##"href="#a""##),
            "the reference is untouched: {out}"
        );
        assert!(
            out.contains(r#"<p id="a">first</p>"#),
            "and its target is unchanged: {out}"
        );
        assert!(out.contains(r#"id="a-2""#));
    }

    #[test]
    fn a_new_name_avoids_ids_already_in_the_document() {
        let doc =
            r#"<html><body><p id="a">1</p><p id="a">2</p><p id="a-2">taken</p></body></html>"#;
        let out = dedup_ids(doc, &["a"]);
        assert!(out.contains(r#"id="a-3""#), "must skip a-2: {out}");
        assert!(
            out.contains(r#"id="a-2">taken"#),
            "the squatter is untouched"
        );
    }

    /// A value that is both duplicated and not an NCName must not yield more
    /// invalid names than it found.
    #[test]
    fn a_duplicated_invalid_ncname_is_renamed_from_a_sanitized_stem() {
        let doc = r#"<html><body><p id="09">1</p><p id="09">2</p></body></html>"#;
        let out = dedup_ids(doc, &["09"]);
        assert!(out.contains(r#"id="id_09""#), "{out}");
        assert!(
            out.contains(r#"<p id="09">1</p>"#),
            "the first keeps its value, for the invalid-id fixer to consider"
        );
    }

    #[test]
    fn a_stale_finding_renames_nothing() {
        // The value no longer occurs twice — an earlier fix already moved it.
        let doc = r#"<html><body><p id="a">only</p></body></html>"#;
        assert_eq!(dedup_ids(doc, &["a"]), doc);
    }

    #[test]
    fn only_the_reported_id_is_touched() {
        let doc = r#"<html><body><p id="a">1</p><p id="a">2</p><p id="b">3</p><p id="b">4</p></body></html>"#;
        let out = dedup_ids(doc, &["a"]);
        assert!(out.contains(r#"id="a-2""#));
        assert!(
            out.contains(r#"<p id="b">3</p><p id="b">4</p>"#),
            "b was not reported, so it is left alone: {out}"
        );
    }

    /// An EPUB 2 package with `{cover}` and `{spine}` substituted in.
    fn package(cover_item: &str, meta: &str, spine: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>A Book</dc:title>
    {meta}
  </metadata>
  <manifest>
    {cover_item}
    <item href="ch1.xhtml" id="ch1" media-type="application/xhtml+xml"/>
  </manifest>
  <spine {spine}><itemref idref="ch1"/></spine>
</package>"#
        )
    }

    fn droppable(opf: &str, attrs: &[&str]) -> Vec<String> {
        let set: BTreeSet<String> = attrs.iter().map(|s| s.to_string()).collect();
        compute_epub3_attr_edits(opf, &set)
            .map(|v| v.into_iter().map(|d| d.name).collect())
            .unwrap_or_default()
    }

    const COVER_PROPS: &str =
        r#"<item href="c.jpg" id="cover-image" media-type="image/jpeg" properties="cover-image"/>"#;
    const COVER_META: &str = r#"<meta name="cover" content="cover-image"/>"#;

    #[test]
    fn a_redundant_cover_property_is_dropped() {
        let opf = package(COVER_PROPS, COVER_META, r#"toc="ncx""#);
        assert_eq!(droppable(&opf, &["properties"]), vec!["properties"]);
        let set = BTreeSet::from(["properties".to_string()]);
        let edits = compute_epub3_attr_edits(&opf, &set)
            .unwrap()
            .into_iter()
            .map(|d| d.edit)
            .collect();
        let out = apply_edits(&opf, edits);
        assert!(!out.contains("properties"));
        assert!(
            out.contains(r#"id="cover-image" media-type="image/jpeg"/>"#),
            "the attribute goes with its leading space: {out}"
        );
        assert!(out.contains(COVER_META), "the EPUB 2 declaration stays");
    }

    #[test]
    fn a_cover_property_with_no_legacy_meta_is_declined() {
        // Then the attribute is the ONLY cover declaration — dropping it loses
        // the cover.
        let opf = package(COVER_PROPS, "", r#"toc="ncx""#);
        assert!(droppable(&opf, &["properties"]).is_empty());
    }

    #[test]
    fn a_cover_property_whose_meta_names_another_item_is_declined() {
        let opf = package(
            COVER_PROPS,
            r#"<meta name="cover" content="some-other-item"/>"#,
            r#"toc="ncx""#,
        );
        assert!(droppable(&opf, &["properties"]).is_empty());
    }

    #[test]
    fn a_property_that_is_not_the_cover_is_declined() {
        // EPUB 2 has no equivalent declaration for these, so dropping one would
        // discard a real claim about the document.
        for token in ["nav", "mathml", "scripted", "cover-image mathml"] {
            let item = format!(
                r#"<item href="c.xhtml" id="cover-image" media-type="application/xhtml+xml" properties="{token}"/>"#
            );
            let opf = package(&item, COVER_META, r#"toc="ncx""#);
            assert!(
                droppable(&opf, &["properties"]).is_empty(),
                "properties={token:?} must decline"
            );
        }
    }

    #[test]
    fn a_default_page_progression_direction_is_dropped() {
        let opf = package(
            COVER_PROPS,
            COVER_META,
            r#"toc="ncx" page-progression-direction="ltr""#,
        );
        assert_eq!(
            droppable(&opf, &["page-progression-direction"]),
            vec!["page-progression-direction"]
        );
    }

    #[test]
    fn a_right_to_left_reading_direction_is_declined() {
        // Authored information EPUB 2 has nowhere to put — a reason to leave the
        // book alone, not to erase it.
        let opf = package(
            COVER_PROPS,
            COVER_META,
            r#"toc="ncx" page-progression-direction="rtl""#,
        );
        assert!(droppable(&opf, &["page-progression-direction"]).is_empty());
    }

    #[test]
    fn only_the_attributes_the_finding_named_are_considered() {
        let opf = package(
            COVER_PROPS,
            COVER_META,
            r#"toc="ncx" page-progression-direction="ltr""#,
        );
        // The finding named only `properties`, so the spine attribute is not
        // touched even though it would qualify.
        assert_eq!(droppable(&opf, &["properties"]), vec!["properties"]);
    }

    /// A container holding exactly the files given, so a cross-file rename can
    /// be exercised end to end rather than helper by helper.
    fn container(files: &[(&str, &str)]) -> Workspace {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("mimetype", opts).unwrap();
            zip.write_all(b"application/epub+zip").unwrap();
            for (name, body) in files {
                zip.start_file(*name, opts).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        Workspace::load(&buf).unwrap()
    }

    fn planned(ws: &Workspace, doc: &str, bad: &[&str]) -> Option<IdRenamePlan> {
        let values: BTreeSet<String> = bad.iter().map(|s| s.to_string()).collect();
        plan_id_renames(ws, doc, &values)
    }

    /// Apply a plan and return the resulting text of one file.
    fn applied(ws: &Workspace, plan: &IdRenamePlan, file: &str) -> String {
        let text = ws.get_text(file).unwrap();
        let edits = plan.edits.get(file).cloned().unwrap_or_default();
        apply_edits(&text, edits)
    }

    #[test]
    fn invalid_id_is_sanitized_and_its_own_fragment_moves_with_it() {
        let ws = container(&[(
            "ch1.xhtml",
            r##"<html><body><p id="09">x</p><a href="#09">back</a></body></html>"##,
        )]);
        let plan = planned(&ws, "ch1.xhtml", &["09"]).unwrap();
        assert_eq!(plan.renames, vec![("09".into(), "id_09".into())]);
        let out = applied(&ws, &plan, "ch1.xhtml");
        assert!(out.contains(r#"id="id_09""#));
        assert!(
            out.contains(r##"href="#id_09""##),
            "the link must follow the anchor: {out}"
        );
    }

    #[test]
    fn a_reference_from_another_document_is_rewritten() {
        let ws = container(&[
            ("ch1.xhtml", r#"<html><body><p id="09">x</p></body></html>"#),
            ("toc.ncx", r##"<ncx><content src="ch1.xhtml#09"/></ncx>"##),
        ]);
        let plan = planned(&ws, "ch1.xhtml", &["09"]).unwrap();
        assert!(applied(&ws, &plan, "toc.ncx").contains(r##"src="ch1.xhtml#id_09""##));
    }

    /// The measured hazard: six values on the shelf are carried by 6–12
    /// documents of one book, so a global `#value` rewrite would move links that
    /// mean a *different* document's identically-named id.
    #[test]
    fn an_identically_named_id_in_another_document_is_left_alone() {
        let ws = container(&[
            (
                "ch1.xhtml",
                r#"<html><body><p id="09">one</p></body></html>"#,
            ),
            (
                "ch2.xhtml",
                r##"<html><body><p id="09">two</p><a href="#09">its own</a></body></html>"##,
            ),
        ]);
        let plan = planned(&ws, "ch1.xhtml", &["09"]).unwrap();
        let ch2 = applied(&ws, &plan, "ch2.xhtml");
        assert!(
            ch2.contains(r##"href="#09""##) && ch2.contains(r#"id="09""#),
            "ch2's own anchor and its own link are untouched: {ch2}"
        );
    }

    #[test]
    fn a_reference_resolves_against_the_referring_files_directory() {
        let ws = container(&[
            (
                "OEBPS/text/ch1.xhtml",
                r#"<html><body><p id="09">x</p></body></html>"#,
            ),
            (
                "OEBPS/toc.ncx",
                r##"<ncx><content src="text/ch1.xhtml#09"/></ncx>"##,
            ),
            (
                "OEBPS/text/ch2.xhtml",
                r##"<html><body><a href="ch1.xhtml#09">x</a></body></html>"##,
            ),
        ]);
        let plan = planned(&ws, "OEBPS/text/ch1.xhtml", &["09"]).unwrap();
        assert!(applied(&ws, &plan, "OEBPS/toc.ncx").contains("#id_09"));
        assert!(applied(&ws, &plan, "OEBPS/text/ch2.xhtml").contains("#id_09"));
    }

    #[test]
    fn an_unclassifiable_fragment_declines_the_rename() {
        // `#09` in a stylesheet is an id selector we cannot rewrite with
        // confidence, so the id is not renamed at all.
        let ws = container(&[
            ("ch1.xhtml", r#"<html><body><p id="09">x</p></body></html>"#),
            ("style.css", "#09 { color: red }"),
        ]);
        assert!(planned(&ws, "ch1.xhtml", &["09"]).is_none());
    }

    /// A cover image reliably contains the bytes `#1` somewhere. Scanning it
    /// cost ten repairable ids on a real book before this filter existed.
    #[test]
    fn binary_entries_are_not_scanned_for_references() {
        assert!(!can_reference_a_fragment("images/cover.jpeg"));
        assert!(can_reference_a_fragment("OEBPS/toc.ncx"));
        assert!(can_reference_a_fragment("Styles/main.CSS"));
    }

    #[test]
    fn a_longer_id_that_merely_starts_with_the_value_is_not_matched() {
        // `#09` must not be seen inside `#0912`, nor inside a CSS colour.
        assert_eq!(fragment_spans(r##"href="#0912""##, "09").len(), 0);
        assert_eq!(fragment_spans("color:#0099ff", "09").len(), 0);
        assert_eq!(fragment_spans(r##"href="#09""##, "09").len(), 1);
    }

    #[test]
    fn a_new_name_never_collides_with_an_existing_id() {
        let ws = container(&[(
            "ch1.xhtml",
            r#"<html><body><p id="09">x</p><p id="id_09">taken</p></body></html>"#,
        )]);
        let plan = planned(&ws, "ch1.xhtml", &["09"]).unwrap();
        assert_eq!(plan.renames[0].1, "id_09-2");
    }

    #[test]
    fn two_elements_sharing_the_invalid_id_are_declined() {
        // That is a duplicate-id defect, and which element a reference meant is
        // not ours to guess.
        let ws = container(&[(
            "ch1.xhtml",
            r#"<html><body><p id="09">a</p><p id="09">b</p></body></html>"#,
        )]);
        assert!(planned(&ws, "ch1.xhtml", &["09"]).is_none());
    }

    #[test]
    fn resolve_against_handles_dot_dot_and_refuses_percent_encoding() {
        assert_eq!(
            resolve_against("OEBPS/toc.ncx", "text/ch1.xhtml").as_deref(),
            Some("OEBPS/text/ch1.xhtml")
        );
        assert_eq!(
            resolve_against("OEBPS/text/ch2.xhtml", "../images/x.svg").as_deref(),
            Some("OEBPS/images/x.svg")
        );
        assert_eq!(resolve_against("a/b.xhtml", "c%20d.xhtml"), None);
    }

    /// One empty date, one malformed-but-real date, one valid one. The middle
    /// element is the point of the whole fixer: `OPF-054` reports it, and we
    /// still must not touch it.
    const DATE_OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>A Book</dc:title>
    <dc:date opf:event="publication">2019-10-31</dc:date>
    <dc:date opf:event="modification">March 2019</dc:date>
    <dc:date opf:event="creation"></dc:date>
  </metadata>
</package>"#;

    #[test]
    fn font_face_drops_the_rule_whose_only_source_is_missing() {
        let css = "body { color: black }\n\n@font-face {\n  font-family: \"Arial\";\n  src: url(../Fonts/arial.ttf);\n}\n\np { margin: 0 }";
        let urls = BTreeSet::from(["../Fonts/arial.ttf".to_string()]);
        let out = apply_edits(css, plan_font_face_drops(css, &urls).unwrap());
        assert!(!out.contains("@font-face"));
        assert!(out.contains("body { color: black }"));
        assert!(out.contains("p { margin: 0 }"));
    }

    #[test]
    fn font_face_leaves_a_rule_whose_font_is_present() {
        // Only the url epubveri reported is ours; a sibling rule stays.
        let css = "@font-face { src: url(a.ttf); }\n@font-face { src: url(b.ttf); }";
        let urls = BTreeSet::from(["b.ttf".to_string()]);
        let out = apply_edits(css, plan_font_face_drops(css, &urls).unwrap());
        assert!(out.contains("url(a.ttf)"));
        assert!(!out.contains("url(b.ttf)"));
    }

    #[test]
    fn font_face_declines_a_rule_with_a_second_source() {
        // The missing woff may sit beside a ttf that works; cutting one line is
        // editing CSS, not deleting a dead rule.
        let css = "@font-face { src: url(x.woff), url(x.ttf); }";
        let urls = BTreeSet::from(["x.woff".to_string()]);
        assert!(plan_font_face_drops(css, &urls).is_none());
    }

    #[test]
    fn font_face_declines_when_the_braces_do_not_delimit_a_rule() {
        let unclosed = "@font-face { src: url(x.ttf);";
        let nested = "@font-face { src: url(x.ttf); a { b: c } }";
        let urls = BTreeSet::from(["x.ttf".to_string()]);
        assert!(plan_font_face_drops(unclosed, &urls).is_none());
        assert!(plan_font_face_drops(nested, &urls).is_none());
    }

    #[test]
    fn font_face_ignores_a_matching_url_outside_a_font_face_rule() {
        let css = "body { background: url(../Fonts/arial.ttf) }";
        let urls = BTreeSet::from(["../Fonts/arial.ttf".to_string()]);
        assert!(plan_font_face_drops(css, &urls).is_none());
    }

    #[test]
    fn media_type_renames_the_unambiguous_ones() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf"><manifest>
    <item id="f1" href="a.otf" media-type="application/vnd.ms-opentype"/>
    <item id="s1" href="s.js" media-type="text/javascript"/>
    <item id="c1" href="c.xhtml" media-type="application/xhtml+xml"/>
  </manifest></package>"#;
        let out = apply_edits(opf, compute_media_type_edits(opf).unwrap());
        assert!(out.contains(r#"media-type="font/otf""#));
        assert!(out.contains(r#"media-type="application/javascript""#));
        assert!(
            out.contains(r#"media-type="application/xhtml+xml""#),
            "a type that is already preferred is not touched"
        );
        assert!(!out.contains("vnd.ms-opentype"));
    }

    #[test]
    fn media_type_declines_the_ambiguous_sfnt() {
        // SFNT is the container TrueType and OpenType share: the name cannot
        // say which the file is, and this fixer never reads the file.
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf"><manifest>
    <item id="f" href="a.font" media-type="application/font-sfnt"/>
  </manifest></package>"#;
        assert!(compute_media_type_edits(opf).is_none());
    }

    #[test]
    fn media_type_matches_past_a_parameter_and_drops_it_with_the_old_name() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf"><manifest>
    <item id="s" href="s.js" media-type="text/javascript; charset=utf-8"/>
  </manifest></package>"#;
        let out = apply_edits(opf, compute_media_type_edits(opf).unwrap());
        assert!(out.contains(r#"media-type="application/javascript""#));
        assert!(
            !out.contains("charset=utf-8"),
            "the parameter belonged to the old name"
        );
    }

    #[test]
    fn media_type_leaves_a_matching_string_outside_the_manifest_alone() {
        // Only <manifest> items declare resource types; a stray occurrence
        // elsewhere is not this fixer's business.
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf"><metadata>
    <meta name="note" content="text/javascript"/>
  </metadata><manifest/></package>"#;
        assert!(compute_media_type_edits(opf).is_none());
    }

    #[test]
    fn empty_metadata_drops_both_shapes_and_keeps_what_speaks() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:source/>
    <dc:coverage></dc:coverage>
    <dc:rights>© 2019</dc:rights>
  </metadata></package>"#;
        let out = apply_edits(opf, compute_empty_metadata_edits(opf).unwrap());
        assert!(
            !out.contains("dc:source"),
            "self-closing empty element dropped"
        );
        assert!(!out.contains("dc:coverage"), "empty pair dropped");
        assert!(
            out.contains("<dc:rights>© 2019</dc:rights>"),
            "a value is never touched"
        );
    }

    #[test]
    fn empty_metadata_never_drops_a_required_element() {
        // epubveri excludes these, so this can only fire if that list moves —
        // which is exactly why the guard is here.
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title/>
    <dc:identifier/>
    <dc:language/>
  </metadata></package>"#;
        assert!(
            compute_empty_metadata_edits(opf).is_none(),
            "deleting an empty required element trades 'empty' for 'missing'"
        );
    }

    #[test]
    fn empty_metadata_leaves_dc_date_to_its_own_fixer() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:date/>
  </metadata></package>"#;
        assert!(
            compute_empty_metadata_edits(opf).is_none(),
            "two fixers proposing on one element would be two edits to one range"
        );
    }

    #[test]
    fn empty_metadata_declines_when_a_meta_refines_the_element() {
        let opf = r##"<package xmlns="http://www.idpf.org/2007/opf"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:source id="src"/>
    <meta refines="#src" property="role">aut</meta>
  </metadata></package>"##;
        assert!(
            compute_empty_metadata_edits(opf).is_none(),
            "dropping the target would orphan the refinement"
        );
    }

    #[test]
    fn empty_metadata_ignores_a_lookalike_in_another_namespace() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:x="http://example.invalid/">
    <x:source/>
  </metadata></package>"#;
        assert!(
            compute_empty_metadata_edits(opf).is_none(),
            "only Dublin Core elements are ours; the namespace is what decides"
        );
    }

    #[test]
    fn empty_metadata_leaves_no_blank_line_behind() {
        let opf = "<package xmlns=\"http://www.idpf.org/2007/opf\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n    <dc:source/>\n    <dc:rights>x</dc:rights>\n  </metadata></package>";
        let out = apply_edits(opf, compute_empty_metadata_edits(opf).unwrap());
        assert!(
            !out.contains("\n\n"),
            "the whitespace before the element goes with it"
        );
    }

    #[test]
    fn empty_dc_date_drops_only_the_empty_one() {
        let edits = compute_empty_dc_date_edits(DATE_OPF).unwrap();
        assert_eq!(edits.len(), 1, "exactly one <dc:date> is empty");
        let out = apply_edits(DATE_OPF, edits);
        assert!(
            !out.contains("creation"),
            "the empty element is gone, attributes and all"
        );
        assert!(
            out.contains("March 2019"),
            "a malformed but real date is authored information — never dropped"
        );
        assert!(out.contains("2019-10-31"), "the valid date is untouched");
    }

    #[test]
    fn empty_dc_date_declines_a_whitespace_only_value_is_still_empty() {
        let opf = DATE_OPF.replace(
            "<dc:date opf:event=\"creation\"></dc:date>",
            "<dc:date>\n   \n  </dc:date>",
        );
        let edits = compute_empty_dc_date_edits(&opf).unwrap();
        assert_eq!(edits.len(), 1, "whitespace is not a date");
    }

    #[test]
    fn empty_dc_date_declines_when_every_date_carries_a_value() {
        let opf = DATE_OPF.replace("<dc:date opf:event=\"creation\"></dc:date>", "");
        assert!(
            compute_empty_dc_date_edits(&opf).is_none(),
            "nothing empty — decline rather than propose a no-op"
        );
    }

    /// The finding can be real (a malformed date) while the fix declines. The
    /// finding then survives the repair, which is the honest outcome.
    #[test]
    fn empty_dc_date_declines_a_book_whose_only_defect_is_a_malformed_date() {
        let opf = DATE_OPF
            .replace("<dc:date opf:event=\"creation\"></dc:date>", "")
            .replace("2019-10-31", "2022-09-08)");
        assert!(compute_empty_dc_date_edits(&opf).is_none());
    }

    #[test]
    fn empty_dc_date_declines_when_a_meta_refines_the_element() {
        let opf = DATE_OPF.replace(
            "<dc:date opf:event=\"creation\"></dc:date>",
            "<dc:date id=\"d1\"></dc:date>\n    <meta refines=\"#d1\" property=\"x\">y</meta>",
        );
        assert!(
            compute_empty_dc_date_edits(&opf).is_none(),
            "dropping it would orphan the refinement"
        );
    }

    #[test]
    fn empty_dc_date_leaves_no_blank_line_behind() {
        let edits = compute_empty_dc_date_edits(DATE_OPF).unwrap();
        let out = apply_edits(DATE_OPF, edits);
        assert!(
            !out.contains("\n\n"),
            "the whitespace that preceded the element goes with it: {out}"
        );
    }

    #[test]
    fn guide_dangling_drops_only_the_missing_reference() {
        let hrefs = BTreeSet::from(["gone.html".to_string()]);
        let (edits, dropped_guide, n) = compute_guide_dangling_edits(GUIDE_OPF, &hrefs).unwrap();
        assert!(!dropped_guide);
        assert_eq!(n, 1);
        let out = apply_edits(GUIDE_OPF, edits);
        assert!(!out.contains("gone.html"));
        assert!(out.contains("cover.xhtml") && out.contains("ch1.xhtml"));
    }

    #[test]
    fn guide_dangling_drops_the_whole_guide_when_all_references_are_missing() {
        // Every reference's href is flagged.
        let hrefs = BTreeSet::from([
            "cover.xhtml".to_string(),
            "gone.html".to_string(),
            "ch1.xhtml".to_string(),
        ]);
        let (edits, dropped_guide, _) = compute_guide_dangling_edits(GUIDE_OPF, &hrefs).unwrap();
        assert!(
            dropped_guide,
            "an empty guide is invalid — drop the element"
        );
        let out = apply_edits(GUIDE_OPF, edits);
        assert!(!out.contains("<guide"), "the guide element is gone");
    }

    #[test]
    fn guide_dangling_declines_when_no_reference_matches() {
        let hrefs = BTreeSet::from(["nowhere.xhtml".to_string()]);
        assert!(compute_guide_dangling_edits(GUIDE_OPF, &hrefs).is_none());
    }

    #[test]
    fn guide_duplicate_keeps_first_of_each_identical_pair() {
        // The two type="cover" href="cover.xhtml" are duplicates; the two
        // type="text" have DIFFERENT hrefs and are not.
        let edits = compute_guide_duplicate_edits(GUIDE_OPF).unwrap();
        assert_eq!(edits.len(), 1, "only the repeated cover is a duplicate");
        let out = apply_edits(GUIDE_OPF, edits);
        assert_eq!(out.matches("type=\"cover\"").count(), 1);
        assert_eq!(
            out.matches("type=\"text\"").count(),
            2,
            "different hrefs are not duplicates"
        );
    }

    #[test]
    fn guide_duplicate_declines_when_nothing_repeats() {
        let opf = r#"<package><guide><reference type="cover" href="c.xhtml"/><reference type="text" href="t.xhtml"/></guide></package>"#;
        assert!(compute_guide_duplicate_edits(opf).is_none());
    }

    /// A guide with one reference carrying a fragment, in an OPF that sits in a
    /// subdirectory — so the test also pins the `params[1]` resolution.
    const FRAG_OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <guide>
    <reference type="toc" title="Contents" href="Text/ch1.html#filepos16691"/>
    <reference type="cover" title="Cover" href="Text/cover.xhtml"/>
  </guide>
</package>"#;

    fn flagged(pairs: &[(&str, &str)]) -> BTreeSet<(String, String)> {
        pairs
            .iter()
            .map(|(f, t)| (f.to_string(), t.to_string()))
            .collect()
    }

    #[test]
    fn guide_fragment_is_dropped_and_the_path_is_kept() {
        let f = flagged(&[("filepos16691", "OEBPS/Text/ch1.html")]);
        let rewrites = compute_guide_fragment_edits(FRAG_OPF, "OEBPS/content.opf", &f).unwrap();
        assert_eq!(rewrites.len(), 1);
        assert_eq!(rewrites[0].0, "Text/ch1.html#filepos16691");
        assert_eq!(rewrites[0].1, "Text/ch1.html");

        let out = apply_edits(FRAG_OPF, rewrites.into_iter().map(|(_, _, e)| e).collect());
        assert!(out.contains(r#"href="Text/ch1.html""#));
        assert!(!out.contains("filepos16691"), "the fragment is gone: {out}");
        // The rest of the reference — and the rest of the guide — is untouched.
        assert!(out.contains(r#"type="toc" title="Contents""#));
        assert!(out.contains(r#"href="Text/cover.xhtml""#));
    }

    #[test]
    fn guide_fragment_declines_when_the_target_does_not_match() {
        // Right fragment, wrong document: this finding is about another book's
        // reference, so nothing here is touched.
        let f = flagged(&[("filepos16691", "OEBPS/Text/elsewhere.html")]);
        assert!(compute_guide_fragment_edits(FRAG_OPF, "OEBPS/content.opf", &f).is_none());
    }

    #[test]
    fn guide_fragment_declines_when_the_fragment_does_not_match() {
        let f = flagged(&[("other", "OEBPS/Text/ch1.html")]);
        assert!(compute_guide_fragment_edits(FRAG_OPF, "OEBPS/content.opf", &f).is_none());
    }

    #[test]
    fn guide_fragment_declines_rather_than_creating_a_duplicate_reference() {
        // Dropping the fragment would make this reference identical to the
        // second one — clearing an RSC-012 by creating an RSC-017.
        let opf = FRAG_OPF.replace(
            r#"<reference type="cover" title="Cover" href="Text/cover.xhtml"/>"#,
            r#"<reference type="toc" title="Contents" href="Text/ch1.html"/>"#,
        );
        let f = flagged(&[("filepos16691", "OEBPS/Text/ch1.html")]);
        assert!(
            compute_guide_fragment_edits(&opf, "OEBPS/content.opf", &f).is_none(),
            "the repair would introduce a duplicate guide reference"
        );
    }

    #[test]
    fn guide_fragment_declines_a_pair_that_would_collide_with_each_other() {
        // Two flagged references that would become identical: both are left
        // alone rather than silently merged.
        let opf = FRAG_OPF.replace(
            r#"<reference type="cover" title="Cover" href="Text/cover.xhtml"/>"#,
            r#"<reference type="toc" title="Contents" href="Text/ch1.html#filepos99"/>"#,
        );
        let f = flagged(&[
            ("filepos16691", "OEBPS/Text/ch1.html"),
            ("filepos99", "OEBPS/Text/ch1.html"),
        ]);
        assert!(compute_guide_fragment_edits(&opf, "OEBPS/content.opf", &f).is_none());
    }

    #[test]
    fn guide_fragment_repairs_the_others_when_one_collides() {
        // One flagged reference collides; a second, on a different document,
        // does not — and is still repaired.
        let opf = FRAG_OPF.replace(
            r#"<reference type="cover" title="Cover" href="Text/cover.xhtml"/>"#,
            r#"<reference type="toc" title="Contents" href="Text/ch1.html"/>
    <reference type="text" title="Start" href="Text/ch2.html#nope"/>"#,
        );
        let f = flagged(&[
            ("filepos16691", "OEBPS/Text/ch1.html"),
            ("nope", "OEBPS/Text/ch2.html"),
        ]);
        let rewrites = compute_guide_fragment_edits(&opf, "OEBPS/content.opf", &f).unwrap();
        assert_eq!(rewrites.len(), 1, "only the non-colliding one is repaired");
        assert_eq!(rewrites[0].1, "Text/ch2.html");
    }

    #[test]
    fn guide_fragment_leaves_a_fragmentless_reference_alone() {
        // A reference with no `#` can never be this defect, whatever is flagged.
        let opf =
            r#"<package><guide><reference type="toc" href="Text/ch1.html"/></guide></package>"#;
        let f = flagged(&[("filepos16691", "Text/ch1.html")]);
        assert!(compute_guide_fragment_edits(opf, "content.opf", &f).is_none());
    }

    #[test]
    fn play_order_renumbers_duplicates_by_document_order() {
        let ncx = r#"<navMap><navPoint playOrder="1"><navPoint playOrder="1"/></navPoint><navPoint playOrder="1"/></navMap>"#;
        let (out, n) = renumber_play_order(ncx);
        assert_eq!(n, 3);
        // Document order: outer=1, its child=2, next sibling=3.
        assert_eq!(
            out,
            r#"<navMap><navPoint playOrder="1"><navPoint playOrder="2"/></navPoint><navPoint playOrder="3"/></navMap>"#
        );
    }

    #[test]
    fn play_order_leaves_a_document_with_none_unchanged() {
        let t = "<navMap><navPoint id=\"a\"/></navMap>";
        let (out, n) = renumber_play_order(t);
        assert_eq!(n, 0);
        assert_eq!(out, t);
    }

    #[test]
    fn play_order_handles_single_quotes() {
        // One root, because the renumbering now parses the NCX rather than
        // scanning it — see `an_ncx_that_will_not_parse_is_declined`.
        let (out, n) = renumber_play_order("<navMap><x playOrder='5'/><y playOrder='5'/></navMap>");
        assert_eq!(n, 2);
        assert_eq!(out, "<navMap><x playOrder='1'/><y playOrder='2'/></navMap>");
    }

    /// Elements naming the same target must carry the *same* playOrder — one
    /// position reached by two routes. The old scan numbered by file position and
    /// would have created `ncx.play_order.target_mismatch` here.
    #[test]
    fn play_order_gives_one_target_one_number() {
        let ncx = r#"<navMap><navPoint playOrder="9"><content src="a.xhtml"/></navPoint><navPoint playOrder="4"><content src="b.xhtml"/></navPoint><navPoint playOrder="7"><content src="a.xhtml"/></navPoint></navMap>"#;
        let (out, n) = renumber_play_order(ncx);
        assert_eq!(n, 3);
        assert_eq!(
            out,
            r#"<navMap><navPoint playOrder="1"><content src="a.xhtml"/></navPoint><navPoint playOrder="2"><content src="b.xhtml"/></navPoint><navPoint playOrder="1"><content src="a.xhtml"/></navPoint></navMap>"#,
            "a.xhtml keeps the number it was first given; numbering stays dense"
        );
    }

    /// Dense from 1, so a gap cannot survive the renumbering either.
    #[test]
    fn play_order_closes_gaps() {
        let ncx = r#"<navMap><navPoint playOrder="1"><content src="a.xhtml"/></navPoint><navPoint playOrder="17"><content src="b.xhtml"/></navPoint></navMap>"#;
        let (out, _) = renumber_play_order(ncx);
        assert!(out.contains(r#"playOrder="2""#), "{out}");
    }

    /// The real shape from the shelf: an NCX at the container root pointing into
    /// a directory the file does not live in, while exactly one entry carries the
    /// basename.
    fn ncx_repoint_ws() -> Workspace {
        container(&[
            (
                "toc.ncx",
                r#"<ncx><navMap><navPoint id="n1" playOrder="1"><navLabel><text>T</text></navLabel><content src="OEBPS/Text/titlepage.xhtml"/></navPoint></navMap></ncx>"#,
            ),
            ("titlepage.xhtml", "<html><body><p>t</p></body></html>"),
            (
                "OEBPS/Text/Section0001.xhtml",
                "<html><body><p>s</p></body></html>",
            ),
        ])
    }

    fn ncx_missing_resource(ncx: &str, raw: &str) -> Report {
        let mut report = Report::default();
        report.messages = vec![epubveri::report::Message {
            id: "RSC-007",
            severity: Severity::Error,
            text: String::new(),
            location: Some(ncx.to_string()),
            position: None,
            rule: Some("opf.ncx.content_src_missing_resource"),
            params: vec![raw.to_string()],
            element_path: None,
        }];
        report
    }

    #[test]
    fn an_ncx_src_is_repointed_when_one_entry_carries_the_name() {
        let ws = ncx_repoint_ws();
        let fixes = ncx_src_wrong_path(
            &ncx_missing_resource("toc.ncx", "OEBPS/Text/titlepage.xhtml"),
            &ws,
        );
        assert_eq!(fixes.len(), 1, "one proposal for the NCX");
        assert!(
            fixes[0].preview[0].note.contains("→ titlepage.xhtml"),
            "repointed relative to the NCX: {}",
            fixes[0].preview[0].note
        );
    }

    /// The guard that carries the whole family: two entries with the same
    /// basename make the target a guess.
    #[test]
    fn an_ambiguous_basename_is_never_repointed() {
        let ws = container(&[
            (
                "toc.ncx",
                r#"<ncx><navMap><navPoint><content src="gone/page.xhtml"/></navPoint></navMap></ncx>"#,
            ),
            ("a/page.xhtml", "<html/>"),
            ("b/page.xhtml", "<html/>"),
        ]);
        assert!(
            ncx_src_wrong_path(&ncx_missing_resource("toc.ncx", "gone/page.xhtml"), &ws).is_empty(),
            "two candidates: which one it meant is a guess"
        );
    }

    /// The overwhelming majority of this rule's findings on the shelf: the file
    /// is simply not in the book, so there is nothing to repair toward.
    #[test]
    fn an_absent_file_leaves_the_navigation_alone() {
        let ws = ncx_repoint_ws();
        assert!(
            ncx_src_wrong_path(&ncx_missing_resource("toc.ncx", "Text/main-1.xhtml"), &ws)
                .is_empty()
        );
    }

    /// Clearing RSC-007 by creating a dangling RSC-012 is not a repair.
    ///
    /// **The NCX must carry the fragment too, or this test passes for the wrong
    /// reason** — verified by deleting the fragment guard and watching it stay
    /// green. With a fragment-less `src` in the file, `quoted_attr_span` fails to
    /// match the reported value and the fixer declines before the guard is ever
    /// consulted.
    #[test]
    fn a_fragment_the_target_lacks_declines_the_repoint() {
        let ws = container(&[
            (
                "toc.ncx",
                r#"<ncx><navMap><navPoint><content src="OEBPS/Text/titlepage.xhtml#nope"/></navPoint></navMap></ncx>"#,
            ),
            (
                "titlepage.xhtml",
                "<html><body><p id=\"real\">t</p></body></html>",
            ),
        ]);
        assert!(
            ncx_src_wrong_path(
                &ncx_missing_resource("toc.ncx", "OEBPS/Text/titlepage.xhtml#nope"),
                &ws
            )
            .is_empty(),
            "the chosen target does not define that anchor"
        );
    }

    /// The control for the test above: the same shape with an anchor the target
    /// *does* define is repaired, so the decline above is the fragment check
    /// rather than the fixer being unable to see a fragment at all.
    #[test]
    fn a_fragment_the_target_defines_is_carried_across() {
        let ws = container(&[
            (
                "toc.ncx",
                r#"<ncx><navMap><navPoint><content src="OEBPS/Text/titlepage.xhtml#real"/></navPoint></navMap></ncx>"#,
            ),
            (
                "titlepage.xhtml",
                "<html><body><p id=\"real\">t</p></body></html>",
            ),
        ]);
        let fixes = ncx_src_wrong_path(
            &ncx_missing_resource("toc.ncx", "OEBPS/Text/titlepage.xhtml#real"),
            &ws,
        );
        assert_eq!(fixes.len(), 1);
        assert!(
            fixes[0].preview[0].note.contains("→ titlepage.xhtml#real"),
            "the fragment survives the repoint: {}",
            fixes[0].preview[0].note
        );
    }

    fn lang_pairs(pairs: &[(&str, &str)]) -> BTreeSet<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    const LANG_DOC: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml" lang="tr" xml:lang=""><body><p>x</p></body></html>"#;

    #[test]
    fn an_empty_xml_lang_is_filled_from_lang() {
        let edits = plan_lang_agreement_edits(LANG_DOC, &lang_pairs(&[("tr", "")]));
        let out = apply_edits(LANG_DOC, edits);
        assert!(out.contains(r#"xml:lang="tr""#), "{out}");
        assert!(
            out.contains(r#"lang="tr""#),
            "the populated side is untouched"
        );
    }

    #[test]
    fn an_empty_lang_is_filled_from_xml_lang() {
        let doc = r#"<html lang="" xml:lang="de"><body/></html>"#;
        let out = apply_edits(
            doc,
            plan_lang_agreement_edits(doc, &lang_pairs(&[("", "de")])),
        );
        assert!(out.contains(r#"lang="de""#), "{out}");
        assert!(
            out.contains(r#"xml:lang="de""#),
            "the populated side is untouched"
        );
    }

    /// The decline that carries this fixer: two populated values are two claims
    /// about what language the text is in, and choosing is editorial. No shelf
    /// book has the shape, so this test is the only thing holding the branch.
    #[test]
    fn two_populated_values_are_left_to_a_human() {
        let doc = r#"<html lang="en" xml:lang="fr"><body/></html>"#;
        assert!(
            plan_lang_agreement_edits(doc, &lang_pairs(&[("en", "fr")])).is_empty(),
            "neither value may be overwritten by the other"
        );
    }

    /// `set_attr_value` searches for `lang=`, and `xml:lang=` contains it. The
    /// boundary check is what keeps the two apart; without it, filling `lang`
    /// would rewrite the wrong attribute.
    ///
    /// **`xml:lang` is written first here on purpose.** With `lang=""` first, the
    /// naive search happens to land on the right attribute and the test passes
    /// against a broken boundary check — verified by mutating `is_attr_boundary`
    /// to always return true. Only this order discriminates.
    #[test]
    fn filling_lang_does_not_hit_xml_lang() {
        let doc = r#"<html xml:lang="de" lang=""><body/></html>"#;
        let out = apply_edits(
            doc,
            plan_lang_agreement_edits(doc, &lang_pairs(&[("", "de")])),
        );
        assert!(
            out.contains(r#"xml:lang="de""#),
            "xml:lang keeps its own value and its prefix: {out}"
        );
        assert!(
            out.contains(r#" lang="de""#),
            "the bare lang is the one filled: {out}"
        );
        assert_eq!(
            out.matches("de").count(),
            2,
            "no third value appeared: {out}"
        );
    }

    #[test]
    fn a_pair_no_finding_named_is_left_alone() {
        // Only what epubveri reported is repaired — the fixer never sweeps the
        // document for other disagreeing elements.
        assert!(plan_lang_agreement_edits(LANG_DOC, &lang_pairs(&[("en", "")])).is_empty());
    }

    #[test]
    fn a_document_that_will_not_parse_declines_the_lang_fix() {
        assert!(
            plan_lang_agreement_edits("<html lang=\"tr\"", &lang_pairs(&[("tr", "")])).is_empty()
        );
    }

    /// The repair level: a sequence that never reaches 1 gets an origin.
    #[test]
    fn play_order_gives_the_sequence_an_origin() {
        let ncx = r#"<navMap><navPoint playOrder="0"><content src="a.xhtml"/></navPoint></navMap>"#;
        let (out, n) = renumber_play_order(ncx);
        assert_eq!(n, 1);
        assert!(out.contains(r#"playOrder="1""#), "{out}");
    }

    /// The dispatch level, which is where the fault actually was. `no_origin`
    /// can be a book's *only* playOrder fault — epubveri compares the origin as a
    /// string and the gaps numerically — so a book reporting nothing else got no
    /// proposal at all, while the repair below it had always been correct. Two
    /// shelf books were in that state.
    #[test]
    fn no_origin_alone_is_enough_to_propose_a_renumbering() {
        let ws = container(&[(
            "toc.ncx",
            r#"<navMap><navPoint playOrder="0"><content src="a.xhtml"/></navPoint></navMap>"#,
        )]);
        let mut report = Report::default();
        report.messages = vec![epubveri::report::Message {
            id: "RSC-005",
            severity: Severity::Error,
            text: String::new(),
            location: Some("toc.ncx".to_string()),
            position: None,
            rule: Some("ncx.play_order.no_origin"),
            params: vec!["0".to_string()],
            element_path: None,
        }];
        assert_eq!(
            ncx_play_order(&report, &ws).len(),
            1,
            "the missing origin is a fault on its own"
        );
    }

    #[test]
    fn an_ncx_that_will_not_parse_is_declined() {
        let (out, n) = renumber_play_order("<navMap><navPoint playOrder=\"1\">");
        assert_eq!(n, 0, "nothing is rewritten in a document we cannot read");
        assert_eq!(out, "<navMap><navPoint playOrder=\"1\">");
    }

    #[test]
    fn duplicate_id_keeps_first_and_renames_the_rest() {
        let t = r#"<a id="dup"/><b id="dup"/><c id="dup"/>"#;
        let out =
            rename_later_id_occurrences(t, "dup", &["dup-2".to_string(), "dup-3".to_string()]);
        assert_eq!(out, r#"<a id="dup"/><b id="dup-2"/><c id="dup-3"/>"#);
    }

    #[test]
    fn duplicate_id_does_not_match_a_longer_attribute_or_value() {
        // `xid="dup"` (not an id attr) and `id="duplicate"` (different value) untouched.
        let t = r#"<a xid="dup"/><b id="dup"/><c id="duplicate"/><d id="dup"/>"#;
        let out = rename_later_id_occurrences(t, "dup", &["dup-2".to_string()]);
        assert_eq!(
            out,
            r#"<a xid="dup"/><b id="dup"/><c id="duplicate"/><d id="dup-2"/>"#
        );
    }

    #[test]
    fn duplicate_id_new_values_avoid_existing_ids() {
        // If `dup-2` already exists, make_unique must skip to `dup-3`.
        let mut used: HashSet<String> = ["dup".into(), "dup-2".into()].into_iter().collect();
        let new = make_unique("dup".to_string(), &used);
        used.insert(new.clone());
        assert_eq!(new, "dup-3");
    }

    #[test]
    fn doctype_span_stops_at_the_declaration_not_a_body_bracket() {
        // A footnote [1] in the body must not extend the DOCTYPE (the bracket-bug lesson).
        let t = "<!DOCTYPE html PUBLIC \"x\" \"y\">\n<html><body><p>note [1]</p></body></html>";
        let span = doctype_span(t).expect("span");
        assert_eq!(&t[span], "<!DOCTYPE html PUBLIC \"x\" \"y\">");
    }

    #[test]
    fn doctype_span_includes_a_real_internal_subset() {
        let t = "<!DOCTYPE html [ <!ENTITY nbsp \"&#160;\"> ]>\n<html/>";
        let span = doctype_span(t).expect("span");
        assert!(
            t[span].ends_with("]>"),
            "the subset is part of the declaration"
        );
    }

    #[test]
    fn epub3_obsolete_reduces_to_html5_doctype() {
        let t = "<?xml version=\"1.0\"?>\n<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n<html/>";
        let span = doctype_span(t).unwrap();
        let out = format!("{}<!DOCTYPE html>{}", &t[..span.start], &t[span.end..]);
        assert!(out.contains("<!DOCTYPE html>\n<html/>"));
        assert!(!out.contains("PUBLIC"));
    }

    #[test]
    fn malformed_xhtml11_is_recognized_when_it_names_1_1() {
        // Missing a slash — clearly intends 1.1, not the exact recognized string.
        assert!(is_malformed_xhtml11(
            "<!DOCTYPE html PUBLIC \"-//W3C/DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">"
        ));
    }

    #[test]
    fn xhtml_1_0_is_not_treated_as_malformed_1_1() {
        // The corpus case: XHTML 1.0 Strict is a DIFFERENT DTD — must be declined.
        assert!(!is_malformed_xhtml11(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd\">"
        ));
    }

    #[test]
    fn bare_html5_doctype_in_epub2_is_declined() {
        assert!(!is_malformed_xhtml11("<!DOCTYPE html>"));
    }

    #[test]
    fn exact_xhtml11_identifier_is_not_flagged_as_malformed() {
        // If it already carries the recognized string, there's nothing to canonicalize.
        assert!(!is_malformed_xhtml11(
            "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">"
        ));
    }

    #[test]
    fn malformed_xhtml11_with_internal_subset_is_declined() {
        // Canonicalizing would drop the subset's declarations.
        assert!(!is_malformed_xhtml11(
            "<!DOCTYPE html PUBLIC \"-//W3C/DTD XHTML 1.1//EN\" \"x\" [ <!ENTITY foo \"bar\"> ]>"
        ));
    }

    #[test]
    fn missing_semicolon_maps_a_known_entity_to_its_character() {
        let out = replace_unterminated_entity("a&nbsp b", "nbsp", "\u{00A0}");
        assert_eq!(out, "a\u{00A0} b");
    }

    #[test]
    fn missing_semicolon_leaves_a_correct_reference_alone() {
        // The terminated `&nbsp;` must not be touched when repairing `&nbsp`.
        let out = replace_unterminated_entity("x&nbsp;y&nbsp z", "nbsp", "\u{00A0}");
        assert_eq!(out, "x&nbsp;y\u{00A0} z");
    }

    #[test]
    fn missing_semicolon_does_not_touch_a_longer_entity_name() {
        // `&notin;` starts with `not` but is a different, complete entity.
        let out = replace_unterminated_entity("p&notin;q", "not", "\u{00AC}");
        assert_eq!(out, "p&notin;q");
    }

    #[test]
    fn missing_semicolon_repairs_at_end_of_text() {
        let out = replace_unterminated_entity("ends with&nbsp", "nbsp", "\u{00A0}");
        assert_eq!(out, "ends with\u{00A0}");
    }

    #[test]
    fn missing_semicolon_repairs_every_unterminated_occurrence() {
        let out = replace_unterminated_entity("&nbsp and&nbsp and &nbsp!", "nbsp", "\u{00A0}");
        assert_eq!(out, "\u{00A0} and\u{00A0} and \u{00A0}!");
    }

    #[test]
    fn predefined_entity_is_closed_not_substituted() {
        // `&amp` denotes `&`; substituting would re-introduce a bare delimiter,
        // so the repair is to add the missing `;`.
        assert_eq!(
            missing_semicolon_replacement("amp").as_deref(),
            Some("&amp;")
        );
        let out = replace_unterminated_entity("Tom &amp Jerry", "amp", "&amp;");
        assert_eq!(out, "Tom &amp; Jerry");
    }

    #[test]
    fn mapped_entity_replacement_is_the_character() {
        assert_eq!(missing_semicolon_replacement("mdash").as_deref(), Some("—"));
    }

    #[test]
    fn unrecognized_name_is_declined() {
        // Not in the table and not predefined — never guessed.
        assert!(missing_semicolon_replacement("Jerry").is_none());
    }

    /// A spine that lists `ch1` twice — the Kindle→EPUB conversion artifact.
    const DUPE_OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
    <itemref idref="ch2"/>
    <itemref idref="ch1"/>
  </spine>
</package>"#;

    #[test]
    fn duplicate_itemref_keeps_the_first_and_drops_the_repeat() {
        let edits = compute_duplicate_itemref_edits(DUPE_OPF, "ch1").expect("fix");
        assert_eq!(edits.len(), 1);
        let out = apply_edits(DUPE_OPF, edits);
        // The kept entry is the FIRST: ch1 must still precede ch2.
        let ch1 = out.find("idref=\"ch1\"").expect("ch1 survives");
        let ch2 = out.find("idref=\"ch2\"").expect("ch2 untouched");
        assert!(ch1 < ch2, "the first occurrence is the one kept");
        assert_eq!(out.matches("idref=\"ch1\"").count(), 1, "no repeat left");
    }

    #[test]
    fn duplicate_itemref_declines_a_stale_finding() {
        assert!(compute_duplicate_itemref_edits(DUPE_OPF, "ch2").is_none());
    }

    /// An absent `linear` means `yes`, so these two entries really are the same
    /// entry and the repeat may go.
    #[test]
    fn duplicate_itemref_treats_absent_linear_as_yes() {
        let opf = DUPE_OPF.replace(
            "<itemref idref=\"ch1\"/>\n    <itemref idref=\"ch2\"/>",
            "<itemref idref=\"ch1\" linear=\"yes\"/>\n    <itemref idref=\"ch2\"/>",
        );
        assert!(
            compute_duplicate_itemref_edits(&opf, "ch1").is_some(),
            "linear=\"yes\" and an absent linear are the same entry"
        );
    }

    /// One linear, one not, is an authored intent — not a duplicate to delete.
    #[test]
    fn duplicate_itemref_declines_when_linear_disagrees() {
        let opf = DUPE_OPF.replace(
            "<itemref idref=\"ch1\"/>\n  </spine>",
            "<itemref idref=\"ch1\" linear=\"no\"/>\n  </spine>",
        );
        assert!(
            compute_duplicate_itemref_edits(&opf, "ch1").is_none(),
            "in the reading order AND reachable out-of-line is deliberate"
        );
    }

    /// Dropping an itemref a `<meta refines>` points at would orphan metadata —
    /// a finding epubsana would have created itself.
    #[test]
    fn duplicate_itemref_declines_when_the_repeat_is_refined() {
        let opf = DUPE_OPF
            .replace(
                "<itemref idref=\"ch1\"/>\n  </spine>",
                "<itemref idref=\"ch1\" id=\"sp1\"/>\n  </spine>",
            )
            .replace(
                "<manifest>",
                "<metadata><meta refines=\"#sp1\" property=\"x\">v</meta></metadata>\n  <manifest>",
            );
        assert!(compute_duplicate_itemref_edits(&opf, "ch1").is_none());
    }

    /// Isolates the guard above: it is the `<meta refines>` that declines the
    /// fix, not the mere presence of an `id`. An unreferenced id is just a label
    /// on a repeat, and the repeat still goes.
    #[test]
    fn duplicate_itemref_with_an_unreferenced_id_is_still_dropped() {
        let opf = DUPE_OPF.replace(
            "<itemref idref=\"ch1\"/>\n  </spine>",
            "<itemref idref=\"ch1\" id=\"sp1\"/>\n  </spine>",
        );
        assert!(compute_duplicate_itemref_edits(&opf, "ch1").is_some());
    }

    #[test]
    fn linear_is_normalized_not_compared_raw() {
        let doc = parse_xml(DUPE_OPF).expect("parse");
        let irs = spine_itemrefs(&doc);
        assert_eq!(linear_of(&irs[0]), "yes", "absent linear defaults to yes");
    }

    /// A package with one dangling item (`gone`), one live chapter, and a cover
    /// meta naming a live image — enough to exercise the cascade and its guards.
    const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata>
    <meta name="cover" content="cover-img"/>
  </metadata>
  <manifest>
    <item id="cover-img" href="cover.jpg" media-type="image/jpeg"/>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="gone" href="gone.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
    <itemref idref="gone"/>
  </spine>
</package>"#;

    fn dropping(opf: &str, id: &str) -> String {
        let (edits, _, _) = compute_dangling_item_edits(opf, id).expect("fix");
        apply_edits(opf, edits)
    }

    /// Sorted and unique, so the list reads as a set and a duplicate entry (the
    /// likely shape of a bad merge) fails rather than hides.
    #[test]
    fn handled_rules_is_a_sorted_set() {
        let rules = handled_rules();
        let mut sorted = rules.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(rules, sorted.as_slice());
    }

    /// A report carrying one `RSC-001` per named manifest id — the only part of
    /// a real report the dangling fixers read.
    /// A container whose package declares `version`, holding one stylesheet the
    /// finding names — the shape `fix.content_properties` acts on.
    fn properties_ws(version: &str) -> Workspace {
        let opf = format!(
            r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">x</dc:identifier></metadata>
  <manifest><item href="s.css" id="css" media-type="text/css"/></manifest>
  <spine toc="ncx"/>
</package>"#
        );
        container(&[
            (
                "META-INF/container.xml",
                r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
            ),
            ("content.opf", &opf),
            ("s.css", "@font-face{src:url(res:///system/fonts/a.ttf)}"),
        ])
    }

    fn undeclared_property(doc: &str, prop: &str) -> Report {
        let mut report = Report::default();
        report.messages = vec![epubveri::report::Message {
            id: "OPF-014",
            severity: Severity::Error,
            text: String::new(),
            location: Some(doc.to_string()),
            position: None,
            rule: Some("opf.content_document.property_used_undeclared"),
            params: vec![prop.to_string()],
            element_path: None,
        }];
        report
    }

    #[test]
    fn properties_are_declared_on_an_epub3_package() {
        let ws = properties_ws("3.0");
        let fixes = content_properties(&undeclared_property("s.css", "remote-resources"), &ws);
        assert_eq!(fixes.len(), 1, "EPUB 3 is where the attribute belongs");
    }

    /// The shelf's only regression on 2026-08-20: adding an EPUB 3 attribute to
    /// an EPUB 2 package cleared the OPF-014 and produced an RSC-005 in its
    /// place. `properties` does not exist in OPS 2.0.1, so there is no edit that
    /// repairs this book — declining is the whole repair.
    #[test]
    fn an_epub2_package_has_nowhere_to_declare_a_property() {
        let ws = properties_ws("2.0");
        let fixes = content_properties(&undeclared_property("s.css", "remote-resources"), &ws);
        assert!(fixes.is_empty(), "EPUB 2 has no properties attribute");
    }

    #[test]
    fn version_is_read_from_the_package_not_the_xml_declaration() {
        // The XML declaration's own `version="1.0"` sits above the package
        // element and must not be what the guard reads.
        let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0"/>"#;
        assert!(!is_epub2_package(opf));
        assert!(is_epub2_package(
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0"/>"#
        ));
        // A package we cannot read is not treated as EPUB 2 — declining there
        // would remove a repair on a guess.
        assert!(!is_epub2_package("<package/>"));
        assert!(!is_epub2_package("not xml at all"));
    }

    // Two navPoints naming the SAME document, which is the shape that matters:
    // the worst book on the shelf names one document from 28 of them.
    const SPACED_NCX: &str = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <navMap>
    <navPoint id="n1" playOrder="1"><navLabel><text>One</text></navLabel><content src="a b.xhtml"/></navPoint>
    <navPoint id="n2" playOrder="1"><navLabel><text>Two</text></navLabel><content src="a b.xhtml"/></navPoint>
    <navPoint id="n3" playOrder="2"><navLabel><text>Three</text></navLabel><content src="a b.xhtml#s2"/></navPoint>
    <navPoint id="n4" playOrder="3"><navLabel><text>Four</text></navLabel><content src="clean.xhtml"/></navPoint>
  </navMap>
</ncx>"#;

    fn encoded_ncx(srcs: &[&str]) -> String {
        let values: BTreeSet<String> = srcs.iter().map(|s| s.to_string()).collect();
        let edits = plan_src_encoding(SPACED_NCX, &values);
        apply_edits(SPACED_NCX, edits)
    }

    /// The worst book on the shelf names one document from 28 navPoints, so
    /// editing the first match only would leave 27 findings behind.
    #[test]
    fn every_content_element_carrying_the_value_is_encoded() {
        let out = encoded_ncx(&["a b.xhtml"]);
        // Both navPoints naming the document are edited. Editing only the first
        // match would leave 27 findings behind on the shelf's worst book, and the
        // count is what catches that — a `contains` check passes either way.
        assert_eq!(
            out.matches(r#"src="a%20b.xhtml""#).count(),
            2,
            "every element carrying the value, not the first"
        );
        assert!(
            !out.contains(r#"src="a b.xhtml""#),
            "no raw spelling survives"
        );
        // A src that merely shares a prefix is a different value and a different
        // finding, so it is left for its own report.
        assert!(
            out.contains(r#"src="a b.xhtml#s2""#),
            "the fragment-bearing src was not reported, so it is left alone"
        );
    }

    #[test]
    fn a_src_with_a_fragment_is_encoded_when_it_is_the_reported_value() {
        let out = encoded_ncx(&["a b.xhtml#s2"]);
        assert!(
            out.contains(r#"src="a%20b.xhtml#s2""#),
            "the fragment is preserved"
        );
        assert!(
            out.contains(r#"src="a b.xhtml""#),
            "the other src is untouched"
        );
    }

    #[test]
    fn a_src_without_spaces_is_never_touched() {
        assert!(plan_src_encoding(SPACED_NCX, &["clean.xhtml".to_string()].into()).is_empty());
    }

    #[test]
    fn an_unreported_src_is_left_alone() {
        // Only what epubveri flagged is encoded — the fixer never goes looking
        // for other spaces it could fix.
        let out = encoded_ncx(&["nothing here.xhtml"]);
        assert_eq!(out, SPACED_NCX, "no finding named it, so nothing changes");
    }

    #[test]
    fn an_unparseable_ncx_declines() {
        assert!(plan_src_encoding("<ncx><navMap>", &["a b.xhtml".to_string()].into()).is_empty());
    }

    fn report_with_dangling(ids: &[&str]) -> Report {
        let mut report = Report::default();
        report.messages = ids
            .iter()
            .map(|id| epubveri::report::Message {
                id: "RSC-001",
                severity: Severity::Error,
                text: String::new(),
                location: None,
                position: None,
                rule: Some("opf.manifest_item.missing_resource"),
                params: vec![id.to_string(), format!("{id}.xhtml")],
                element_path: None,
            })
            .collect();
        report
    }

    #[test]
    fn dangling_item_drops_the_item_and_cascades_into_the_spine() {
        let (_, spine_drops, cover_meta) = compute_dangling_item_edits(OPF, "gone").expect("fix");
        assert_eq!(spine_drops, 1, "the itemref naming it goes too");
        assert!(!cover_meta, "it is not the declared cover");

        let out = dropping(OPF, "gone");
        assert!(!out.contains("id=\"gone\""), "item dropped");
        assert!(
            !out.contains("idref=\"gone\""),
            "the OPF-049 we would have created is dropped in the same edit"
        );
        assert!(out.contains("id=\"ch1\"") && out.contains("idref=\"ch1\""));
    }

    #[test]
    fn dangling_cover_item_takes_its_cover_meta_with_it() {
        let (_, spine_drops, cover_meta) =
            compute_dangling_item_edits(OPF, "cover-img").expect("fix");
        assert_eq!(spine_drops, 0, "an image is not in the spine");
        assert!(cover_meta, "the meta that named it points at a hole now");

        let out = dropping(OPF, "cover-img");
        assert!(
            !out.contains("name=\"cover\""),
            "the dangling cover meta goes"
        );
        assert!(out.contains("id=\"ch1\""), "nothing else is touched");
    }

    #[test]
    fn dangling_item_declines_when_no_item_carries_the_id() {
        assert!(compute_dangling_item_edits(OPF, "no-such-id").is_none());
    }

    /// A dangling item that is also the navigation document: dropping it would
    /// clear `RSC-001` and hand the book `opf.package.missing_nav_document`
    /// instead. Measured on a real book — the shelf's only self-inflicted
    /// finding — and the reason the fixer declines rather than trades.
    #[test]
    fn a_dangling_nav_item_is_declined() {
        let opf = OPF.replace(
            "<item id=\"gone\" href=\"gone.xhtml\"",
            "<item id=\"gone\" properties=\"nav\" href=\"gone.xhtml\"",
        );
        assert_eq!(nav_item_ids(&opf), BTreeSet::from(["gone".to_string()]));

        let report = report_with_dangling(&["gone"]);
        assert!(
            dangling_item_ids(&report, &opf).is_empty(),
            "a declined deletion must not count against the spine guard either"
        );
    }

    /// `properties` is a token list. Neither a different property nor one that
    /// merely contains the letters makes an item the navigation document.
    #[test]
    fn nav_is_matched_as_a_token_not_a_substring() {
        for props in ["mathml", "navigation", "scripted mathml"] {
            let opf = OPF.replace(
                "<item id=\"gone\" href=\"gone.xhtml\"",
                &format!("<item id=\"gone\" properties=\"{props}\" href=\"gone.xhtml\""),
            );
            assert!(
                nav_item_ids(&opf).is_empty(),
                "properties=\"{props}\" is not a navigation document"
            );
            let report = report_with_dangling(&["gone"]);
            assert_eq!(
                dangling_item_ids(&report, &opf),
                BTreeSet::from(["gone"]),
                "so it is still droppable"
            );
        }
    }

    #[test]
    fn spine_survives_when_a_live_itemref_remains() {
        let dangling = BTreeSet::from(["gone"]);
        assert!(spine_survives_dangling_drops(OPF, &dangling));
    }

    /// The guard is per book, not per fix, and this is why: two dangling items
    /// with one spine entry each each pass an individual check and empty the
    /// spine together.
    #[test]
    fn spine_does_not_survive_when_every_entry_is_dangling() {
        let opf = OPF.replace("<itemref idref=\"ch1\"/>", "<itemref idref=\"gone2\"/>");
        let dangling = BTreeSet::from(["gone", "gone2"]);
        assert!(
            !spine_survives_dangling_drops(&opf, &dangling),
            "a spine-less EPUB is not a repaired book — decline instead"
        );
    }

    /// An itemref naming nothing at all also dies, so it counts against survival
    /// even though a *different* fixer drops it.
    #[test]
    fn spine_survival_counts_pre_existing_dangling_itemrefs_too() {
        let opf = OPF.replace("<itemref idref=\"ch1\"/>", "<itemref idref=\"never\"/>");
        assert!(!spine_survives_dangling_drops(
            &opf,
            &BTreeSet::from(["gone"])
        ));
    }

    #[test]
    fn dangling_itemref_drops_only_its_own_entry() {
        let opf = OPF.replace("<itemref idref=\"gone\"/>", "<itemref idref=\"never\"/>");
        let edits = compute_dangling_itemref_edits(&opf, "never").expect("fix");
        assert_eq!(edits.len(), 1);
        let out = apply_edits(&opf, edits);
        assert!(!out.contains("idref=\"never\""));
        assert!(
            out.contains("idref=\"ch1\""),
            "the reading order keeps its place"
        );
    }

    #[test]
    fn dangling_itemref_declines_when_no_entry_carries_the_idref() {
        assert!(compute_dangling_itemref_edits(OPF, "no-such-id").is_none());
    }

    /// The two fixers never contend for the same `<itemref>` — the cascade only
    /// touches idrefs whose item exists at plan time, the OPF-049 fixer only
    /// idrefs already absent from the manifest. Plan-once is sound because the
    /// sets are disjoint, not because we got lucky.
    #[test]
    fn the_two_dangling_fixers_touch_disjoint_itemrefs() {
        let opf = OPF.replace(
            "<itemref idref=\"ch1\"/>",
            "<itemref idref=\"ch1\"/>\n    <itemref idref=\"never\"/>",
        );
        let (cascade, _, _) = compute_dangling_item_edits(&opf, "gone").expect("fix");
        let standalone = compute_dangling_itemref_edits(&opf, "never").expect("fix");
        for a in &cascade {
            for b in &standalone {
                assert!(
                    a.range.end <= b.range.start || b.range.end <= a.range.start,
                    "the two fixers' edits must never overlap"
                );
            }
        }
    }

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

    /// A `schema_violation` message, the way the grammar emits it: the message
    /// names the containing element and `params[0]` repeats it.
    fn schema_violation(text: &str, param: &str) -> epubveri::report::Message {
        epubveri::report::Message {
            id: "RSC-005",
            severity: Severity::Error,
            text: text.to_string(),
            location: Some("OEBPS/ch1.xhtml".to_string()),
            position: None,
            rule: Some("opf.content_document.schema_violation"),
            params: vec![param.to_string()],
            element_path: None,
        }
    }

    /// An `element "X" is not allowed here` message, with the expected set the
    /// grammar lists after it.
    fn element_violation(element: &str, expected: &[&str]) -> epubveri::report::Message {
        let mut params = vec![element.to_string()];
        params.extend(expected.iter().map(|s| s.to_string()));
        epubveri::report::Message {
            id: "RSC-005",
            severity: Severity::Error,
            text: format!("element \"{element}\" is not allowed here; expected one of …"),
            location: Some("OEBPS/ch1.xhtml".to_string()),
            position: None,
            rule: Some("opf.content_document.schema_violation"),
            params,
            element_path: None,
        }
    }

    #[test]
    fn an_inline_element_the_grammar_wants_wrapped_is_matched() {
        assert!(is_misplaced_inline_element(&element_violation(
            "a",
            &["blockquote", "div", "p"]
        )));
        assert!(is_misplaced_inline_element(&element_violation(
            "br",
            &["div", "p"]
        )));
    }

    /// The decline that carries the most weight: XHTML 1.1 has no `figure`, so a
    /// `<div>` around one would move the violation rather than clear it.
    #[test]
    fn an_element_xhtml11_does_not_have_is_not_matched() {
        for el in ["figure", "figcaption", "section", "center", "li"] {
            assert!(
                !is_misplaced_inline_element(&element_violation(el, &["div", "p"])),
                "{el} must not be wrapped"
            );
        }
    }

    #[test]
    fn an_inline_element_is_not_matched_when_div_is_not_expected() {
        // Then the grammar objects to something other than block-level
        // placement, and a wrapper is not the repair.
        assert!(!is_misplaced_inline_element(&element_violation(
            "a",
            &["em", "strong"]
        )));
    }

    #[test]
    fn stray_text_in_a_blockquote_is_wrapped() {
        let doc = "<html><body><blockquote>quoted words</blockquote></body></html>";
        let out = wrap_body_text(doc).unwrap();
        assert!(
            out.contains("<blockquote><div>quoted words</div></blockquote>"),
            "{out}"
        );
    }

    /// The two containers must not fight: a `<blockquote>` is a block element,
    /// so it ends a run in `<body>`, and its own children are walked separately.
    #[test]
    fn a_blockquote_ends_the_body_run_and_is_walked_itself() {
        let doc = "<html><body>loose <blockquote>quoted</blockquote></body></html>";
        let out = wrap_body_text(doc).unwrap();
        assert!(out.contains("<div>loose</div>"), "the body run: {out}");
        assert!(
            out.contains("<blockquote><div>quoted</div></blockquote>"),
            "and the blockquote's own run: {out}"
        );
        assert_eq!(out.matches("<div>").count(), 2, "two runs, not one: {out}");
    }

    #[test]
    fn a_nested_blockquote_is_walked_too() {
        let doc =
            "<html><body><blockquote><blockquote>inner</blockquote></blockquote></body></html>";
        let out = wrap_body_text(doc).unwrap();
        assert_eq!(out.matches("<div>").count(), 1);
        assert!(
            out.contains("<blockquote><div>inner</div></blockquote>"),
            "{out}"
        );
    }

    /// `<ol>` wants an `<li>` and `<head>` wants a `<title>` — wrappers that
    /// assert what the content is. Only containers admitting a neutral `<div>`
    /// are in scope.
    #[test]
    fn a_container_that_needs_an_asserting_wrapper_is_not_touched() {
        let doc = "<html><body><ol>stray</ol></body></html>";
        let out = wrap_body_text(doc).unwrap();
        assert!(out.contains("<ol>stray</ol>"), "left alone: {out}");
        assert!(!WRAPPABLE_CONTAINERS.contains(&"ol"));
        assert!(!WRAPPABLE_CONTAINERS.contains(&"head"));
    }

    #[test]
    fn the_incomplete_content_message_triggers_only_for_wrappable_containers() {
        // The same defect, reported from the other end.
        for c in ["body", "blockquote"] {
            let mut m = element_violation(c, &[]);
            m.text = format!("element \"{c}\" has incomplete content");
            assert!(is_incomplete_container(&m), "{c} must trigger");
        }
        for c in ["ol", "ul", "head"] {
            let mut m = element_violation(c, &[]);
            m.text = format!("element \"{c}\" has incomplete content");
            assert!(!is_incomplete_container(&m), "{c} must not");
        }
    }

    #[test]
    fn text_and_an_inline_element_become_one_div() {
        let doc = "<html><body>\n  text <a href=\"x\">link</a>\n</body></html>";
        let out = wrap_body_text(doc).unwrap();
        assert!(
            out.contains("<div>text <a href=\"x\">link</a></div>"),
            "one run, one block: {out}"
        );
        assert_eq!(out.matches("<div>").count(), 1);
    }

    #[test]
    fn a_block_element_ends_the_run() {
        let doc =
            "<html><body><a href=\"x\">one</a><p>block</p><a href=\"y\">two</a></body></html>";
        let out = wrap_body_text(doc).unwrap();
        assert_eq!(out.matches("<div>").count(), 2, "two runs: {out}");
        assert!(out.contains("<p>block</p>"), "the block is untouched");
    }

    #[test]
    fn an_unknown_element_ends_the_run_and_is_left_alone() {
        let doc =
            "<html><body><a href=\"x\">one</a><figure><img src=\"i\"/></figure></body></html>";
        let out = wrap_body_text(doc).unwrap();
        assert_eq!(out.matches("<div>").count(), 1);
        assert!(
            out.contains("<figure><img src=\"i\"/></figure>"),
            "the figure is not wrapped: {out}"
        );
    }

    #[test]
    fn whitespace_inside_a_run_is_kept_and_around_it_is_left_outside() {
        let doc = "<html><body>\n\n  <a href=\"x\">one</a>  <b>two</b>\n\n</body></html>";
        let out = wrap_body_text(doc).unwrap();
        assert!(
            out.contains("\n\n  <div><a href=\"x\">one</a>  <b>two</b></div>\n\n"),
            "the run's own spacing survives, the surrounding indentation stays put: {out:?}"
        );
    }

    /// The finding this fixer used to have to itself now arrives inside
    /// `schema_violation`, so the match has to be narrow in two directions at
    /// once: the right kind of violation, and a container we can repair.
    #[test]
    fn only_stray_text_in_body_is_matched() {
        assert!(is_stray_text_in_body(&schema_violation(
            "stray text is not allowed directly in \"body\"; wrap it in an element",
            "body",
        )));

        // Right kind, wrong container: an <li> is the correct wrapper inside an
        // <ol>, and that asserts the text is a list item — a judgement.
        assert!(!is_stray_text_in_body(&schema_violation(
            "stray text is not allowed directly in \"ol\"; wrap it in an element",
            "ol",
        )));

        // Right container, wrong kind — the param alone would have matched.
        assert!(!is_stray_text_in_body(&schema_violation(
            "element \"body\" is not allowed here",
            "body",
        )));

        // A sibling rule that is not a schema violation at all.
        let mut other = schema_violation(
            "stray text is not allowed directly in \"body\"; wrap it in an element",
            "body",
        );
        other.rule = Some("opf.content_document.empty_title");
        assert!(!is_stray_text_in_body(&other));
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

    // --- fix.anchor_name -------------------------------------------------

    fn drop_anchor_names(doc: &str) -> Option<String> {
        let spans = plan_anchor_name_drops(doc)?;
        let edits = spans
            .into_iter()
            .map(|range| MetaEdit {
                range,
                replacement: String::new(),
            })
            .collect();
        Some(apply_edits(doc, edits))
    }

    /// The shelf's shape, all 162 of them: the `id` already carries the value,
    /// so the `name` is a duplicate declaration and the fragment still resolves.
    #[test]
    fn a_redundant_anchor_name_is_dropped_with_its_whitespace() {
        let doc = r#"<html><body><a href="x.xhtml#f1" id="f1" name="f1">1</a></body></html>"#;
        assert_eq!(
            drop_anchor_names(doc).unwrap(),
            r#"<html><body><a href="x.xhtml#f1" id="f1">1</a></body></html>"#
        );
    }

    /// No `id` to fall back on: renaming `name` → `id` would have to prove the
    /// value is an NCName and unique, so the fixer leaves it for a human.
    #[test]
    fn an_anchor_name_without_an_id_is_declined() {
        let doc = r#"<html><body><a name="f1">1</a></body></html>"#;
        assert!(plan_anchor_name_drops(doc).unwrap().is_empty());
    }

    /// Two different values: dropping `name` breaks any `#fragment` targeting
    /// it, and an element cannot carry two ids.
    #[test]
    fn an_anchor_whose_id_and_name_differ_is_declined() {
        let doc = r#"<html><body><a id="a1" name="f1">1</a></body></html>"#;
        assert!(plan_anchor_name_drops(doc).unwrap().is_empty());
    }

    /// `name` is obsolete on `<a>`; on a form control it is the control's
    /// name and carries the submitted data. The fixer is `<a>`-only.
    #[test]
    fn a_name_on_another_element_is_never_touched() {
        let doc = r#"<html><body><input id="q" name="q"/></body></html>"#;
        assert!(plan_anchor_name_drops(doc).unwrap().is_empty());
    }

    #[test]
    fn several_anchors_in_one_document_are_all_dropped() {
        let doc = r#"<html><body><a id="a" name="a">1</a><a id="b" name="b">2</a></body></html>"#;
        assert_eq!(
            drop_anchor_names(doc).unwrap(),
            r#"<html><body><a id="a">1</a><a id="b">2</a></body></html>"#
        );
    }

    /// Attributes spread over lines: the deletion takes the run of whitespace
    /// in front of the attribute, so the tag reads as if it were never written.
    #[test]
    fn a_multiline_tag_keeps_its_shape() {
        let doc = "<html><body><a\n  id=\"f\"\n  name=\"f\">1</a></body></html>";
        assert_eq!(
            drop_anchor_names(doc).unwrap(),
            "<html><body><a\n  id=\"f\">1</a></body></html>"
        );
    }

    // --- fix.empty_lang --------------------------------------------------

    /// Apply the fixer with no book language available — the delete branch,
    /// which is what every pre-existing test below exercises.
    fn drop_empty_langs(doc: &str) -> Option<String> {
        Some(apply_edits(doc, plan_empty_lang_edits(doc, None)?))
    }

    /// Apply it with a book language, i.e. the filling branch on the root.
    fn fill_empty_langs(doc: &str, lang: &str) -> Option<String> {
        Some(apply_edits(doc, plan_empty_lang_edits(doc, Some(lang))?))
    }

    /// The shelf's whole population: an empty pair on the root element, with a
    /// book that declares one language. Both are filled, both keep their
    /// spelling, and the document ends up stating the language the book states.
    #[test]
    fn a_root_lang_is_filled_from_the_book_not_deleted() {
        let doc = r#"<html lang="" xml:lang=""><body>x</body></html>"#;
        assert_eq!(
            fill_empty_langs(doc, "tr").unwrap(),
            r#"<html lang="tr" xml:lang="tr"><body>x</body></html>"#
        );
    }

    /// Single quotes survive: only the value between them changes.
    #[test]
    fn filling_keeps_the_original_quote_character() {
        let doc = "<html lang=''><body>x</body></html>";
        assert_eq!(
            fill_empty_langs(doc, "en").unwrap(),
            "<html lang='en'><body>x</body></html>"
        );
    }

    /// Off the root there is an ancestor to inherit from, and an empty tag may
    /// have meant "not the book's language" — so it is deleted, never filled.
    #[test]
    fn a_non_root_empty_lang_is_still_deleted_even_with_a_book_language() {
        let doc = r#"<html lang="tr"><body><span lang="">x</span></body></html>"#;
        assert_eq!(
            fill_empty_langs(doc, "tr").unwrap(),
            r#"<html lang="tr"><body><span>x</span></body></html>"#
        );
    }

    /// With no usable book language the root falls back to the old behaviour.
    #[test]
    fn without_a_book_language_the_root_is_deleted_as_before() {
        let doc = r#"<html lang=""><body>x</body></html>"#;
        assert_eq!(
            drop_empty_langs(doc).unwrap(),
            r#"<html><body>x</body></html>"#
        );
    }

    #[test]
    fn a_language_tag_is_recognised_but_a_language_name_is_not() {
        assert!(is_language_tag("tr"));
        assert!(is_language_tag("en-US"));
        assert!(is_language_tag("zh-Hant-TW"));
        // The two shapes the corpus actually produces, both rejected.
        assert!(
            !is_language_tag("en_US"),
            "an underscore is not a separator"
        );
        assert!(!is_language_tag("turkish"), "a language name is not a tag");
        assert!(!is_language_tag(""), "and neither is nothing");
        assert!(
            !is_language_tag("e"),
            "a one-letter primary subtag is not one"
        );
    }

    /// The shelf's shape: both spellings on the same element, both empty.
    #[test]
    fn both_spellings_of_an_empty_lang_are_deleted() {
        let doc = r#"<html><body><p lang="" xml:lang="">x</p></body></html>"#;
        assert_eq!(
            drop_empty_langs(doc).unwrap(),
            "<html><body><p>x</p></body></html>"
        );
    }

    /// A malformed tag is a different defect: repairing it means guessing which
    /// language was meant, which is exactly what epubsana refuses to do.
    #[test]
    fn a_non_empty_language_tag_is_never_touched() {
        for doc in [
            r#"<html><body><p lang="en_US">x</p></body></html>"#,
            r#"<html><body><p lang="tr" xml:lang="tr">x</p></body></html>"#,
        ] {
            assert!(
                plan_empty_lang_edits(doc, None).unwrap().is_empty(),
                "{doc}"
            );
        }
    }

    /// Only the empty one goes, even when its sibling on the same element is a
    /// perfectly good tag.
    #[test]
    fn a_mixed_pair_loses_only_the_empty_half() {
        let doc = r#"<html><body><p lang="" xml:lang="tr">x</p></body></html>"#;
        assert_eq!(
            drop_empty_langs(doc).unwrap(),
            r#"<html><body><p xml:lang="tr">x</p></body></html>"#
        );
    }

    /// The finding decides, not the fixer's own reading of the document: a
    /// malformed value and a non-`lang` attribute must both fail the match.
    #[test]
    fn only_an_empty_lang_finding_is_matched() {
        let empty = |attr: &str, value: &str| epubveri::report::Message {
            id: "RSC-005",
            severity: Severity::Error,
            text: format!("value of attribute \"{attr}\" is invalid: \"{value}\""),
            location: Some("OEBPS/ch1.xhtml".to_string()),
            position: None,
            rule: Some("opf.content_document.schema_violation"),
            params: vec![attr.to_string(), value.to_string()],
            element_path: None,
        };
        assert!(is_empty_lang(&empty("lang", "")));
        assert!(is_empty_lang(&empty("xml:lang", "")));
        assert!(!is_empty_lang(&empty("lang", "en_US")));
        // The cross-file one: renaming an id means moving every href="#…" that
        // targets it. Determinate in principle, not a local edit, not this fixer.
        assert!(!is_empty_lang(&empty("id", "06")));
    }

    #[test]
    fn whitespace_only_span_is_none_but_real_text_is_trimmed() {
        assert_eq!(trimmed_span(0..3, "   "), None);
        assert_eq!(trimmed_span(10..17, "\n abc \n"), Some(12..15));
    }
}
