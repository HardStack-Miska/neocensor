use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// Download and extract xray-core from GitHub releases.
/// Kept for potential future use; sing-box is the primary engine now.
#[allow(dead_code)]
pub async fn download_xray(version: &str, dest: &Path) -> Result<PathBuf> {
    let asset = "Xray-windows-64.zip";
    let url = format!(
        "https://github.com/XTLS/Xray-core/releases/download/v{version}/{asset}"
    );
    let dgst_url = format!("{url}.dgst");

    tracing::info!("downloading xray-core v{version} from {url}");
    let bin_path = download_and_extract_zip(&url, Some(&dgst_url), dest, "xray.exe").await?;
    tracing::info!("xray-core installed to {}", bin_path.display());
    Ok(bin_path)
}

/// Download and extract sing-box from GitHub releases.
pub async fn download_singbox(version: &str, dest: &Path) -> Result<PathBuf> {
    let asset = format!("sing-box-{version}-windows-amd64.zip");
    let url = format!(
        "https://github.com/SagerNet/sing-box/releases/download/v{version}/{asset}"
    );
    // sing-box releases include checksums.txt
    let dgst_url = format!(
        "https://github.com/SagerNet/sing-box/releases/download/v{version}/sing-box-{version}-windows-amd64.zip.sha256"
    );

    tracing::info!("downloading sing-box v{version} from {url}");
    let bin_path = download_and_extract_zip(&url, Some(&dgst_url), dest, "sing-box.exe").await?;
    tracing::info!("sing-box installed to {}", bin_path.display());
    Ok(bin_path)
}

/// Check latest release version from GitHub API.
pub async fn check_latest_version(repo: &str) -> Result<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent("NeoCensor/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;

    resp.get("tag_name")
        .and_then(|t| t.as_str())
        .map(|t| t.trim_start_matches('v').to_string())
        .ok_or_else(|| anyhow!("no tag_name in release response"))
}

/// Download a ZIP from URL, optionally verify SHA256, extract a specific binary.
async fn download_and_extract_zip(
    url: &str,
    dgst_url: Option<&str>,
    dest_dir: &Path,
    binary_name: &str,
) -> Result<PathBuf> {
    std::fs::create_dir_all(dest_dir)?;

    let client = reqwest::Client::builder()
        .user_agent("NeoCensor/0.1")
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let resp = client
        .get(url)
        .send()
        .await
        .context("failed to download")?;

    if !resp.status().is_success() {
        return Err(anyhow!("download failed: HTTP {}", resp.status()));
    }

    let bytes = resp.bytes().await.context("failed to read response body")?;

    // Verify SHA256 checksum if checksum URL is available.
    // For supply-chain safety we treat any verification failure as fatal — including
    // network/parse errors. A missing checksum file is a release-process bug, not a
    // reason to install an unverified binary.
    if let Some(dgst_url) = dgst_url {
        verify_sha256(&client, dgst_url, &bytes).await?;
    }

    // Write to temp zip file
    let zip_path = dest_dir.join("_download.zip");
    let mut file = tokio::fs::File::create(&zip_path).await?;
    file.write_all(&bytes).await?;
    file.flush().await?;
    drop(file);

    // Extract using PowerShell Expand-Archive
    let dest_str = dest_dir.to_string_lossy().replace('/', "\\");
    let zip_str = zip_path.to_string_lossy().replace('/', "\\");

    #[cfg(windows)]
    let output = {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;
        tokio::process::Command::new("powershell")
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    zip_str, dest_str
                ),
            ])
            .output()
            .await
            .context("failed to run Expand-Archive")?
    };
    #[cfg(not(windows))]
    let output = tokio::process::Command::new("unzip")
        .args(["-o", &zip_str, "-d", &dest_str])
        .output()
        .await
        .context("failed to run unzip")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Expand-Archive failed: {stderr}"));
    }

    // Clean up zip
    tokio::fs::remove_file(&zip_path).await.ok();

    // Find the binary (may be in a subdirectory)
    let bin_path = find_file_recursive(dest_dir, binary_name)?;
    let final_path = dest_dir.join(binary_name);

    if bin_path != final_path {
        std::fs::copy(&bin_path, &final_path)?;
    }

    Ok(final_path)
}

/// Verify SHA256 of downloaded bytes against a checksum file from the release.
///
/// Supports two formats:
///   - sing-box / standard `sha256sum` output: `<64-hex-hash>  <filename>` (one or more lines)
///   - Xray `.dgst` format: `SHA256= <hex_hash>`
async fn verify_sha256(
    client: &reqwest::Client,
    dgst_url: &str,
    data: &[u8],
) -> Result<()> {
    let resp = client
        .get(dgst_url)
        .send()
        .await
        .context("failed to fetch checksum file")?;

    if !resp.status().is_success() {
        return Err(anyhow!("checksum fetch failed: HTTP {}", resp.status()));
    }

    let dgst_body = resp.text().await?;
    let expected = parse_sha256_from_checksum(&dgst_body)
        .ok_or_else(|| anyhow!("could not extract SHA256 from checksum file"))?;

    let mut hasher = Sha256::new();
    hasher.update(data);
    let actual = hex::encode(hasher.finalize());

    if actual != expected {
        return Err(anyhow!(
            "SHA256 mismatch: expected {expected}, got {actual}"
        ));
    }

    tracing::info!("SHA256 checksum verified: {actual}");
    Ok(())
}

/// Extract a 64-char hex SHA256 from a checksum file body.
/// Tries sha256sum-style format first, then Xray `SHA256=` format.
fn parse_sha256_from_checksum(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // sha256sum format: leading 64-hex token
        if let Some(first) = trimmed.split_whitespace().next() {
            if first.len() == 64 && first.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(first.to_lowercase());
            }
        }

        // Xray .dgst format: "SHA256= <hex>"
        if trimmed.starts_with("SHA256") {
            if let Some(after_eq) = trimmed.split('=').nth(1) {
                let candidate = after_eq.trim().to_lowercase();
                if candidate.len() == 64 && candidate.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sha256_singbox_format() {
        let body = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef  sing-box-1.11.0-windows-amd64.zip\n";
        let hash = parse_sha256_from_checksum(body).unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.starts_with("1234567890abcdef"));
    }

    #[test]
    fn parse_sha256_xray_format() {
        let body = "MD5= ignored\nSHA1= ignored\nSHA256= ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890\n";
        let hash = parse_sha256_from_checksum(body).unwrap();
        assert_eq!(hash, "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890");
    }

    #[test]
    fn parse_sha256_returns_none_on_garbage() {
        assert!(parse_sha256_from_checksum("not a checksum file").is_none());
        assert!(parse_sha256_from_checksum("").is_none());
        // Too short
        assert!(parse_sha256_from_checksum("abc123  file.zip").is_none());
    }

    #[test]
    fn parse_sha256_skips_comments_and_blanks() {
        let body = "# comment\n\nDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF  file.zip\n";
        let hash = parse_sha256_from_checksum(body).unwrap();
        assert!(hash.starts_with("deadbeef"));
    }
}

fn find_file_recursive(dir: &Path, name: &str) -> Result<PathBuf> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.file_name().map(|f| f == name).unwrap_or(false) {
            return Ok(path);
        }
        if path.is_dir() {
            if let Ok(found) = find_file_recursive(&path, name) {
                return Ok(found);
            }
        }
    }
    Err(anyhow!("{name} not found in {}", dir.display()))
}
