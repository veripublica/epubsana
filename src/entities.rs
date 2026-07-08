//! HTML named character references → their Unicode character.
//!
//! The `RSC-016` "entity 'X' was referenced but not declared" finding is the
//! single most common real-world defect (spike: 48/171 books, 11k+
//! occurrences): XHTML using HTML named entities (`&nbsp;`, `&mdash;`, …)
//! without a DTD that declares them. The safe, content-preserving fix is to
//! replace each such reference with the actual character it denotes.
//!
//! This table is a curated subset covering Latin-1 + General Punctuation + the
//! common symbols that occur in real books. It is deliberately conservative:
//! an entity **not** in this table is left untouched (reported as residual),
//! never guessed. The XML-predefined five (`amp`/`lt`/`gt`/`quot`/`apos`) are
//! intentionally absent — they are always declared, so never flagged.

/// Look up a named entity (without the `&`/`;`), returning its replacement
/// character, or `None` if we don't map it (leave it alone).
pub fn lookup(name: &str) -> Option<&'static str> {
    TABLE
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, replacement)| *replacement)
}

/// (entity name, replacement character). Kept as a flat slice — small, and the
/// per-file fixer looks up only the handful of distinct names it actually saw.
pub const TABLE: &[(&str, &str)] = &[
    // Latin-1 punctuation & symbols
    ("nbsp", "\u{00A0}"),
    ("iexcl", "¡"),
    ("cent", "¢"),
    ("pound", "£"),
    ("curren", "¤"),
    ("yen", "¥"),
    ("brvbar", "¦"),
    ("sect", "§"),
    ("uml", "¨"),
    ("copy", "©"),
    ("ordf", "ª"),
    ("laquo", "«"),
    ("not", "¬"),
    ("shy", "\u{00AD}"),
    ("reg", "®"),
    ("macr", "¯"),
    ("deg", "°"),
    ("plusmn", "±"),
    ("sup2", "²"),
    ("sup3", "³"),
    ("acute", "´"),
    ("micro", "µ"),
    ("para", "¶"),
    ("middot", "·"),
    ("cedil", "¸"),
    ("sup1", "¹"),
    ("ordm", "º"),
    ("raquo", "»"),
    ("frac14", "¼"),
    ("frac12", "½"),
    ("frac34", "¾"),
    ("iquest", "¿"),
    ("times", "×"),
    ("divide", "÷"),
    // Latin-1 accented letters (uppercase)
    ("Agrave", "À"),
    ("Aacute", "Á"),
    ("Acirc", "Â"),
    ("Atilde", "Ã"),
    ("Auml", "Ä"),
    ("Aring", "Å"),
    ("AElig", "Æ"),
    ("Ccedil", "Ç"),
    ("Egrave", "È"),
    ("Eacute", "É"),
    ("Ecirc", "Ê"),
    ("Euml", "Ë"),
    ("Igrave", "Ì"),
    ("Iacute", "Í"),
    ("Icirc", "Î"),
    ("Iuml", "Ï"),
    ("ETH", "Ð"),
    ("Ntilde", "Ñ"),
    ("Ograve", "Ò"),
    ("Oacute", "Ó"),
    ("Ocirc", "Ô"),
    ("Otilde", "Õ"),
    ("Ouml", "Ö"),
    ("Oslash", "Ø"),
    ("Ugrave", "Ù"),
    ("Uacute", "Ú"),
    ("Ucirc", "Û"),
    ("Uuml", "Ü"),
    ("Yacute", "Ý"),
    ("THORN", "Þ"),
    ("szlig", "ß"),
    // Latin-1 accented letters (lowercase)
    ("agrave", "à"),
    ("aacute", "á"),
    ("acirc", "â"),
    ("atilde", "ã"),
    ("auml", "ä"),
    ("aring", "å"),
    ("aelig", "æ"),
    ("ccedil", "ç"),
    ("egrave", "è"),
    ("eacute", "é"),
    ("ecirc", "ê"),
    ("euml", "ë"),
    ("igrave", "ì"),
    ("iacute", "í"),
    ("icirc", "î"),
    ("iuml", "ï"),
    ("eth", "ð"),
    ("ntilde", "ñ"),
    ("ograve", "ò"),
    ("oacute", "ó"),
    ("ocirc", "ô"),
    ("otilde", "õ"),
    ("ouml", "ö"),
    ("oslash", "ø"),
    ("ugrave", "ù"),
    ("uacute", "ú"),
    ("ucirc", "û"),
    ("uuml", "ü"),
    ("yacute", "ý"),
    ("thorn", "þ"),
    ("yuml", "ÿ"),
    // General punctuation
    ("ndash", "–"),
    ("mdash", "—"),
    ("lsquo", "\u{2018}"),
    ("rsquo", "\u{2019}"),
    ("sbquo", "\u{201A}"),
    ("ldquo", "\u{201C}"),
    ("rdquo", "\u{201D}"),
    ("bdquo", "\u{201E}"),
    ("dagger", "†"),
    ("Dagger", "‡"),
    ("bull", "•"),
    ("hellip", "…"),
    ("permil", "‰"),
    ("prime", "′"),
    ("Prime", "″"),
    ("lsaquo", "‹"),
    ("rsaquo", "›"),
    ("oline", "‾"),
    ("frasl", "⁄"),
    ("euro", "€"),
    ("trade", "™"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_entities_resolve() {
        assert_eq!(lookup("nbsp"), Some("\u{00A0}"));
        assert_eq!(lookup("mdash"), Some("—"));
        assert_eq!(lookup("eacute"), Some("é"));
    }

    #[test]
    fn unknown_entity_is_left_alone() {
        assert_eq!(lookup("definitely-not-an-entity"), None);
        // XML-predefined ones are intentionally not in the table.
        assert_eq!(lookup("amp"), None);
    }
}
