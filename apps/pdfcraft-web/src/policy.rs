//! The embedding boundary, shared by browser code and native regression tests.
pub const MAX_BYTES: u32 = 128 * 1024 * 1024;
pub fn valid_size(len: u32) -> bool {
    len > 0 && len <= MAX_BYTES
}
pub fn parent_origin(own: &str, configured: Option<&str>) -> Result<String, String> {
    let origin = configured.unwrap_or(own);
    let url = url::Url::parse(origin).map_err(|_| "Invalid parentOrigin".to_owned())?;
    if !matches!(url.scheme(), "https" | "http") || url.origin().ascii_serialization() != origin {
        return Err("parentOrigin must be an exact HTTP(S) origin without a path".into());
    }
    Ok(origin.to_owned())
}
pub fn accepts_message(expected: &str, actual: &str, is_parent: bool) -> bool {
    is_parent && expected == actual
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_origin_and_parent_are_both_required() {
        assert_eq!(parent_origin("https://app.example", None).unwrap(), "https://app.example");
        assert_eq!(parent_origin("https://app.example", Some("https://host.example:8443")).unwrap(), "https://host.example:8443");
        for invalid in [
            "*",
            "null",
            "file:///tmp",
            "https://host.example/path",
            "https://host.example/",
            "https://user:secret@host.example",
            "https://host.example?x=1",
        ] {
            assert!(parent_origin("https://app.example", Some(invalid)).is_err(), "{invalid}");
        }
        assert!(parent_origin("null", None).is_err());
        assert!(accepts_message("https://host.example", "https://host.example", true));
        assert!(!accepts_message("https://host.example", "https://host.example", false));
        assert!(!accepts_message("https://host.example", "https://evil.example", true));
    }
    #[test]
    fn input_allocation_is_bounded() {
        assert!(!valid_size(0));
        assert!(valid_size(1));
        assert!(valid_size(MAX_BYTES));
        assert!(!valid_size(MAX_BYTES + 1));
    }
}
