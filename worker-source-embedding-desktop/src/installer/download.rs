use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use reqwest::blocking::Client;
use sha2::{Digest, Sha256};

pub(super) fn download_to_path(
    download_url: &str,
    bearer_token: Option<&str>,
    destination: &Path,
    expected_sha256: &str,
) -> Result<(), String> {
    let normalized_sha256 = normalize_sha256(expected_sha256)?;
    let mut request = Client::new().get(download_url);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    let mut response = request.send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Download failed for {} ({})",
            download_url,
            response.status()
        ));
    }

    let mut file = fs::File::create(destination).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];

    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
    }

    let actual = format!("{:x}", hasher.finalize());
    if actual == normalized_sha256 {
        return Ok(());
    }

    let _ = fs::remove_file(destination);
    Err(format!(
        "Invalid sha256 for {}: expected {}, got {}",
        destination.display(),
        normalized_sha256,
        actual
    ))
}

fn normalize_sha256(expected_sha256: &str) -> Result<String, String> {
    let normalized = expected_sha256.trim().to_ascii_lowercase();
    if normalized.len() == 64
        && normalized
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Ok(normalized);
    }

    Err("Bundle manifest is missing a valid sha256 checksum.".to_string())
}

#[cfg(test)]
mod tests {
    use super::normalize_sha256;

    #[test]
    fn sha256_must_be_present_and_well_formed() {
        assert!(normalize_sha256("").is_err());
        assert!(normalize_sha256("abc").is_err());
        assert_eq!(
            normalize_sha256("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }
}
