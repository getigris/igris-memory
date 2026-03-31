use super::*;

#[test]
fn strips_single_private_tag() {
    let input = "API key is <private>sk-abc123</private> here";
    assert_eq!(strip_private_tags(input), "API key is [REDACTED] here");
}

#[test]
fn strips_multiple_private_tags() {
    let input = "Key: <private>secret1</private> and <private>secret2</private>";
    assert_eq!(strip_private_tags(input), "Key: [REDACTED] and [REDACTED]");
}

#[test]
fn no_tags_unchanged() {
    let input = "nothing sensitive here";
    assert_eq!(strip_private_tags(input), input);
}

#[test]
fn hash_is_deterministic() {
    let a = hash_content("hello world");
    let b = hash_content("hello  world");
    assert_eq!(a, b, "whitespace normalization should produce same hash");
}

#[test]
fn hash_differs_for_different_content() {
    let a = hash_content("hello world");
    let b = hash_content("goodbye world");
    assert_ne!(a, b);
}

#[test]
fn strips_unclosed_private_tag_unchanged() {
    // Unclosed tag should NOT match (regex requires closing tag)
    let input = "Key: <private>secret without closing";
    assert_eq!(strip_private_tags(input), input);
}

#[test]
fn strips_private_tag_multiline() {
    // Default regex . doesn't match newlines, so multiline private tags survive
    let input = "before <private>line1\nline2</private> after";
    // This depends on regex behavior — our regex uses .*? which is single-line by default
    assert_eq!(
        strip_private_tags(input),
        input,
        "multiline private tags are not stripped (by design)"
    );
}

#[test]
fn strips_adjacent_private_tags() {
    let input = "<private>a</private><private>b</private>";
    assert_eq!(strip_private_tags(input), "[REDACTED][REDACTED]");
}

#[test]
fn strips_private_tag_with_special_chars() {
    let input = "Token: <private>sk-abc/123+xyz==</private>";
    assert_eq!(strip_private_tags(input), "Token: [REDACTED]");
}

#[test]
fn hash_empty_string() {
    let a = hash_content("");
    let b = hash_content("   ");
    assert_eq!(a, b, "empty and whitespace-only should hash the same");
}

#[test]
fn hash_unicode_content() {
    let a = hash_content("认证设计 🔐");
    let b = hash_content("认证设计  🔐");
    assert_eq!(
        a, b,
        "unicode with different whitespace should produce same hash"
    );
}
