//! Closed, host-owned registry of **platform capabilities** (ADR-0022 §6).
//!
//! ADR-0022 splits capabilities into two orthogonal kinds:
//!
//! - **Platform capabilities** (`network`, `storage`, `ui`, `notify`, `llm`,
//!   `system`) — the *permission* axis governed by ADR-0011. These are always
//!   provided by the host and gated by allow/deny permission checks. They form a
//!   **closed set** owned here.
//! - **Provider capabilities** (`commerce`, `scene`, `physics`, `scanning`, …) —
//!   the *interface* axis governed by ADR-0022. These are versioned contracts
//!   satisfied by provider plugins (or host built-ins) and resolved against
//!   `[capabilities.provided]` declarations. They form an **open set**.
//!
//! The plugin loader uses [`is_platform_capability`] to **route** a
//! `[capabilities.required]` entry: platform-capability names go to the ADR-0011
//! permission gate; everything else is treated as a provider capability and goes
//! to capability resolution (Step 2). This routing function lives in the
//! manifest/schema layer because it is pure, host-owned policy data; the loader
//! that consumes it is built in Step 2.

/// The closed set of platform-capability names the host always provides
/// (ADR-0011 §, ADR-0022 §1/§6).
///
/// This is intentionally a fixed `&[&str]` (not user-extensible): the platform
/// capability taxonomy is owned by the host. Adding a new platform capability is
/// a deliberate host change, not something a plugin can declare into existence.
/// Anything not in this set is, by definition, a **provider** capability.
pub const PLATFORM_CAPABILITIES: &[&str] = &["network", "storage", "ui", "notify", "llm", "system"];

/// Returns `true` if `name` is a host-owned platform capability (permission
/// axis, ADR-0011) rather than a provider capability (interface axis, ADR-0022).
///
/// Matching is exact and case-sensitive: capability names are lowercase
/// identifiers by convention (see inner-space `plugin.toml`).
pub fn is_platform_capability(name: &str) -> bool {
    PLATFORM_CAPABILITIES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_capabilities_are_recognized() {
        // Every name in the closed set must be recognized.
        for name in PLATFORM_CAPABILITIES {
            assert!(
                is_platform_capability(name),
                "{name} should be a platform capability"
            );
        }
        // Spot-check the documented platform names explicitly.
        assert!(is_platform_capability("network"));
        assert!(is_platform_capability("storage"));
        assert!(is_platform_capability("llm"));
        assert!(is_platform_capability("ui"));
        assert!(is_platform_capability("notify"));
        assert!(is_platform_capability("system"));
    }

    #[test]
    fn provider_capabilities_are_not_platform() {
        // Provider/interface capabilities (ADR-0022) must NOT match the closed
        // platform set — these route to capability resolution, not the
        // permission gate.
        assert!(!is_platform_capability("commerce"));
        assert!(!is_platform_capability("scene"));
        assert!(!is_platform_capability("physics"));
        assert!(!is_platform_capability("scanning"));
        assert!(!is_platform_capability("audio"));
        assert!(!is_platform_capability("ar"));
    }

    #[test]
    fn matching_is_exact_and_case_sensitive() {
        assert!(!is_platform_capability("Network"));
        assert!(!is_platform_capability("NETWORK"));
        assert!(!is_platform_capability("net"));
        assert!(!is_platform_capability(""));
    }
}
