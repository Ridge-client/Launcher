//! VPN sidecar (gluetun) configuration.

use std::path::Path;

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
pub struct VpnConfig {
    pub country: String,
    pub provider: Option<String>,
    /// Extra KEY=VALUE settings (credentials, VPN_TYPE, ...) for gluetun.
    pub extra_env: Vec<(String, String)>,
}

impl VpnConfig {
    pub fn new(country: String, provider: Option<String>, env_file: Option<&Path>) -> Result<Self> {
        let extra_env = match env_file {
            Some(path) => {
                let raw = std::fs::read_to_string(path)
                    .with_context(|| format!("reading VPN env file {}", path.display()))?;
                parse_env_file(&raw)
                    .with_context(|| format!("parsing VPN env file {}", path.display()))?
            }
            None => Vec::new(),
        };
        let config = Self {
            country,
            provider,
            extra_env,
        };
        if config.provider.is_none() && config.lookup("VPN_SERVICE_PROVIDER").is_none() {
            bail!(
                "no VPN provider: pass --vpn-provider or set VPN_SERVICE_PROVIDER in --vpn-env-file"
            );
        }
        Ok(config)
    }

    fn lookup(&self, key: &str) -> Option<&str> {
        self.extra_env
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Environment for the gluetun container. Explicit flags win over the env file.
    pub fn container_env(&self) -> Vec<String> {
        let mut env: Vec<String> = self
            .extra_env
            .iter()
            .filter(|(k, _)| k != "VPN_SERVICE_PROVIDER" && k != "SERVER_COUNTRIES")
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        let provider = self
            .provider
            .as_deref()
            .or_else(|| self.lookup("VPN_SERVICE_PROVIDER"))
            .unwrap_or_default();
        env.push(format!("VPN_SERVICE_PROVIDER={provider}"));
        env.push(format!("SERVER_COUNTRIES={}", self.country));
        env
    }
}

/// Parses `KEY=VALUE` lines; blank lines and `#` comments are ignored, surrounding quotes stripped.
pub fn parse_env_file(raw: &str) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            bail!("line {}: expected KEY=VALUE", i + 1);
        };
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            bail!("line {}: invalid key '{key}'", i + 1);
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        out.push((key.to_string(), value.to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_env_file() {
        let env = parse_env_file(
            "# comment\n\nVPN_TYPE=wireguard\nexport WIREGUARD_PRIVATE_KEY=\"abc=\"\nX='y z'\n",
        )
        .unwrap();
        assert_eq!(
            env,
            vec![
                ("VPN_TYPE".into(), "wireguard".into()),
                ("WIREGUARD_PRIVATE_KEY".into(), "abc=".into()),
                ("X".into(), "y z".into()),
            ]
        );
        assert!(parse_env_file("NOEQUALS").is_err());
        assert!(parse_env_file("BAD KEY=1").is_err());
    }

    #[test]
    fn flags_override_env_file() {
        let cfg = VpnConfig {
            country: "Sweden".into(),
            provider: Some("mullvad".into()),
            extra_env: vec![
                ("VPN_SERVICE_PROVIDER".into(), "other".into()),
                ("SERVER_COUNTRIES".into(), "France".into()),
                ("VPN_TYPE".into(), "wireguard".into()),
            ],
        };
        assert_eq!(
            cfg.container_env(),
            vec![
                "VPN_TYPE=wireguard",
                "VPN_SERVICE_PROVIDER=mullvad",
                "SERVER_COUNTRIES=Sweden"
            ]
        );
    }

    #[test]
    fn provider_required() {
        assert!(VpnConfig::new("Sweden".into(), None, None).is_err());
        assert!(VpnConfig::new("Sweden".into(), Some("mullvad".into()), None).is_ok());
    }
}
