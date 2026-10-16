//! HTML named character references → their Unicode character.
//!
//! The `RSC-016` "entity 'X' was referenced but not declared" finding is the
//! single most common real-world defect (spike: 48/171 books, 11k+
//! occurrences): XHTML using HTML named entities (`&nbsp;`, `&mdash;`, …)
//! without a DTD that declares them. The safe, content-preserving fix is to
//! replace each such reference with the actual character it denotes.
//!
//! The names are the three XHTML 1.x DTD entity sets (`HTMLlat1`,
//! `HTMLsymbol`, `HTMLspecial`), read from epubveri's
//! [`epubveri::htm::xhtml_entity`] — the table it declares from — so the
//! detector and the repair never disagree about what a name means. A name
//! outside those sets (HTML5's `&NewLine;`) is left untouched, never guessed.
//! The XML-predefined five (`amp`/`lt`/`gt`/`quot`/`apos`) are excluded: their
//! character is a bare delimiter, and they are always declared, so never
//! flagged.

/// The XML-predefined entities, which [`lookup`] never maps.
const PREDEFINED: [&str; 5] = ["amp", "lt", "gt", "quot", "apos"];

/// Look up a named entity (without the `&`/`;`), returning the character it
/// denotes, or `None` if it is not an XHTML 1.x DTD entity or is one of the
/// XML-predefined five (leave it alone). Case matters: `Eacute` and `eacute`
/// are two entities.
pub fn lookup(name: &str) -> Option<char> {
    if PREDEFINED.contains(&name) {
        return None;
    }
    epubveri::htm::xhtml_entity(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_entities_resolve() {
        assert_eq!(lookup("nbsp"), Some('\u{00A0}'));
        assert_eq!(lookup("mdash"), Some('—'));
        assert_eq!(lookup("eacute"), Some('é'));
        assert_eq!(lookup("Eacute"), Some('É'));
    }

    #[test]
    fn the_whole_xhtml_table_resolves() {
        // Outside the 117 names this module once carried: one from each set.
        assert_eq!(lookup("emsp"), Some('\u{2003}')); // HTMLspecial
        assert_eq!(lookup("Agrave"), Some('À')); // HTMLlat1
        assert_eq!(lookup("alefsym"), Some('ℵ')); // HTMLsymbol
    }

    #[test]
    fn unknown_entity_is_left_alone() {
        assert_eq!(lookup("definitely-not-an-entity"), None);
        // HTML5-only names are not XHTML's.
        assert_eq!(lookup("NewLine"), None);
        assert_eq!(lookup("NBSP"), None);
    }

    #[test]
    fn predefined_entities_are_never_mapped() {
        for name in PREDEFINED {
            assert_eq!(lookup(name), None, "{name}");
        }
    }
}
