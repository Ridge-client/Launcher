//! Sock-puppet persona environments: naming, configuration and Docker orchestration.

pub mod bookmarks;
pub mod docker;
pub mod firewall;
pub mod hostnet;
pub mod profile;
pub mod setup;
pub mod vpn;

use anyhow::{bail, Result};

pub const DEFAULT_BROWSER_IMAGE: &str = "lscr.io/linuxserver/firefox:latest";
pub const DEFAULT_VPN_IMAGE: &str = "qmcgaw/gluetun:latest";

/// Port the browser image serves its GUI on inside the container.
pub const GUI_CONTAINER_PORT: u16 = 3000;

pub const LABEL_MANAGED: &str = "ridge.managed";
pub const LABEL_PERSONA: &str = "ridge.persona";
pub const LABEL_ROLE: &str = "ridge.role";
/// On a browser container: the protections it was started with (`Protections::to_label`).
pub const LABEL_PROTECTIONS: &str = "ridge.protections";

/// Docker object names derived from a persona name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaNames {
    pub persona: String,
    pub volume: String,
    pub network: String,
    pub browser: String,
    pub vpn: String,
    pub firewall: String,
}

impl PersonaNames {
    pub fn new(persona: &str) -> Result<Self> {
        validate_name(persona)?;
        let prefix = format!("ridge_persona_{persona}");
        Ok(Self {
            persona: persona.to_string(),
            volume: format!("{prefix}_data"),
            network: format!("{prefix}_net"),
            browser: format!("{prefix}_browser"),
            vpn: format!("{prefix}_vpn"),
            firewall: format!("{prefix}_fw"),
        })
    }
}

/// A Docker-safe persona id from any typed name: "Bob Carlson" -> "bob-carlson".
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() || c == '_' {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.truncate(32);
    slug.trim_end_matches(['-', '_']).to_string()
}

fn validate_name(name: &str) -> Result<()> {
    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    let starts_alnum = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric());
    if name.is_empty() || name.len() > 32 || !valid_chars || !starts_alnum {
        bail!(
            "invalid persona name '{name}': use 1-32 lowercase letters, digits, '-' or '_', \
             starting with a letter or digit"
        );
    }
    Ok(())
}

/// Ensures an image reference carries a tag or digest; an untagged pull would fetch every tag.
pub fn normalize_image_ref(image: &str) -> String {
    let last_segment = image.rsplit('/').next().unwrap_or(image);
    if image.contains('@') || last_segment.contains(':') {
        image.to_string()
    } else {
        format!("{image}:latest")
    }
}

/// First free localhost TCP port in `start..start+100`.
pub fn find_free_port(start: u16) -> Option<u16> {
    (start..start.saturating_add(100))
        .find(|&p| std::net::TcpListener::bind(("127.0.0.1", p)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_docker_names() {
        let n = PersonaNames::new("alice-1").unwrap();
        assert_eq!(n.volume, "ridge_persona_alice-1_data");
        assert_eq!(n.network, "ridge_persona_alice-1_net");
        assert_eq!(n.browser, "ridge_persona_alice-1_browser");
        assert_eq!(n.vpn, "ridge_persona_alice-1_vpn");
    }

    #[test]
    fn rejects_bad_names() {
        for bad in ["", "Alice", "a b", "-x", "../x", "x/y", &"a".repeat(33)] {
            assert!(
                PersonaNames::new(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
        assert!(PersonaNames::new("bob_2").is_ok());
    }

    #[test]
    fn slugify_makes_valid_names() {
        assert_eq!(slugify("Bob carlson"), "bob-carlson");
        assert_eq!(slugify("  Émile  O'Brien!! "), "mile-o-brien");
        assert_eq!(slugify("alice_2"), "alice_2");
        assert_eq!(slugify("---"), "");
        assert!(PersonaNames::new(&slugify(&"Very Long Name ".repeat(5))).is_ok());
    }

    #[test]
    fn image_refs_get_a_tag() {
        assert_eq!(normalize_image_ref("alpine"), "alpine:latest");
        assert_eq!(
            normalize_image_ref("qmcgaw/gluetun:v3"),
            "qmcgaw/gluetun:v3"
        );
        assert_eq!(
            normalize_image_ref("localhost:5000/firefox"),
            "localhost:5000/firefox:latest"
        );
        assert_eq!(normalize_image_ref("x@sha256:abc"), "x@sha256:abc");
    }
}
