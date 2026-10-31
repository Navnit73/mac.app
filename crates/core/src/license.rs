//! Licensing. MyMacStorage is free with full access and has no licence keys or trial limits
//! (product owner, 2026-10-31; see docs/decisions.md). Kept only for the heartbeat field.

/// Whether a licence key is active. There are no licences, so this is always false.
pub fn is_active() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {}
}
