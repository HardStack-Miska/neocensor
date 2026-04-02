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

    // Verify SHA256 checksum if .dgst URL is available
    if let Some(dgst_url) = dgst_url {
        match verify_sha256(&client, dgst_url, &bytes).await {
            Ok(()) => {}
            Err(e) if e.to_string().contains("SHA256 mismatch") => {
                // Checksum mismatch is a hard failure — binary may be corrupted/tampered
                return Err(e);
            }
            Err(e) => {
                // .dgst fetch failed (404, network) — warn but proceed
                tracing::warn!("SHA256 verification unavailable: {e}");
            }
        }
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
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        tokio::process::Command::new("powershell")
            .creation_flags(CREATE_NO_WINDOW)
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

/// Verify SHA256 of downloaded bytes against a .dgst file from the release.
/// Xray .dgst files contain lines like: `SHA256= <hex_hash>`
async fn verify_sha256(
    client: &reqwest::Client,
    dgst_url: &str,
    data: &[u8],
) -> Result<()> {
    let resp = client
        .get(dgst_url)
        .send()
        .await
        .context("failed to fetch .dgst file")?;

    if !resp.status().is_success() {
        return Err(anyhow!(".dgst fetch failed: HTTP {}", resp.status()));
    }

    let dgst_body = resp.text().await?;

    // Parse SHA256 hash from .dgst file
    let expected = dgst_body
        .lines()
        .find(|line| line.starts_with("SHA256"))
        .and_then(|line| line.split('=').nth(1))
        .map(|h| h.trim().to_lowercase())
        .ok_or_else(|| anyhow!("SHA256 not found in .dgst file"))?;

    // Compute actual hash
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
