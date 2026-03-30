use super::*;

#[test]
fn strips_single_private_tag() {
    let input = "API key is <private>sk-abc123</private> here";
    assert_eq!(strip_private_tags(input), "API key is [REDACTED] here");
}

#[test]
fn strips_multiple_private_tags() {
    let input = "Key: <private>secret1</private> and <private>secret2</private>";
    assert_eq!(
        strip_private_tags(input),
        "Key: [REDACTED] and [REDACTED]"
    );
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
