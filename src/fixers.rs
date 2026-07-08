//! The fix registry: turn epubveri findings into [`ProposedFix`]es.
//!
//! Each fixer keys off an epubveri message `rule` (or an unambiguous `id`) and
//! builds a proposal, or declines (returns nothing) when it can't fix a finding
//! safely. v1 ships one fixer — HTML-entity mapping (`RSC-016`), the highest-ROI
//! defect from the feasibility spike — and the registry grows from there.

use std::collections::{BTreeMap, HashSet};

use epubveri::report::Report;

use crate::{entities, Change, Goal, ProposedFix, Tier, Workspace};

/// Build the ordered list of proposals for a detection [`Report`].
pub fn plan(report: &Report, ws: &Workspace, _goal: Goal) -> Vec<ProposedFix> {
    let mut fixes = Vec::new();
    fixes.extend(html_entities(report, ws));
    fixes.extend(ncx_ncnames(report, ws));
    // Future fixers append here, in a sensible confirm order.
    fixes
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
            addresses_rule: Some("htm.entity.undeclared".to_string()),
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
            addresses_rule: Some("ncx.ids.invalid_ncname".to_string()),
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
}
