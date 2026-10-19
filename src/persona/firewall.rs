//! Per-persona protections: a small firewall container that holds the persona's network (the
//! browser joins it and has no right to change its rules), and the checks that show what a
//! persona's browser can reach.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The firewall image, built once on this PC from [`DOCKERFILE`].
/// It also runs the checks of the test persona, so it has bash.
pub const FIREWALL_IMAGE: &str = "ridge-firewall:2";
const DOCKERFILE: &str = "FROM alpine:3.20\nRUN apk add --no-cache iptables ip6tables bash\n";
/// The firewall container creates this file once its rules are in place.
pub const READY_FILE: &str = "/tmp/ridge-firewall-ready";

/// What a persona's browser is kept away from. Everything is on by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Protections {
    /// This computer: Windows file sharing, remote access and its other services.
    pub block_pc: bool,
    /// The home network: the router and the other devices on it.
    pub block_lan: bool,
    /// The other personas (and the other way round).
    pub block_personas: bool,
    /// The persona's remote screen asks for a password (which Ridge fills in).
    pub password: bool,
}

impl Default for Protections {
    fn default() -> Self {
        Self {
            block_pc: true,
            block_lan: true,
            block_personas: true,
            password: true,
        }
    }
}

impl Protections {
    /// Whether any network rule is needed (the firewall container is skipped otherwise).
    pub fn blocks_anything(&self) -> bool {
        self.block_pc || self.block_lan || self.block_personas
    }

    /// All off: a persona started without a firewall or password.
    pub const NONE: Self = Self {
        block_pc: false,
        block_lan: false,
        block_personas: false,
        password: false,
    };

    /// "pc,lan,personas,password" with the ones that are on, for a container label.
    pub fn to_label(self) -> String {
        [
            (self.block_pc, "pc"),
            (self.block_lan, "lan"),
            (self.block_personas, "personas"),
            (self.password, "password"),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(",")
    }

    pub fn from_label(label: &str) -> Self {
        let on = |name: &str| label.split(',').any(|part| part == name);
        Self {
            block_pc: on("pc"),
            block_lan: on("lan"),
            block_personas: on("personas"),
            password: on("password"),
        }
    }
}

/// Docker Desktop's side of the PC as containers see it (`host.docker.internal`).
const DOCKER_DESKTOP_HOST: &str = "192.168.65.0/24";
/// Private IPv4 ranges a home network uses, plus link-local and carrier-grade NAT.
const PRIVATE_V4: [&str; 5] = [
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "100.64.0.0/10",
    "169.254.0.0/16",
];
/// Where Docker puts its networks, so the other personas' too.
const DOCKER_NETWORKS: &str = "172.16.0.0/12";
/// Private and link-local IPv6 (Docker Desktop's `host.docker.internal` is a private one).
const PRIVATE_V6: [&str; 2] = ["fc00::/7", "fe80::/10"];

/// The firewall container's start script: rules for `protections`, then wait forever.
/// `pc_addresses` are this PC's own IPv4 addresses on the home network.
pub fn rules_script(protections: &Protections, pc_addresses: &[String]) -> String {
    let mut v4: Vec<String> = Vec::new();
    let mut v6: Vec<String> = Vec::new();
    let add = |list: &mut Vec<String>, net: &str| {
        if !list.iter().any(|n| n == net) {
            list.push(net.to_string());
        }
    };
    if protections.block_pc {
        add(&mut v4, DOCKER_DESKTOP_HOST);
        for ip in pc_addresses {
            add(&mut v4, &format!("{ip}/32"));
        }
        add(&mut v6, PRIVATE_V6[0]);
    }
    if protections.block_lan {
        for net in PRIVATE_V4 {
            add(&mut v4, net);
        }
        for net in PRIVATE_V6 {
            add(&mut v6, net);
        }
    }
    if protections.block_personas {
        add(&mut v4, DOCKER_NETWORKS);
    }

    let mut script = String::from("set -e\n");
    for (tool, nets, reject) in [
        ("iptables", &v4, "icmp-port-unreachable"),
        ("ip6tables", &v6, "icmp6-port-unreachable"),
    ] {
        // replies to the remote screen and anything this browser opened go through untouched
        script.push_str(&format!(
            "{tool} -A OUTPUT -o lo -j ACCEPT\n\
             {tool} -A OUTPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT\n"
        ));
        for net in nets {
            script.push_str(&format!(
                "{tool} -A OUTPUT -p tcp -d {net} -j REJECT --reject-with tcp-reset\n\
                 {tool} -A OUTPUT -d {net} -j REJECT --reject-with {reject}\n"
            ));
        }
    }
    script.push_str(&format!("touch {READY_FILE}\nexec sleep infinity\n"));
    script
}

/// Tar build context holding the firewall image's Dockerfile.
pub fn build_context() -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(DOCKERFILE.len() as u64);
    builder.append_data(&mut header, "Dockerfile", DOCKERFILE.as_bytes())?;
    Ok(builder.into_inner()?)
}

/// One line of a persona check: what was tried, and whether the result is the safe one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub group: String,
    pub label: String,
    /// What was seen: "reachable", "blocked", "present", "none"…
    pub seen: String,
    pub safe: bool,
}

/// Another running persona, as seen from the one being checked.
pub struct OtherPersona {
    pub name: String,
    /// Its address on its Docker network.
    pub ip: Option<String>,
    /// Its remote screen's port on this PC.
    pub port: Option<u16>,
}

/// A bash script run inside a persona: each line it prints is `group|label|seen|safe(1/0)`.
/// It only tries connections and reads files; it changes nothing. `in_browser` adds the lines
/// about the browser container itself (Docker socket, hardware, folders); the test persona has
/// only the network ones.
pub fn checks_script(
    pc_address: Option<&str>,
    router: Option<&str>,
    others: &[OtherPersona],
    in_browser: bool,
) -> String {
    let mut s = String::from(
        r#"tcp() { if timeout 3 bash -c "</dev/tcp/$1/$2" 2>/dev/null; then echo "$3|$4|reachable|$5"; else echo "$3|$4|blocked|$6"; fi; }
tcp example.com 443 "Internet" "Websites (example.com)" 1 0
"#,
    );
    if let Some(pc) = pc_address {
        s.push_str(&format!(
            "tcp {pc} 445 \"This PC\" \"File sharing (SMB)\" 0 1\n\
             tcp {pc} 135 \"This PC\" \"Windows services (RPC)\" 0 1\n\
             tcp {pc} 3389 \"This PC\" \"Remote Desktop\" 0 1\n"
        ));
    }
    s.push_str(
        "tcp host.docker.internal 445 \"This PC\" \"File sharing through Docker\" 0 1\n\
         tcp host.docker.internal 2375 \"Docker\" \"Docker control port\" 0 1\n",
    );
    if let Some(router) = router {
        s.push_str(&format!(
            "tcp {router} 53 \"Home network\" \"Router (DNS)\" 0 1\n\
             tcp {router} 80 \"Home network\" \"Router (web admin)\" 0 1\n\
             tcp {router} 443 \"Home network\" \"Router (secure web admin)\" 0 1\n"
        ));
    }
    for other in others {
        if let Some(ip) = &other.ip {
            s.push_str(&format!(
                "tcp {ip} 3000 \"Other personas\" \"{} directly\" 0 1\n",
                other.name
            ));
        }
        if let Some(port) = other.port {
            s.push_str(&format!(
                "tcp host.docker.internal {port} \"Other personas\" \"{} through this PC\" 0 1\n",
                other.name
            ));
        }
    }
    if !in_browser {
        return s;
    }
    s.push_str(
        r#"[ -S /var/run/docker.sock ] && echo "Docker|Docker socket|present|0" || echo "Docker|Docker socket|none|1"
for d in /sys/class/video4linux/*; do [ -e "$d" ] && cam=1; done
[ -n "$cam" ] && echo "Hardware|Camera|present|0" || echo "Hardware|Camera|none|1"
[ -e /dev/snd ] && echo "Hardware|Sound devices (microphone)|present|0" || echo "Hardware|Sound devices (microphone)|none|1"
[ -e /dev/dri ] && echo "Hardware|Graphics card|present|0" || echo "Hardware|Graphics card|none|1"
[ -e /dev/bus/usb ] && echo "Hardware|USB devices|present|0" || echo "Hardware|USB devices|none|1"
m=$(grep -vE ' (proc|sysfs|cgroup2?|tmpfs|devpts|mqueue|overlay|shm) ' /proc/mounts | grep -vE ' /config | /etc/(resolv.conf|hostname|hosts) ' | wc -l)
[ "$m" = 0 ] && echo "This PC|Folders of this PC|none|1" || echo "This PC|Folders of this PC|present|0"
"#,
    );
    s
}

/// Reads the lines printed by [`checks_script`].
pub fn parse_checks(output: &str) -> Vec<Check> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().splitn(4, '|');
            let (group, label, seen, safe) =
                (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            Some(Check {
                group: group.to_string(),
                label: label.to_string(),
                seen: seen.to_string(),
                safe: safe == "1",
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_on_blocks_pc_lan_and_personas() {
        let s = rules_script(&Protections::default(), &["10.0.0.5".into()]);
        for net in [
            "192.168.65.0/24",
            "10.0.0.5/32",
            "10.0.0.0/8",
            "192.168.0.0/16",
            "172.16.0.0/12",
            "fc00::/7",
            "fe80::/10",
        ] {
            assert!(s.contains(&format!("-d {net} ")), "{net} not blocked");
        }
        // each range once, even when two protections cover it
        assert_eq!(s.matches("-p tcp -d 172.16.0.0/12 ").count(), 1);
        assert!(s.contains("ESTABLISHED,RELATED -j ACCEPT"));
        assert!(s.ends_with("exec sleep infinity\n"));
    }

    #[test]
    fn nothing_blocked_when_all_off() {
        let off = Protections {
            block_pc: false,
            block_lan: false,
            block_personas: false,
            password: false,
        };
        assert!(!off.blocks_anything());
        assert!(!rules_script(&off, &[]).contains("REJECT"));
    }

    #[test]
    fn only_other_personas() {
        let p = Protections {
            block_pc: false,
            block_lan: false,
            block_personas: true,
            password: false,
        };
        let s = rules_script(&p, &["10.0.0.5".into()]);
        assert!(s.contains("-d 172.16.0.0/12 "));
        assert!(!s.contains("10.0.0.5") && !s.contains("192.168.") && !s.contains("fc00"));
    }

    #[test]
    fn label_round_trip() {
        for p in [Protections::default(), Protections::NONE] {
            assert_eq!(Protections::from_label(&p.to_label()), p);
        }
        let only_pc = Protections {
            block_pc: true,
            ..Protections::NONE
        };
        assert_eq!(only_pc.to_label(), "pc");
        assert_eq!(Protections::from_label("pc"), only_pc);
    }

    #[test]
    fn checks_round_trip() {
        let script = checks_script(
            Some("10.0.0.5"),
            Some("10.0.0.1"),
            &[OtherPersona {
                name: "bob".into(),
                ip: Some("172.18.0.2".into()),
                port: Some(3000),
            }],
            true,
        );
        assert!(script.contains("Docker socket"));
        assert!(!checks_script(None, None, &[], false).contains("Hardware"));
        assert!(script.contains("tcp 10.0.0.5 445"));
        assert!(script.contains("tcp 10.0.0.1 80"));
        assert!(script.contains("tcp 172.18.0.2 3000 \"Other personas\" \"bob directly\""));
        let checks = parse_checks(
            "Internet|Websites (example.com)|reachable|1\nThis PC|File sharing (SMB)|reachable|0\nnoise\n",
        );
        assert_eq!(checks.len(), 2);
        assert!(checks[0].safe && !checks[1].safe);
        assert_eq!(checks[1].seen, "reachable");
    }
}
