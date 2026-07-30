use crate::config::HealthchecksConfig;
use reqwest::Client;
use tracing::{info, warn};

const BASE_URL: &str = "https://hc-ping.com";

pub struct HealthchecksClient {
    client: Client,
    uuid: String,
}

impl HealthchecksClient {
    pub fn from_config(config: &HealthchecksConfig) -> Option<Self> {
        if config.uuid.is_empty() {
            return None;
        }
        let uuid = extract_uuid(&config.uuid)?;
        Some(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .ok()?,
            uuid,
        })
    }

    pub async fn start(&self) {
        let url = format!("{}/{}/start", BASE_URL, self.uuid);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    info!(status = %status, "healthchecks start ping");
                }
            }
            Err(e) => warn!(error = %e, "healthchecks start ping failed"),
        }
    }

    pub async fn ping(&self) {
        let url = format!("{}/{}", BASE_URL, self.uuid);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    info!(status = %status, "healthchecks ping");
                }
            }
            Err(e) => warn!(error = %e, "healthchecks ping failed"),
        }
    }
}

fn extract_uuid(input: &str) -> Option<String> {
    let uuid_len = 36usize;
    let bytes = input.as_bytes();
    if bytes.len() < uuid_len {
        return None;
    }
    for start in 0..=bytes.len() - uuid_len {
        let slice = &bytes[start..start + uuid_len];
        if matches_uuid_pattern(slice) {
            return Some(input[start..start + uuid_len].to_string());
        }
    }
    None
}

fn matches_uuid_pattern(s: &[u8]) -> bool {
    if s.len() != 36 {
        return false;
    }
    // UUID: 8-4-4-4-12 hex digits
    for (i, &b) in s.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if b != b'-' {
                    return false;
                }
            }
            _ => {
                if !b.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_uuid_bare() {
        let uuid = "b6fb34e0-de2d-4b37-8edc-c32c6745f03f";
        assert_eq!(extract_uuid(uuid).as_deref(), Some(uuid));
    }

    #[test]
    fn test_extract_uuid_from_healthchecks_url() {
        let input = "https://hc-ping.com/b6fb34e0-de2d-4b37-8edc-c32c6745f03f";
        assert_eq!(
            extract_uuid(input).as_deref(),
            Some("b6fb34e0-de2d-4b37-8edc-c32c6745f03f")
        );
    }

    #[test]
    fn test_extract_uuid_from_selfhosted_url() {
        let input = "https://healthchecks.example.com/b6fb34e0-de2d-4b37-8edc-c32c6745f03f";
        assert_eq!(
            extract_uuid(input).as_deref(),
            Some("b6fb34e0-de2d-4b37-8edc-c32c6745f03f")
        );
    }

    #[test]
    fn test_extract_uuid_with_trailing_path() {
        let input = "https://hc-ping.com/b6fb34e0-de2d-4b37-8edc-c32c6745f03f/start";
        assert_eq!(
            extract_uuid(input).as_deref(),
            Some("b6fb34e0-de2d-4b37-8edc-c32c6745f03f")
        );
    }

    #[test]
    fn test_extract_uuid_empty() {
        assert_eq!(extract_uuid(""), None);
    }

    #[test]
    fn test_extract_uuid_invalid() {
        assert_eq!(extract_uuid("not-a-uuid"), None);
    }

    #[test]
    fn test_extract_uuid_too_short() {
        assert_eq!(extract_uuid("abc"), None);
    }

    #[test]
    fn test_matches_uuid_pattern_valid() {
        let uuid = b"b6fb34e0-de2d-4b37-8edc-c32c6745f03f";
        assert!(matches_uuid_pattern(uuid));
    }

    #[test]
    fn test_matches_uuid_pattern_uppercase() {
        let uuid = b"B6FB34E0-DE2D-4B37-8EDC-C32C6745F03F";
        assert!(matches_uuid_pattern(uuid));
    }

    #[test]
    fn test_matches_uuid_pattern_bad_hyphen_pos() {
        let uuid = b"b6fb34e0de2d-4b37-8edc-c32c6745f03f";
        assert!(!matches_uuid_pattern(uuid));
    }
}
