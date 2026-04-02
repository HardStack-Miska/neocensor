pub mod paths;

pub use paths::*;

/// Private IP ranges for proxy bypass (used in PAC files and ProxyOverride).
pub const PRIVATE_IP_RANGES: &[&str] = &[
    "localhost",
    "127.*",
    "10.*",
    "172.16.*",
    "172.17.*",
    "172.18.*",
    "172.19.*",
    "172.20.*",
    "172.21.*",
    "172.22.*",
    "172.23.*",
    "172.24.*",
    "172.25.*",
    "172.26.*",
    "172.27.*",
    "172.28.*",
    "172.29.*",
    "172.30.*",
    "172.31.*",
    "192.168.*",
];

/// Build a semicolon-separated ProxyOverride string for Windows registry.
#[cfg(windows)]
pub fn proxy_override_string() -> String {
    let mut parts: Vec<&str> = PRIVATE_IP_RANGES.to_vec();
    parts.push("<local>");
    parts.join(";")
}
