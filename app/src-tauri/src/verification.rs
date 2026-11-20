//! Opt-in page bound for authorized live checks in an isolated debug profile.
pub(crate) fn source_page_limit() -> Option<usize> {
    #[cfg(debug_assertions)]
    if std::env::var_os("NNA_DATA_DIR").is_some()
        && std::env::var("NNA_VERIFY_SOURCE_PAGES").as_deref() == Ok("5")
    {
        return Some(5);
    }
    None
}

#[cfg(test)]
#[path = "verification_tests.rs"]
mod tests;
