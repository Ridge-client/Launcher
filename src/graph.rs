//! Investigation graphs: entities (people, emails, buildings, crypto wallets…) placed on a board,
//! each with its own facts and notes, and labelled links between them.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// A kind of entity: its name, icon, category (which sets its colour), and the facts it has.
pub struct Kind {
    pub key: &'static str,
    /// `CATEGORIES` entry it is listed under.
    pub category: &'static str,
    pub label: &'static str,
    /// A Phosphor icon (see `gui::icons`).
    pub icon: &'static str,
    pub fields: &'static [KindField],
}

pub struct KindField {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    /// Values to pick from a menu next to the box (it still takes any text).
    pub presets: &'static [&'static str],
}

const fn field(key: &'static str, label: &'static str, hint: &'static str) -> KindField {
    KindField {
        key,
        label,
        hint,
        presets: &[],
    }
}

const fn pick(
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    presets: &'static [&'static str],
) -> KindField {
    KindField {
        presets,
        ..field(key, label, hint)
    }
}

const COUNTRY: KindField = field("country", "Country", "France");
const PLATFORMS: &[&str] = &[
    "Facebook",
    "Instagram",
    "X (Twitter)",
    "TikTok",
    "LinkedIn",
    "Reddit",
    "YouTube",
    "Telegram",
    "WhatsApp",
    "Discord",
    "Snapchat",
    "GitHub",
];

/// Each category's colour, which all its kinds share: hues spread around the wheel.
pub fn category_colour(category: &str) -> [u8; 3] {
    match category {
        "Cryptocurrency" => [0xd3, 0x68, 0x68],
        "Custom" => [0x68, 0xd3, 0x87],
        "Devices" => [0xa7, 0x68, 0xd3],
        "Events" => [0xd3, 0xc6, 0x68],
        "Groups" => [0x68, 0xc1, 0xd3],
        "Infrastructure" => [0xd3, 0x68, 0xa2],
        "Locations" => [0x83, 0xd3, 0x68],
        "Malware" => [0x6d, 0x68, 0xd3],
        "MISP" => [0xd3, 0x8c, 0x68],
        "Money" => [0x68, 0xd3, 0xab],
        "Media & files" => [0xca, 0x68, 0xd3],
        "Organisations" => [0xbd, 0xd3, 0x68],
        "PassiveTotal" => [0x68, 0x9d, 0xd3],
        "Peering DB" => [0xd3, 0x68, 0x7e],
        "Penetration testing" => [0x68, 0xd3, 0x71],
        "People" => [0x90, 0x68, 0xd3],
        "Personal" => [0xd3, 0xb0, 0x68],
        "Project Hades" => [0x68, 0xd3, 0xcf],
        "Social links" => [0xd3, 0x68, 0xb8],
        "Social network" => [0x99, 0xd3, 0x68],
        "Technology" => [0x68, 0x7a, 0xd3],
        "Tracking" => [0xd3, 0x76, 0x68],
        "Transportation" => [0x68, 0xd3, 0x95],
        "Weapons" => [0xb4, 0x68, 0xd3],
        "Wikimedia" => [0xd3, 0xd3, 0x68],
        _ => [0x90, 0x90, 0x90],
    }
}

impl Kind {
    pub fn colour(&self) -> [u8; 3] {
        category_colour(self.category)
    }
}

/// How an entity's border or a link is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineStyle {
    #[default]
    Solid,
    Dotted,
    ShortDashes,
    LongDashes,
}

impl LineStyle {
    pub const ALL: [Self; 4] = [
        Self::Solid,
        Self::Dotted,
        Self::ShortDashes,
        Self::LongDashes,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dotted => "Dotted",
            Self::ShortDashes => "Short dashes",
            Self::LongDashes => "Long dashes",
        }
    }
}

/// Which way a link points.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arrow {
    #[default]
    None,
    /// From `Link::from` to `Link::to`.
    Forward,
    Backward,
    Both,
}

impl Arrow {
    pub const ALL: [Self; 4] = [Self::None, Self::Forward, Self::Backward, Self::Both];

    /// Its icon (Phosphor, see `gui::icons`).
    pub fn symbol(self) -> &'static str {
        match self {
            Self::None => "\u{E32A}",     // minus
            Self::Forward => "\u{E06C}",  // arrow-right
            Self::Backward => "\u{E058}", // arrow-left
            Self::Both => "\u{E0A0}",     // arrows-left-right
        }
    }

    pub fn to_end(self) -> bool {
        matches!(self, Self::Forward | Self::Both)
    }

    pub fn to_start(self) -> bool {
        matches!(self, Self::Backward | Self::Both)
    }
}

/// The palette's sections, in order (alphabetical, like Maltego's).
pub const CATEGORIES: &[&str] = &[
    "Cryptocurrency",
    "Custom",
    "Devices",
    "Events",
    "Groups",
    "Infrastructure",
    "Locations",
    "Malware",
    "MISP",
    "Money",
    "Media & files",
    "Organisations",
    "PassiveTotal",
    "Peering DB",
    "Penetration testing",
    "People",
    "Personal",
    "Project Hades",
    "Social links",
    "Social network",
    "Technology",
    "Tracking",
    "Transportation",
    "Weapons",
    "Wikimedia",
];

/// Every kind, in palette order. The first field of each names the entity on the board.
pub const KINDS: &[Kind] = &[
    Kind {
        key: "crypto",
        category: "Cryptocurrency",
        label: "Crypto wallet",
        icon: "\u{E68A}", // wallet
        fields: &[
            field("address", "Address", "bc1q…"),
            pick(
                "network",
                "Network",
                "Bitcoin",
                &[
                    "Bitcoin",
                    "Ethereum",
                    "Solana",
                    "Tron",
                    "BNB Chain",
                    "Polygon",
                    "Litecoin",
                    "Monero",
                    "Arbitrum",
                    "Base",
                ],
            ),
            field("balance", "Balance", ""),
            field("exchange", "Linked exchange", ""),
        ],
    },
    Kind {
        key: "crypto_tx",
        category: "Cryptocurrency",
        label: "Crypto transaction",
        icon: "\u{EADA}", // currency-eth
        fields: &[
            field("hash", "Transaction hash", ""),
            field("amount", "Amount", "0.5 BTC"),
            field("date", "Date", ""),
            pick(
                "network",
                "Network",
                "Bitcoin",
                &[
                    "Bitcoin",
                    "Ethereum",
                    "Solana",
                    "Tron",
                    "BNB Chain",
                    "Polygon",
                    "Litecoin",
                    "Monero",
                ],
            ),
        ],
    },
    Kind {
        key: "exchange",
        category: "Cryptocurrency",
        label: "Exchange account",
        icon: "\u{E78E}", // coins
        fields: &[
            field("exchange", "Exchange", "Binance"),
            field("account", "Account / ID", ""),
            field("kyc", "KYC name", ""),
        ],
    },
    Kind {
        key: "token",
        category: "Cryptocurrency",
        label: "Token / NFT",
        icon: "\u{E2AE}", // hexagon
        fields: &[
            field("name", "Name", ""),
            field("contract", "Contract address", ""),
            pick(
                "network",
                "Network",
                "Ethereum",
                &["Ethereum", "Solana", "Polygon", "BNB Chain", "Base"],
            ),
        ],
    },
    Kind {
        key: "contract",
        category: "Cryptocurrency",
        label: "Smart contract",
        icon: "\u{E914}", // file-code
        fields: &[
            field("address", "Address", ""),
            pick(
                "network",
                "Network",
                "Ethereum",
                &["Ethereum", "BNB Chain", "Polygon", "Arbitrum", "Base"],
            ),
            field("creator", "Deployed by", ""),
        ],
    },
    Kind {
        key: "custom",
        category: "Custom",
        label: "Custom",
        icon: "\u{E596}", // puzzle-piece
        fields: &[
            field("value", "Value", ""),
            field("type", "What it is", "Licence key"),
            field("detail", "Detail", ""),
        ],
    },
    Kind {
        key: "device",
        category: "Devices",
        label: "Phone / device",
        icon: "\u{E1E0}", // device-mobile
        fields: &[
            field("model", "Model", "iPhone 13"),
            field("imei", "IMEI / serial", ""),
            field("mac", "MAC address", ""),
            field("os", "System", ""),
        ],
    },
    Kind {
        key: "computer",
        category: "Devices",
        label: "Computer",
        icon: "\u{E586}", // laptop
        fields: &[
            field("name", "Name / model", ""),
            field("os", "System", ""),
            field("mac", "MAC address", ""),
            field("user", "User", ""),
        ],
    },
    Kind {
        key: "sim",
        category: "Devices",
        label: "SIM card",
        icon: "\u{E664}", // sim-card
        fields: &[
            field("iccid", "ICCID", ""),
            field("imsi", "IMSI", ""),
            field("carrier", "Carrier", ""),
            field("number", "Number", ""),
        ],
    },
    Kind {
        key: "camera",
        category: "Devices",
        label: "Camera / CCTV",
        icon: "\u{ECA4}", // security-camera
        fields: &[
            field("name", "Name", ""),
            field("place", "Where", ""),
            field("url", "Stream URL", ""),
        ],
    },
    Kind {
        key: "wifi",
        category: "Devices",
        label: "Wi-Fi network",
        icon: "\u{E4EA}", // wifi-high
        fields: &[
            field("ssid", "SSID", "CafeGuest"),
            field("bssid", "BSSID / MAC", ""),
            field("place", "Seen at", ""),
        ],
    },
    Kind {
        key: "event",
        category: "Events",
        label: "Event",
        icon: "\u{E108}", // calendar
        fields: &[
            field("name", "Event", "Conference"),
            field("date", "Date", "2024-05-12"),
            field("place", "Place", ""),
        ],
    },
    Kind {
        key: "meeting",
        category: "Events",
        label: "Meeting",
        icon: "\u{E582}", // handshake
        fields: &[
            field("what", "What", ""),
            field("date", "Date", ""),
            field("place", "Place", ""),
            field("who", "Who was there", ""),
        ],
    },
    Kind {
        key: "call",
        category: "Events",
        label: "Call",
        icon: "\u{E3BA}", // phone-call
        fields: &[
            field("from", "From", ""),
            field("to", "To", ""),
            field("date", "Date and time", ""),
            field("length", "Length", ""),
        ],
    },
    Kind {
        key: "incident",
        category: "Events",
        label: "Incident",
        icon: "\u{E9B8}", // siren
        fields: &[
            field("what", "What", "Break-in"),
            field("date", "Date", ""),
            field("place", "Place", ""),
            field("source", "Source", ""),
        ],
    },
    Kind {
        key: "travel",
        category: "Events",
        label: "Trip",
        icon: "\u{E5AE}", // suitcase
        fields: &[
            field("where", "Where to", ""),
            field("from", "From", ""),
            field("to", "Until", ""),
            field("who", "Who", ""),
        ],
    },
    Kind {
        key: "group",
        category: "Groups",
        label: "Group",
        icon: "\u{E68E}", // users-three
        fields: &[
            field("name", "Name", "Neighbourhood watch"),
            pick(
                "type",
                "Type",
                "Family",
                &["Family", "Friends", "Gang", "Club", "Team"],
            ),
            field("members", "Members", ""),
        ],
    },
    Kind {
        key: "community",
        category: "Groups",
        label: "Online community",
        icon: "\u{E68C}", // users-four
        fields: &[
            field("name", "Name", ""),
            pick("platform", "Platform", "Reddit", PLATFORMS),
            field("members", "Members", ""),
            field("url", "Link", ""),
        ],
    },
    Kind {
        key: "domain",
        category: "Infrastructure",
        label: "Domain",
        icon: "\u{E288}", // globe
        fields: &[
            field("domain", "Domain", "example.com"),
            field("registrar", "Registrar", ""),
            field("created", "Registered", "2016-03-01"),
            field("owner", "Registrant", ""),
        ],
    },
    Kind {
        key: "dns",
        category: "Infrastructure",
        label: "DNS name",
        icon: "\u{E89C}", // signpost
        fields: &[
            field("name", "Name", "mail.example.com"),
            pick(
                "type",
                "Record",
                "A",
                &["A", "AAAA", "MX", "NS", "CNAME", "TXT"],
            ),
            field("value", "Points to", ""),
        ],
    },
    Kind {
        key: "website",
        category: "Infrastructure",
        label: "Website",
        icon: "\u{E0F4}", // browser
        fields: &[
            field("url", "URL", "https://example.com/about"),
            field("title", "Title", ""),
            field("archive", "Archived copy", "https://web.archive.org/…"),
        ],
    },
    Kind {
        key: "url",
        category: "Infrastructure",
        label: "URL",
        icon: "\u{E2E6}", // link-simple
        fields: &[
            field("url", "URL", "https://…"),
            field("seen", "Seen on", ""),
            field("date", "Date", ""),
        ],
    },
    Kind {
        key: "ip",
        category: "Infrastructure",
        label: "IP address",
        icon: "\u{E2A0}", // hard-drives
        fields: &[
            field("ip", "IP", "203.0.113.7"),
            field("isp", "ISP / ASN", ""),
            pick(
                "type",
                "Type",
                "",
                &["Residential", "Hosting", "VPN", "Tor exit", "Mobile"],
            ),
            COUNTRY,
        ],
    },
    Kind {
        key: "netblock",
        category: "Infrastructure",
        label: "Network / ASN",
        icon: "\u{E28C}", // globe-hemisphere-west
        fields: &[
            field("asn", "ASN or range", "AS13335"),
            field("owner", "Owner", ""),
            COUNTRY,
        ],
    },
    Kind {
        key: "hosting",
        category: "Infrastructure",
        label: "Server / hosting",
        icon: "\u{E1AA}", // cloud
        fields: &[
            field("name", "Host or provider", "Hetzner"),
            field("ip", "IP", ""),
            field("location", "Data centre", ""),
        ],
    },
    Kind {
        key: "certificate",
        category: "Infrastructure",
        label: "TLS certificate",
        icon: "\u{E766}", // certificate
        fields: &[
            field("subject", "Subject", "example.com"),
            field("issuer", "Issuer", ""),
            field("sha256", "SHA-256", ""),
            field("valid", "Valid until", ""),
        ],
    },
    Kind {
        key: "port",
        category: "Infrastructure",
        label: "Port / service",
        icon: "\u{E946}", // plug
        fields: &[
            field("port", "Port", "443/tcp"),
            field("service", "Service", "nginx 1.24"),
            field("banner", "Banner", ""),
        ],
    },
    Kind {
        key: "location",
        category: "Locations",
        label: "Location",
        icon: "\u{E316}", // map-pin
        fields: &[
            field("name", "Place", "Central Station"),
            field("address", "Address", ""),
            field("coordinates", "Coordinates", "48.8566, 2.3522"),
            COUNTRY,
        ],
    },
    Kind {
        key: "building",
        category: "Locations",
        label: "Building",
        icon: "\u{E2C2}", // house
        fields: &[
            field("address", "Address", "1234 Elm Street"),
            pick(
                "type",
                "Type",
                "House",
                &[
                    "House",
                    "Apartment",
                    "Office",
                    "Shop",
                    "Warehouse",
                    "Factory",
                    "Farm",
                    "Hotel",
                ],
            ),
            field("city", "City", "Columbus"),
            field("postcode", "Postcode", ""),
            COUNTRY,
            pick("use", "Seen as", "", &["Lives", "Works", "Owns", "Visits"]),
        ],
    },
    Kind {
        key: "address",
        category: "Locations",
        label: "Address",
        icon: "\u{E31A}", // map-trifold
        fields: &[
            field("street", "Street", "1234 Elm Street"),
            field("city", "City", ""),
            field("postcode", "Postcode", ""),
            COUNTRY,
        ],
    },
    Kind {
        key: "city",
        category: "Locations",
        label: "City",
        icon: "\u{EA6A}", // city
        fields: &[
            field("name", "City", "Columbus"),
            field("region", "Region", ""),
            COUNTRY,
        ],
    },
    Kind {
        key: "country",
        category: "Locations",
        label: "Country",
        icon: "\u{E1C8}", // compass
        fields: &[
            field("name", "Country", "France"),
            field("code", "Code", "FR"),
        ],
    },
    Kind {
        key: "malware",
        category: "Malware",
        label: "Malware",
        icon: "\u{E5F4}", // bug
        fields: &[
            field("name", "Family", "Emotet"),
            pick(
                "type",
                "Type",
                "",
                &["Trojan", "Ransomware", "Stealer", "RAT", "Loader", "Botnet"],
            ),
            field("first_seen", "First seen", ""),
        ],
    },
    Kind {
        key: "hash",
        category: "Malware",
        label: "File hash",
        icon: "\u{E23E}", // fingerprint
        fields: &[
            field("hash", "Hash", "SHA-256 …"),
            pick(
                "type",
                "Type",
                "SHA-256",
                &["MD5", "SHA-1", "SHA-256", "Perceptual"],
            ),
            field("file", "File name", ""),
        ],
    },
    Kind {
        key: "c2",
        category: "Malware",
        label: "C2 server",
        icon: "\u{E47C}", // target
        fields: &[
            field("address", "Address", "203.0.113.9:443"),
            field("malware", "Malware", ""),
            field("seen", "Last seen", ""),
        ],
    },
    Kind {
        key: "phishing",
        category: "Malware",
        label: "Phishing campaign",
        icon: "\u{E72A}", // fish-simple
        fields: &[
            field("name", "Name", ""),
            field("lure", "Lure", ""),
            field("url", "Landing URL", ""),
            field("date", "Date", ""),
        ],
    },
    Kind {
        key: "misp_event",
        category: "MISP",
        label: "MISP event",
        icon: "\u{E712}", // calendar-check
        fields: &[
            field("info", "Event info", ""),
            field("id", "Event ID", ""),
            pick(
                "threat",
                "Threat level",
                "",
                &["High", "Medium", "Low", "Undefined"],
            ),
            field("date", "Date", ""),
        ],
    },
    Kind {
        key: "indicator",
        category: "MISP",
        label: "Indicator (IOC)",
        icon: "\u{E7FC}", // warning-diamond
        fields: &[
            field("value", "Value", ""),
            pick(
                "type",
                "Type",
                "",
                &["ip-dst", "domain", "url", "sha256", "email-src", "filename"],
            ),
            field("tags", "Tags", ""),
        ],
    },
    Kind {
        key: "threat_actor",
        category: "MISP",
        label: "Threat actor",
        icon: "\u{E916}", // skull
        fields: &[
            field("name", "Name", "APT28"),
            field("aliases", "Also known as", ""),
            field("country", "Attributed to", ""),
        ],
    },
    Kind {
        key: "bank",
        category: "Money",
        label: "Bank account",
        icon: "\u{E0B4}", // bank
        fields: &[
            field("bank", "Bank", ""),
            field("iban", "IBAN / number", ""),
            field("holder", "Holder", ""),
            COUNTRY,
        ],
    },
    Kind {
        key: "card",
        category: "Money",
        label: "Payment card",
        icon: "\u{E1D2}", // credit-card
        fields: &[
            field("number", "Card number", ""),
            pick(
                "network",
                "Network",
                "Visa",
                &["Visa", "Mastercard", "American Express", "UnionPay"],
            ),
            field("holder", "Holder", ""),
            field("expiry", "Expiry", ""),
        ],
    },
    Kind {
        key: "transaction",
        category: "Money",
        label: "Transaction",
        icon: "\u{E0A0}", // arrows-left-right
        fields: &[
            field("amount", "Amount", "1,200 EUR"),
            field("date", "Date", ""),
            field("reference", "Reference", ""),
            field("from", "From", ""),
            field("to", "To", ""),
        ],
    },
    Kind {
        key: "payment",
        category: "Money",
        label: "Invoice / receipt",
        icon: "\u{E3EC}", // receipt
        fields: &[
            field("what", "What", "Hotel stay"),
            field("amount", "Amount", ""),
            field("date", "Date", ""),
            field("seller", "Seller", ""),
        ],
    },
    Kind {
        key: "image",
        category: "Media & files",
        label: "Image",
        icon: "\u{E2CA}", // image
        fields: &[
            field("title", "Description", "Profile photo"),
            field("source", "Source URL", ""),
            field("taken", "Taken", ""),
            field("place", "Place seen", ""),
        ],
    },
    Kind {
        key: "video",
        category: "Media & files",
        label: "Video",
        icon: "\u{E4DA}", // video-camera
        fields: &[
            field("title", "Description", ""),
            field("url", "URL", ""),
            field("date", "Date", ""),
            field("place", "Place seen", ""),
        ],
    },
    Kind {
        key: "audio",
        category: "Media & files",
        label: "Audio",
        icon: "\u{E326}", // microphone
        fields: &[
            field("title", "Description", ""),
            field("url", "URL", ""),
            field("date", "Date", ""),
            field("voices", "Voices", ""),
        ],
    },
    Kind {
        key: "file",
        category: "Media & files",
        label: "File",
        icon: "\u{E230}", // file
        fields: &[
            field("name", "File name", ""),
            field("type", "Type", ""),
            field("hash", "Hash", ""),
            field("source", "Source", ""),
        ],
    },
    Kind {
        key: "article",
        category: "Media & files",
        label: "Article",
        icon: "\u{E344}", // newspaper
        fields: &[
            field("title", "Headline", ""),
            field("outlet", "Outlet", ""),
            field("url", "URL", ""),
            field("date", "Date", ""),
        ],
    },
    Kind {
        key: "organisation",
        category: "Organisations",
        label: "Company",
        icon: "\u{E0EE}", // briefcase
        fields: &[
            field("name", "Name", "Acme Logistics Ltd"),
            field("registration", "Registration number", ""),
            field("sector", "Sector", ""),
            COUNTRY,
        ],
    },
    Kind {
        key: "institution",
        category: "Organisations",
        label: "Organisation",
        icon: "\u{E244}", // flag
        fields: &[
            field("name", "Name", ""),
            pick(
                "type",
                "Type",
                "NGO",
                &[
                    "NGO",
                    "Government",
                    "Party",
                    "Religious",
                    "Military",
                    "Media",
                    "School",
                ],
            ),
            COUNTRY,
        ],
    },
    Kind {
        key: "brand",
        category: "Organisations",
        label: "Brand / product",
        icon: "\u{E478}", // tag
        fields: &[field("name", "Name", ""), field("owner", "Owned by", "")],
    },
    Kind {
        key: "shop",
        category: "Organisations",
        label: "Shop / premises",
        icon: "\u{E470}", // storefront
        fields: &[
            field("name", "Name", "Corner café"),
            field("address", "Address", ""),
            field("hours", "Opening hours", ""),
        ],
    },
    Kind {
        key: "court",
        category: "Organisations",
        label: "Court case",
        icon: "\u{EA32}", // gavel
        fields: &[
            field("case", "Case number", ""),
            field("court", "Court", ""),
            field("parties", "Parties", ""),
            field("date", "Date", ""),
        ],
    },
    Kind {
        key: "passive_dns",
        category: "PassiveTotal",
        label: "Passive DNS record",
        icon: "\u{E1A0}", // clock-counter-clockwise
        fields: &[
            field("name", "Name", "example.com"),
            field("resolves", "Resolved to", ""),
            field("first", "First seen", ""),
            field("last", "Last seen", ""),
        ],
    },
    Kind {
        key: "whois",
        category: "PassiveTotal",
        label: "WHOIS record",
        icon: "\u{E6F8}", // address-book
        fields: &[
            field("domain", "Domain", ""),
            field("registrant", "Registrant", ""),
            field("email", "Registrant email", ""),
            field("date", "Record date", ""),
        ],
    },
    Kind {
        key: "tracker",
        category: "PassiveTotal",
        label: "Host tracker",
        icon: "\u{E408}", // share-network
        fields: &[
            field("value", "Tracker value", ""),
            pick(
                "type",
                "Type",
                "",
                &[
                    "Google Analytics",
                    "AdSense",
                    "Facebook Pixel",
                    "Yandex Metrica",
                    "Hotjar",
                ],
            ),
            field("hosts", "Seen on", ""),
        ],
    },
    Kind {
        key: "ix",
        category: "Peering DB",
        label: "Internet exchange",
        icon: "\u{EDDE}", // network
        fields: &[
            field("name", "Name", "DE-CIX Frankfurt"),
            field("city", "City", ""),
            COUNTRY,
        ],
    },
    Kind {
        key: "facility",
        category: "Peering DB",
        label: "Facility / data centre",
        icon: "\u{E760}", // factory
        fields: &[
            field("name", "Name", ""),
            field("operator", "Operator", ""),
            field("address", "Address", ""),
            COUNTRY,
        ],
    },
    Kind {
        key: "operator",
        category: "Peering DB",
        label: "Network operator",
        icon: "\u{E102}", // buildings
        fields: &[
            field("name", "Name", ""),
            field("asn", "ASN", ""),
            field("policy", "Peering policy", ""),
        ],
    },
    Kind {
        key: "vulnerability",
        category: "Penetration testing",
        label: "Vulnerability",
        icon: "\u{E412}", // shield-warning
        fields: &[
            field("cve", "CVE", "CVE-2024-…"),
            field("product", "Affects", ""),
            pick(
                "severity",
                "Severity",
                "",
                &["Critical", "High", "Medium", "Low"],
            ),
        ],
    },
    Kind {
        key: "credential",
        category: "Penetration testing",
        label: "Credential",
        icon: "\u{E752}", // password
        fields: &[
            field("login", "Login", ""),
            field("secret", "Password / hash", ""),
            field("source", "Where it was found", ""),
        ],
    },
    Kind {
        key: "exploit",
        category: "Penetration testing",
        label: "Exploit",
        icon: "\u{E2DE}", // lightning
        fields: &[
            field("name", "Name", ""),
            field("cve", "CVE", ""),
            field("url", "Source", ""),
        ],
    },
    Kind {
        key: "person",
        category: "People",
        label: "Person",
        icon: "\u{E4C2}", // user
        fields: &[
            field("name", "Full name", "Bob Carlson"),
            field("born", "Date of birth", "1989-04-17"),
            field("nationality", "Nationality", "American"),
            pick("gender", "Gender", "", &["Male", "Female", "Other"]),
            field("job", "Occupation", ""),
        ],
    },
    Kind {
        key: "alias",
        category: "People",
        label: "Alias",
        icon: "\u{E9F4}", // mask-happy
        fields: &[
            field("alias", "Alias", "bobby_c"),
            field("used_on", "Used on", "Reddit, forums"),
        ],
    },
    Kind {
        key: "puppet",
        category: "People",
        label: "Sock puppet",
        icon: "\u{E83E}", // detective
        fields: &[
            field("name", "Name", "Persona used"),
            field("platforms", "Platforms", ""),
            field("operator", "Run by", ""),
        ],
    },
    Kind {
        key: "email",
        category: "Personal",
        label: "Email",
        icon: "\u{E214}", // envelope
        fields: &[
            field("address", "Address", "bob@example.com"),
            pick(
                "provider",
                "Provider",
                "Gmail",
                &[
                    "Gmail",
                    "Outlook",
                    "Yahoo",
                    "Proton Mail",
                    "iCloud",
                    "Custom domain",
                ],
            ),
            pick(
                "verified",
                "Checked",
                "",
                &["Exists", "Breached", "Disposable", "Unknown"],
            ),
        ],
    },
    Kind {
        key: "phone",
        category: "Personal",
        label: "Phone number",
        icon: "\u{E3B8}", // phone
        fields: &[
            field("number", "Number", "+1 614 555 0182"),
            pick(
                "type",
                "Type",
                "Mobile",
                &["Mobile", "Landline", "VoIP", "Burner"],
            ),
            field("carrier", "Carrier", ""),
            COUNTRY,
        ],
    },
    Kind {
        key: "document",
        category: "Personal",
        label: "Document",
        icon: "\u{E2C8}", // identification-card
        fields: &[
            field("number", "Number", ""),
            pick(
                "type",
                "Type",
                "Passport",
                &[
                    "Passport",
                    "ID card",
                    "Driver's licence",
                    "Residence permit",
                    "Company record",
                    "Court record",
                ],
            ),
            field("issued", "Issued", ""),
            COUNTRY,
        ],
    },
    Kind {
        key: "phrase",
        category: "Personal",
        label: "Phrase / quote",
        icon: "\u{E660}", // quotes
        fields: &[
            field("text", "Text", ""),
            field("said_by", "Said by", ""),
            field("where", "Where", ""),
        ],
    },
    Kind {
        key: "onion",
        category: "Project Hades",
        label: "Onion site",
        icon: "\u{E62A}", // ghost
        fields: &[
            field("address", "Address", ".onion"),
            field("title", "Title", ""),
            field("seen", "Last seen", ""),
        ],
    },
    Kind {
        key: "market",
        category: "Project Hades",
        label: "Dark web market",
        icon: "\u{E41E}", // shopping-cart
        fields: &[
            field("name", "Name", ""),
            field("vendor", "Vendor", ""),
            field("listing", "Listing", ""),
        ],
    },
    Kind {
        key: "paste",
        category: "Project Hades",
        label: "Paste",
        icon: "\u{E198}", // clipboard-text
        fields: &[
            field("url", "URL", ""),
            pick(
                "site",
                "Site",
                "Pastebin",
                &["Pastebin", "Ghostbin", "Rentry", "Telegra.ph"],
            ),
            field("date", "Date", ""),
        ],
    },
    Kind {
        key: "breach",
        category: "Social links",
        label: "Data breach",
        icon: "\u{E306}", // lock-open
        fields: &[
            field("name", "Breach", "LinkedIn 2021"),
            field("date", "Date", ""),
            field("data", "Data exposed", ""),
        ],
    },
    Kind {
        key: "dating",
        category: "Social links",
        label: "Dating profile",
        icon: "\u{E2A8}", // heart
        fields: &[
            field("name", "Name shown", ""),
            pick(
                "app",
                "App",
                "Tinder",
                &["Tinder", "Bumble", "Hinge", "Badoo", "OkCupid"],
            ),
            field("url", "Link", ""),
        ],
    },
    Kind {
        key: "review",
        category: "Social links",
        label: "Review profile",
        icon: "\u{E46A}", // star
        fields: &[
            field("name", "Name shown", ""),
            pick(
                "site",
                "Site",
                "Google Maps",
                &["Google Maps", "Tripadvisor", "Yelp", "Trustpilot", "Amazon"],
            ),
            field("url", "Link", ""),
        ],
    },
    Kind {
        key: "username",
        category: "Social network",
        label: "Username",
        icon: "\u{E0AC}", // at
        fields: &[
            field("username", "Username", "bcarlson89"),
            pick("platform", "Platform", "Reddit", PLATFORMS),
        ],
    },
    Kind {
        key: "social",
        category: "Social network",
        label: "Social profile",
        icon: "\u{E168}", // chat-circle
        fields: &[
            field("handle", "Handle", "@bcarlson"),
            pick("platform", "Platform", "Instagram", PLATFORMS),
            field("url", "Profile URL", ""),
            field("followers", "Followers", ""),
        ],
    },
    Kind {
        key: "messaging",
        category: "Social network",
        label: "Messaging account",
        icon: "\u{E398}", // paper-plane-tilt
        fields: &[
            field("id", "ID or number", "@bob_c"),
            pick(
                "app",
                "App",
                "Telegram",
                &[
                    "Telegram", "WhatsApp", "Signal", "Discord", "WeChat", "Viber", "Threema",
                    "Session",
                ],
            ),
        ],
    },
    Kind {
        key: "post",
        category: "Social network",
        label: "Post / message",
        icon: "\u{E166}", // chat-centered-text
        fields: &[
            field("text", "Text", "What it says"),
            pick("platform", "Platform", "X (Twitter)", PLATFORMS),
            field("url", "URL", ""),
            field("date", "Posted", ""),
        ],
    },
    Kind {
        key: "hashtag",
        category: "Social network",
        label: "Hashtag",
        icon: "\u{E2A4}", // hash-straight
        fields: &[
            field("tag", "Hashtag", "#protest"),
            pick("platform", "Platform", "", PLATFORMS),
        ],
    },
    Kind {
        key: "channel",
        category: "Social network",
        label: "Channel / forum",
        icon: "\u{E0F2}", // broadcast
        fields: &[
            field("name", "Name", "Local news group"),
            pick(
                "platform",
                "Platform",
                "Telegram",
                &[
                    "Telegram",
                    "Discord",
                    "Reddit",
                    "Forum",
                    "WhatsApp group",
                    "Facebook group",
                ],
            ),
            field("url", "Link", ""),
            field("members", "Members", ""),
        ],
    },
    Kind {
        key: "software",
        category: "Technology",
        label: "Software / app",
        icon: "\u{E5DA}", // app-window
        fields: &[
            field("name", "Name", ""),
            field("version", "Version", ""),
            field("vendor", "Vendor", ""),
        ],
    },
    Kind {
        key: "web_tech",
        category: "Technology",
        label: "Web technology",
        icon: "\u{E1BC}", // code
        fields: &[
            field("name", "Name", "WordPress"),
            field("version", "Version", ""),
            field("site", "Seen on", ""),
        ],
    },
    Kind {
        key: "api_key",
        category: "Technology",
        label: "API key / token",
        icon: "\u{E2D6}", // key
        fields: &[
            field("service", "Service", ""),
            field("value", "Key (part)", ""),
            field("found", "Found in", ""),
        ],
    },
    Kind {
        key: "tracking_id",
        category: "Tracking",
        label: "Tracking ID",
        icon: "\u{E1D6}", // crosshair
        fields: &[
            field("id", "ID", "UA-1234567-1"),
            pick(
                "type",
                "Type",
                "Google Analytics",
                &[
                    "Google Analytics",
                    "Google Tag Manager",
                    "AdSense",
                    "Facebook Pixel",
                    "Yandex Metrica",
                    "Hotjar",
                ],
            ),
            field("sites", "Seen on", ""),
        ],
    },
    Kind {
        key: "fingerprint_id",
        category: "Tracking",
        label: "Browser fingerprint",
        icon: "\u{E240}", // fingerprint-simple
        fields: &[
            field("hash", "Fingerprint", ""),
            field("tool", "Tool", ""),
            field("seen", "Seen on", ""),
        ],
    },
    Kind {
        key: "analytics",
        category: "Tracking",
        label: "Analytics account",
        icon: "\u{E154}", // chart-line
        fields: &[
            field("account", "Account", ""),
            field("owner", "Owner", ""),
            field("sites", "Sites", ""),
        ],
    },
    Kind {
        key: "vehicle",
        category: "Transportation",
        label: "Vehicle",
        icon: "\u{E112}", // car
        fields: &[
            field("model", "Make & model", "Ford Focus"),
            field("plate", "Licence plate", ""),
            field("colour", "Colour", "Silver"),
            field("year", "Year", ""),
            field("vin", "VIN", ""),
        ],
    },
    Kind {
        key: "aircraft",
        category: "Transportation",
        label: "Aircraft",
        icon: "\u{E002}", // airplane
        fields: &[
            field("registration", "Registration", "N123AB"),
            field("model", "Model", ""),
            field("operator", "Operator", ""),
            field("icao", "ICAO 24-bit", ""),
        ],
    },
    Kind {
        key: "flight",
        category: "Transportation",
        label: "Flight",
        icon: "\u{E5D6}", // airplane-tilt
        fields: &[
            field("number", "Flight", "AF1234"),
            field("date", "Date", ""),
            field("from", "From", ""),
            field("to", "To", ""),
        ],
    },
    Kind {
        key: "vessel",
        category: "Transportation",
        label: "Vessel",
        icon: "\u{E786}", // boat
        fields: &[
            field("name", "Name", ""),
            field("imo", "IMO", ""),
            field("mmsi", "MMSI", ""),
            field("flag", "Flag", ""),
        ],
    },
    Kind {
        key: "train",
        category: "Transportation",
        label: "Train",
        icon: "\u{E496}", // train
        fields: &[
            field("number", "Train", "TGV 6201"),
            field("date", "Date", ""),
            field("from", "From", ""),
            field("to", "To", ""),
        ],
    },
    Kind {
        key: "weapon",
        category: "Weapons",
        label: "Weapon",
        icon: "\u{E5BA}", // sword
        fields: &[
            field("name", "Model", "AK-47"),
            pick(
                "type",
                "Type",
                "",
                &["Firearm", "Blade", "Explosive", "Drone", "Launcher"],
            ),
            field("serial", "Serial number", ""),
        ],
    },
    Kind {
        key: "blade",
        category: "Weapons",
        label: "Blade",
        icon: "\u{E636}", // knife
        fields: &[
            field("name", "Description", ""),
            field("seen", "Seen in", ""),
        ],
    },
    Kind {
        key: "explosive",
        category: "Weapons",
        label: "Explosive",
        icon: "\u{EE0A}", // bomb
        fields: &[field("name", "Type", ""), field("seen", "Seen in", "")],
    },
    Kind {
        key: "military",
        category: "Weapons",
        label: "Military equipment",
        icon: "\u{E40A}", // shield
        fields: &[
            field("name", "Equipment", "T-72"),
            field("unit", "Unit", ""),
            field("seen", "Seen at", ""),
        ],
    },
    Kind {
        key: "wiki_article",
        category: "Wikimedia",
        label: "Wikipedia article",
        icon: "\u{E0E6}", // book-open
        fields: &[
            field("title", "Title", ""),
            pick(
                "lang",
                "Language",
                "en",
                &["en", "fr", "de", "es", "ru", "ar"],
            ),
            field("url", "URL", ""),
        ],
    },
    Kind {
        key: "wikidata",
        category: "Wikimedia",
        label: "Wikidata item",
        icon: "\u{E1DE}", // database
        fields: &[field("id", "Item", "Q42"), field("label", "Label", "")],
    },
    Kind {
        key: "wiki_editor",
        category: "Wikimedia",
        label: "Wiki editor",
        icon: "\u{E3B2}", // pencil-line
        fields: &[
            field("name", "Username", ""),
            field("edits", "Edits", ""),
            field("wiki", "Wiki", ""),
        ],
    },
];

pub fn kind(key: &str) -> &'static Kind {
    KINDS.iter().find(|k| k.key == key).unwrap_or(&KINDS[0])
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Node {
    pub id: u64,
    /// `Kind::key`.
    pub kind: String,
    pub values: BTreeMap<String, String>,
    pub notes: String,
    /// Centre on the board.
    pub pos: [f32; 2],
    /// A picture of it (the file's bytes, base64), kept in the graph file so it travels with it.
    pub picture: String,
    /// The picture's original file name.
    pub picture_name: String,
    pub border: LineStyle,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            id: 0,
            kind: KINDS[0].key.to_string(),
            values: BTreeMap::new(),
            notes: String::new(),
            pos: [0.0, 0.0],
            picture: String::new(),
            picture_name: String::new(),
            border: LineStyle::Solid,
        }
    }
}

/// Largest picture an entity takes, so graph files stay easy to share.
pub const MAX_PICTURE_BYTES: usize = 10 * 1024 * 1024;

impl Node {
    /// Sets the entity's picture from a file's bytes.
    pub fn set_picture(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        use base64::Engine;
        if bytes.len() > MAX_PICTURE_BYTES {
            return Err(format!(
                "{name} is {:.0} MB; pictures up to {} MB can be linked",
                bytes.len() as f64 / 1e6,
                MAX_PICTURE_BYTES / 1024 / 1024
            ));
        }
        self.picture = base64::engine::general_purpose::STANDARD.encode(bytes);
        self.picture_name = name.to_string();
        Ok(())
    }

    /// The picture's bytes, if it has one.
    pub fn picture_bytes(&self) -> Option<Vec<u8>> {
        use base64::Engine;
        if self.picture.is_empty() {
            return None;
        }
        base64::engine::general_purpose::STANDARD
            .decode(&self.picture)
            .ok()
    }

    /// What the board shows: its first fact, or the kind's name.
    pub fn title(&self) -> String {
        let kind = kind(&self.kind);
        kind.fields
            .first()
            .and_then(|f| self.values.get(f.key))
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map_or_else(
                || format!("New {}", lower_first(kind.label)),
                str::to_string,
            )
    }

    /// A second line under the title: the next two filled facts.
    pub fn subtitle(&self) -> String {
        kind(&self.kind)
            .fields
            .iter()
            .skip(1)
            .filter_map(|f| self.values.get(f.key).map(|v| v.trim()))
            .filter(|v| !v.is_empty())
            .take(2)
            .collect::<Vec<_>>()
            .join("  ·  ")
    }
}

/// "Crypto wallet" → "crypto wallet", but "IP address" stays.
fn lower_first(label: &str) -> String {
    let mut words = label.splitn(2, ' ');
    let first = words.next().unwrap_or_default();
    let first = if first.chars().skip(1).any(char::is_uppercase) {
        first.to_string()
    } else {
        first.to_lowercase()
    };
    match words.next() {
        Some(rest) => format!("{first} {rest}"),
        None => first,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Link {
    pub from: u64,
    pub to: u64,
    /// "owns", "uses", "lives at"…
    pub label: String,
    pub style: LineStyle,
    pub arrow: Arrow,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Graph {
    /// Its name, shown above the board and offered as the file name.
    pub name: String,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    next_id: u64,
}

/// How entities are arranged automatically, like Maltego's layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// A grid, grouped by kind.
    Block,
    /// Trees: the most linked entity on top, what it links to in rows below.
    Hierarchical,
    /// A ring, grouped by kind.
    Circular,
    /// Linked entities pull together, all push apart, until it settles.
    Organic,
}

/// Space kept between entities by the layouts.
const LAYOUT_GAP: f32 = 40.0;

/// What a graph file says it is, so other JSON is refused.
const FILE_FORMAT: &str = "ridge-graph/1";
/// The extension of graph files.
pub const FILE_EXTENSION: &str = "rgraph";

#[derive(Serialize, Deserialize)]
struct GraphFile {
    format: String,
    graph: Graph,
}

impl Graph {
    /// The graph as saved in a file.
    pub fn to_file(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&GraphFile {
            format: FILE_FORMAT.to_string(),
            graph: self.clone(),
        })
    }

    /// A graph read from a file made by `to_file`.
    pub fn from_file(text: &str) -> Result<Self, String> {
        let file: GraphFile =
            serde_json::from_str(text).map_err(|e| format!("not a Ridge graph: {e}"))?;
        if file.format != FILE_FORMAT {
            return Err(format!("not a Ridge graph (format \"{}\")", file.format));
        }
        Ok(file.graph)
    }

    /// The first free spot from `start`, going right (`columns` per row) then down, where an
    /// entity of about `size` does not cover another.
    pub fn free_spot(&self, start: [f32; 2], size: [f32; 2], columns: usize) -> [f32; 2] {
        let columns = columns.max(1);
        for i in 0..400 {
            let pos = [
                start[0] + (i % columns) as f32 * (size[0] + 30.0),
                start[1] + (i / columns) as f32 * (size[1] + 30.0),
            ];
            let free = self.nodes.iter().all(|n| {
                (n.pos[0] - pos[0]).abs() >= size[0] || (n.pos[1] - pos[1]).abs() >= size[1]
            });
            if free {
                return pos;
            }
        }
        start
    }

    /// Adds an entity of `kind` centred at `pos`; returns its id.
    pub fn add(&mut self, kind: &str, pos: [f32; 2]) -> u64 {
        self.next_id = self
            .next_id
            .max(self.nodes.iter().map(|n| n.id).max().unwrap_or(0))
            + 1;
        let id = self.next_id;
        self.nodes.push(Node {
            id,
            kind: kind.to_string(),
            pos,
            ..Node::default()
        });
        id
    }

    pub fn node(&self, id: u64) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: u64) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Links `from` to `to`, unless they are the same or already linked (either way round).
    pub fn link(&mut self, from: u64, to: u64) -> bool {
        let exists = self
            .links
            .iter()
            .any(|l| (l.from, l.to) == (from, to) || (l.from, l.to) == (to, from));
        if from == to || exists || self.node(from).is_none() || self.node(to).is_none() {
            return false;
        }
        self.links.push(Link {
            from,
            to,
            ..Link::default()
        });
        true
    }

    /// Removes entities `ids` and their links.
    pub fn remove(&mut self, ids: &BTreeSet<u64>) {
        self.nodes.retain(|n| !ids.contains(&n.id));
        self.links
            .retain(|l| !ids.contains(&l.from) && !ids.contains(&l.to));
    }

    /// Arranges entities `ids` (every entity when fewer than two are given) around where they
    /// are now. `sizes` gives each one's width and height.
    pub fn layout(&mut self, ids: &BTreeSet<u64>, how: Layout, sizes: &BTreeMap<u64, [f32; 2]>) {
        let ids: BTreeSet<u64> = if ids.len() >= 2 {
            ids.clone()
        } else {
            self.nodes.iter().map(|n| n.id).collect()
        };
        if ids.len() < 2 {
            return;
        }
        let size = |id: u64| sizes.get(&id).copied().unwrap_or([160.0, 48.0]);
        // the biggest entity sets the cell every layout works with
        let cell = ids.iter().fold([0.0f32, 0.0f32], |c, id| {
            let s = size(*id);
            [c[0].max(s[0]), c[1].max(s[1])]
        });
        let (cw, ch) = (cell[0] + LAYOUT_GAP, cell[1] + LAYOUT_GAP);
        let centre = {
            let pts: Vec<[f32; 2]> = self
                .nodes
                .iter()
                .filter(|n| ids.contains(&n.id))
                .map(|n| n.pos)
                .collect();
            let n = pts.len() as f32;
            [
                pts.iter().map(|p| p[0]).sum::<f32>() / n,
                pts.iter().map(|p| p[1]).sum::<f32>() / n,
            ]
        };
        // grouped by kind, in palette order, so alike entities sit together
        let order = |kind: &str| {
            KINDS
                .iter()
                .position(|k| k.key == kind)
                .unwrap_or(usize::MAX)
        };
        let mut chosen: Vec<(u64, usize)> = self
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .map(|n| (n.id, order(&n.kind)))
            .collect();
        chosen.sort_by_key(|&(id, kind)| (kind, id));
        let ordered: Vec<u64> = chosen.iter().map(|&(id, _)| id).collect();

        let spots: BTreeMap<u64, [f32; 2]> = match how {
            Layout::Block => {
                let columns = (ordered.len() as f32).sqrt().ceil() as usize;
                let rows = ordered.len().div_ceil(columns);
                ordered
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        let (col, row) = ((i % columns) as f32, (i / columns) as f32);
                        let x = centre[0] + (col - (columns as f32 - 1.0) / 2.0) * cw;
                        let y = centre[1] + (row - (rows as f32 - 1.0) / 2.0) * ch;
                        (*id, [x, y])
                    })
                    .collect()
            }
            Layout::Circular => {
                let n = ordered.len() as f32;
                // far enough round that neighbours on the ring do not touch
                let radius = (n * cw / std::f32::consts::TAU).max(cw);
                ordered
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        let a = i as f32 / n * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                        (
                            *id,
                            [centre[0] + radius * a.cos(), centre[1] + radius * a.sin()],
                        )
                    })
                    .collect()
            }
            Layout::Hierarchical => self.hierarchy(&ordered, centre, cw, ch * 1.6),
            Layout::Organic => {
                let mut pos: BTreeMap<u64, [f32; 2]> = self
                    .nodes
                    .iter()
                    .filter(|n| ids.contains(&n.id))
                    .map(|n| (n.id, n.pos))
                    .collect();
                // start from a ring so stacked entities can separate
                for (i, id) in ordered.iter().enumerate() {
                    let a = i as f32 * 2.399; // golden angle
                    let p = pos.get_mut(id).expect("chosen");
                    p[0] = centre[0] + a.cos() * cw * (i as f32).sqrt();
                    p[1] = centre[1] + a.sin() * ch * (i as f32).sqrt();
                }
                for step in 0..300 {
                    let heat = cw * (1.0 - step as f32 / 300.0).max(0.02);
                    self.forces(&mut pos, cw, heat, &BTreeSet::new());
                }
                pos
            }
        };
        for n in self.nodes.iter_mut() {
            if let Some(p) = spots.get(&n.id) {
                n.pos = *p;
            }
        }
        if how == Layout::Organic {
            // the forces leave entities close; make sure none sit on another
            self.separate(&ids, 0, sizes);
        }
    }

    /// One step of the organic forces on every entity, for the live ("interactive") layout;
    /// entities in `pinned` (being dragged) stay where they are. `temperature` (0 to 1) sets how
    /// far an entity may move: high just after a change, falling as the graph settles. Returns
    /// how far things moved.
    pub fn settle_step(
        &mut self,
        sizes: &BTreeMap<u64, [f32; 2]>,
        pinned: &BTreeSet<u64>,
        temperature: f32,
    ) -> f32 {
        let width = sizes.values().map(|s| s[0]).fold(160.0f32, f32::max) + LAYOUT_GAP;
        let mut pos: BTreeMap<u64, [f32; 2]> = self.nodes.iter().map(|n| (n.id, n.pos)).collect();
        let before = pos.clone();
        self.forces(
            &mut pos,
            width,
            width * 0.2 * temperature.clamp(0.0, 1.0),
            pinned,
        );
        let mut moved = 0.0f32;
        for n in self.nodes.iter_mut() {
            if let Some(p) = pos.get(&n.id) {
                let b = before[&n.id];
                moved = moved.max((p[0] - b[0]).abs() + (p[1] - b[1]).abs());
                n.pos = *p;
            }
        }
        moved
    }

    /// One Fruchterman–Reingold step: every pair pushes apart, linked pairs pull together, and
    /// everything is drawn gently to the middle so unlinked entities do not drift off; no entity
    /// moves further than `heat`.
    fn forces(
        &self,
        pos: &mut BTreeMap<u64, [f32; 2]>,
        ideal: f32,
        heat: f32,
        pinned: &BTreeSet<u64>,
    ) {
        let ids: Vec<u64> = pos.keys().copied().collect();
        let mut push: BTreeMap<u64, [f32; 2]> = ids.iter().map(|id| (*id, [0.0, 0.0])).collect();
        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                let (pa, pb) = (pos[a], pos[b]);
                let (mut dx, mut dy) = (pa[0] - pb[0], pa[1] - pb[1]);
                if dx == 0.0 && dy == 0.0 {
                    // the same spot: nudge apart in a direction fixed by their ids
                    dx = ((*a as f32) * 0.7).sin();
                    dy = ((*b as f32) * 0.7).cos();
                }
                let d = (dx * dx + dy * dy).sqrt().max(1.0);
                let f = ideal * ideal / d;
                let (fx, fy) = (dx / d * f, dy / d * f);
                push.get_mut(a).expect("id")[0] += fx;
                push.get_mut(a).expect("id")[1] += fy;
                push.get_mut(b).expect("id")[0] -= fx;
                push.get_mut(b).expect("id")[1] -= fy;
            }
        }
        for l in &self.links {
            let (Some(pa), Some(pb)) = (pos.get(&l.from), pos.get(&l.to)) else {
                continue;
            };
            let (dx, dy) = (pa[0] - pb[0], pa[1] - pb[1]);
            let d = (dx * dx + dy * dy).sqrt().max(1.0);
            let f = d * d / ideal;
            let (fx, fy) = (dx / d * f, dy / d * f);
            push.get_mut(&l.from).expect("id")[0] -= fx;
            push.get_mut(&l.from).expect("id")[1] -= fy;
            push.get_mut(&l.to).expect("id")[0] += fx;
            push.get_mut(&l.to).expect("id")[1] += fy;
        }
        // gravity towards the middle, growing with distance
        let n = ids.len().max(1) as f32;
        let centre = pos
            .values()
            .fold([0.0f32, 0.0f32], |c, p| [c[0] + p[0] / n, c[1] + p[1] / n]);
        for (id, f) in push.iter_mut() {
            let p = pos[id];
            f[0] += (centre[0] - p[0]) * 0.6;
            f[1] += (centre[1] - p[1]) * 0.6;
        }
        for (id, f) in push {
            if pinned.contains(&id) {
                continue;
            }
            let len = (f[0] * f[0] + f[1] * f[1]).sqrt();
            if len > 0.0 {
                let step = len.min(heat);
                let p = pos.get_mut(&id).expect("id");
                p[0] += f[0] / len * step;
                p[1] += f[1] / len * step;
            }
        }
    }

    /// Rows of a tree for each linked group: its most linked entity on top, then what is one
    /// link away, two links away… Groups sit side by side.
    fn hierarchy(
        &self,
        ordered: &[u64],
        centre: [f32; 2],
        cw: f32,
        row_h: f32,
    ) -> BTreeMap<u64, [f32; 2]> {
        let chosen: BTreeSet<u64> = ordered.iter().copied().collect();
        let mut next: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for l in &self.links {
            if chosen.contains(&l.from) && chosen.contains(&l.to) {
                next.entry(l.from).or_default().push(l.to);
                next.entry(l.to).or_default().push(l.from);
            }
        }
        let degree = |id: u64| next.get(&id).map_or(0, Vec::len);
        let mut placed: BTreeSet<u64> = BTreeSet::new();
        // each group as rows of ids
        let mut groups: Vec<Vec<Vec<u64>>> = Vec::new();
        let mut roots: Vec<u64> = ordered.to_vec();
        roots.sort_by_key(|id| (std::cmp::Reverse(degree(*id)), *id));
        for root in roots {
            if placed.contains(&root) {
                continue;
            }
            placed.insert(root);
            let mut rows = vec![vec![root]];
            loop {
                let mut row: Vec<u64> = Vec::new();
                for id in rows.last().expect("a row") {
                    for n in next.get(id).into_iter().flatten() {
                        if placed.insert(*n) {
                            row.push(*n);
                        }
                    }
                }
                if row.is_empty() {
                    break;
                }
                rows.push(row);
            }
            groups.push(rows);
        }
        let widths: Vec<usize> = groups
            .iter()
            .map(|rows| rows.iter().map(Vec::len).max().unwrap_or(1))
            .collect();
        let total: f32 = widths.iter().sum::<usize>() as f32 * cw;
        let tallest = groups.iter().map(Vec::len).max().unwrap_or(1) as f32;
        let mut left = centre[0] - total / 2.0;
        let top = centre[1] - (tallest - 1.0) * row_h / 2.0;
        let mut spots = BTreeMap::new();
        for (rows, width) in groups.iter().zip(widths) {
            let mid = left + width as f32 * cw / 2.0;
            for (r, row) in rows.iter().enumerate() {
                for (i, id) in row.iter().enumerate() {
                    let x = mid + (i as f32 - (row.len() as f32 - 1.0) / 2.0) * cw;
                    spots.insert(*id, [x, top + r as f32 * row_h]);
                }
            }
            left += width as f32 * cw;
        }
        spots
    }

    /// Moves entities `ids` apart along `axis` (0: x, 1: y) where they overlap.
    fn separate(&mut self, ids: &BTreeSet<u64>, axis: usize, sizes: &BTreeMap<u64, [f32; 2]>) {
        const GAP: f32 = 24.0;
        let size = |id: u64| sizes.get(&id).copied().unwrap_or([0.0, 0.0]);
        let other = 1 - axis;
        let mut chosen: Vec<&mut Node> = self
            .nodes
            .iter_mut()
            .filter(|n| ids.contains(&n.id))
            .collect();
        chosen.sort_by(|a, b| a.pos[axis].total_cmp(&b.pos[axis]));
        for i in 1..chosen.len() {
            let (before, here) = (size(chosen[i - 1].id), size(chosen[i].id));
            let prev = chosen[i - 1].pos;
            let side_by_side =
                (chosen[i].pos[other] - prev[other]).abs() < (before[other] + here[other]) / 2.0;
            let clear = prev[axis] + (before[axis] + here[axis]) / 2.0 + GAP;
            if side_by_side && chosen[i].pos[axis] < clear {
                chosen[i].pos[axis] = clear;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[u64]) -> BTreeSet<u64> {
        list.iter().copied().collect()
    }

    #[test]
    fn kinds_are_complete() {
        let keys: BTreeSet<&str> = KINDS.iter().map(|k| k.key).collect();
        assert_eq!(keys.len(), KINDS.len(), "kind keys are unique");
        let icons: BTreeSet<&str> = KINDS.iter().map(|k| k.icon).collect();
        assert_eq!(icons.len(), KINDS.len(), "each kind has its own icon");
        for k in KINDS {
            assert!(!k.fields.is_empty(), "{} has facts", k.key);
            assert!(
                CATEGORIES.contains(&k.category),
                "{} is in a category",
                k.key
            );
        }
        for c in CATEGORIES {
            assert!(KINDS.iter().any(|k| k.category == *c), "{c} has kinds");
        }
        // listed category by category, so the palette needs no sorting
        let order: Vec<usize> = KINDS
            .iter()
            .map(|k| CATEGORIES.iter().position(|c| *c == k.category).unwrap())
            .collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]));
        // the facts asked for
        let building: Vec<&str> = kind("building").fields.iter().map(|f| f.key).collect();
        assert!(["type", "address", "country"]
            .iter()
            .all(|f| building.contains(f)));
        let crypto = kind("crypto")
            .fields
            .iter()
            .find(|f| f.key == "network")
            .unwrap();
        assert!(crypto.presets.contains(&"Ethereum"));
    }

    #[test]
    fn a_category_shares_one_colour() {
        let colours: BTreeSet<[u8; 3]> = CATEGORIES.iter().map(|c| category_colour(c)).collect();
        assert_eq!(
            colours.len(),
            CATEGORIES.len(),
            "categories differ in colour"
        );
        assert_eq!(kind("email").colour(), kind("phone").colour());
        assert_ne!(kind("email").colour(), kind("person").colour());
    }

    #[test]
    fn styles_and_arrows_default_and_load() {
        let mut g = Graph::default();
        let (a, b) = (g.add("person", [0.0, 0.0]), g.add("email", [1.0, 0.0]));
        g.link(a, b);
        assert_eq!(g.links[0].arrow, Arrow::None);
        assert_eq!(g.node(a).unwrap().border, LineStyle::Solid);
        g.links[0].arrow = Arrow::Forward;
        g.links[0].style = LineStyle::LongDashes;
        g.node_mut(a).unwrap().border = LineStyle::Dotted;
        let back = Graph::from_file(&g.to_file().unwrap()).unwrap();
        assert_eq!(back, g);
        assert!(Arrow::Both.to_end() && Arrow::Both.to_start());
        assert!(Arrow::Forward.to_end() && !Arrow::Forward.to_start());
        // a link saved before styles existed loads as a plain line
        let old = r#"{"format":"ridge-graph/1","graph":{"nodes":[],"links":[{"from":1,"to":2,"label":"owns"}]}}"#;
        let old = Graph::from_file(old).unwrap();
        assert_eq!(
            (old.links[0].style, old.links[0].arrow),
            (LineStyle::Solid, Arrow::None)
        );
    }

    #[test]
    fn titles() {
        let mut g = Graph::default();
        let id = g.add("crypto", [0.0, 0.0]);
        assert_eq!(g.node(id).unwrap().title(), "New crypto wallet");
        let ip = g.add("ip", [0.0, 0.0]);
        assert_eq!(g.node(ip).unwrap().title(), "New IP address");
        let n = g.node_mut(id).unwrap();
        n.values.insert("address".into(), "bc1qxyz".into());
        n.values.insert("network".into(), "Bitcoin".into());
        assert_eq!(n.title(), "bc1qxyz");
        assert_eq!(n.subtitle(), "Bitcoin");
    }

    #[test]
    fn links_and_removal() {
        let mut g = Graph::default();
        let (a, b, c) = (
            g.add("person", [0.0, 0.0]),
            g.add("email", [1.0, 0.0]),
            g.add("phone", [2.0, 0.0]),
        );
        assert!(g.link(a, b));
        assert!(!g.link(b, a), "already linked the other way");
        assert!(!g.link(a, a));
        assert!(!g.link(a, 99));
        assert!(g.link(b, c));
        g.remove(&ids(&[b]));
        assert_eq!(g.nodes.len(), 2);
        assert!(g.links.is_empty());
        // ids are never reused
        assert!(g.add("person", [0.0, 0.0]) > c);
    }

    #[test]
    fn new_entities_do_not_cover_others() {
        let mut g = Graph::default();
        let size = [140.0, 50.0];
        let spots: Vec<[f32; 2]> = (0..6)
            .map(|_| {
                let pos = g.free_spot([0.0, 0.0], size, 4);
                g.add("person", pos);
                pos
            })
            .collect();
        for (i, a) in spots.iter().enumerate() {
            for b in &spots[i + 1..] {
                assert!((a[0] - b[0]).abs() >= size[0] || (a[1] - b[1]).abs() >= size[1]);
            }
        }
    }

    /// No two entities in `g` overlap, given each is `size`.
    fn no_overlaps(g: &Graph, size: [f32; 2]) -> bool {
        g.nodes.iter().enumerate().all(|(i, a)| {
            g.nodes[i + 1..].iter().all(|b| {
                (a.pos[0] - b.pos[0]).abs() >= size[0] || (a.pos[1] - b.pos[1]).abs() >= size[1]
            })
        })
    }

    #[test]
    fn layouts_spread_entities_out() {
        let size = [160.0, 48.0];
        for how in [
            Layout::Block,
            Layout::Circular,
            Layout::Hierarchical,
            Layout::Organic,
        ] {
            let mut g = Graph::default();
            // all stacked on one spot to start with
            let ids: Vec<u64> = (0..7)
                .map(|i| g.add(if i % 2 == 0 { "person" } else { "email" }, [5.0, 5.0]))
                .collect();
            g.link(ids[0], ids[1]);
            g.link(ids[0], ids[2]);
            g.link(ids[1], ids[3]);
            let sizes: BTreeMap<u64, [f32; 2]> = ids.iter().map(|id| (*id, size)).collect();
            g.layout(&BTreeSet::new(), how, &sizes);
            assert!(
                no_overlaps(&g, size),
                "{how:?} leaves entities on top of each other"
            );
            assert!(g
                .nodes
                .iter()
                .all(|n| n.pos[0].is_finite() && n.pos[1].is_finite()));
        }
    }

    #[test]
    fn hierarchy_puts_the_most_linked_on_top() {
        let mut g = Graph::default();
        let hub = g.add("person", [0.0, 0.0]);
        let a = g.add("email", [0.0, 0.0]);
        let b = g.add("phone", [0.0, 0.0]);
        let c = g.add("domain", [0.0, 0.0]);
        g.link(hub, a);
        g.link(hub, b);
        g.link(a, c);
        g.layout(&BTreeSet::new(), Layout::Hierarchical, &BTreeMap::new());
        let y = |id| g.node(id).unwrap().pos[1];
        assert!(y(hub) < y(a) && y(a) == y(b) && y(b) < y(c));
    }

    #[test]
    fn only_the_selection_is_laid_out() {
        let mut g = Graph::default();
        let a = g.add("person", [0.0, 0.0]);
        let b = g.add("person", [0.0, 0.0]);
        let still = g.add("person", [900.0, 900.0]);
        g.layout(&ids(&[a, b]), Layout::Block, &BTreeMap::new());
        assert_eq!(g.node(still).unwrap().pos, [900.0, 900.0]);
        assert_ne!(g.node(a).unwrap().pos, g.node(b).unwrap().pos);
    }

    #[test]
    fn settling_moves_linked_entities_closer() {
        let mut g = Graph::default();
        let a = g.add("person", [0.0, 0.0]);
        let b = g.add("email", [3000.0, 0.0]);
        g.link(a, b);
        let before = g.node(b).unwrap().pos[0] - g.node(a).unwrap().pos[0];
        for _ in 0..50 {
            g.settle_step(&BTreeMap::new(), &BTreeSet::new(), 1.0);
        }
        let after = g.node(b).unwrap().pos[0] - g.node(a).unwrap().pos[0];
        assert!(after < before);
        // a pinned entity stays put
        let fixed = g.node(a).unwrap().pos;
        g.settle_step(&BTreeMap::new(), &ids(&[a]), 1.0);
        assert_eq!(g.node(a).unwrap().pos, fixed);
    }

    #[test]
    fn settling_comes_to_rest_without_drifting_off() {
        let mut g = Graph::default();
        // three unlinked entities and a linked pair, all near each other
        for (i, x) in [0.0, 30.0, 60.0, 90.0, 120.0].iter().enumerate() {
            g.add(if i < 3 { "person" } else { "email" }, [*x, 0.0]);
        }
        g.link(4, 5);
        let mut temperature = 1.0f32;
        let mut moved = f32::MAX;
        for _ in 0..600 {
            moved = g.settle_step(&BTreeMap::new(), &BTreeSet::new(), temperature);
            temperature = (temperature * 0.98).max(0.02);
        }
        assert!(moved < 5.0, "still moving {moved} a step");
        // spread out, but not flown away
        assert!(no_overlaps(&g, [160.0, 48.0]));
        let far = g
            .nodes
            .iter()
            .map(|n| n.pos[0].abs().max(n.pos[1].abs()))
            .fold(0.0f32, f32::max);
        assert!(far < 3000.0, "an entity drifted {far} away");
    }

    #[test]
    fn files_round_trip_with_pictures() {
        let mut g = Graph::default();
        let a = g.add("person", [1.0, 2.0]);
        let png = [0x89, b'P', b'N', b'G', 0, 1, 2, 3];
        g.node_mut(a)
            .unwrap()
            .set_picture("face.png", &png)
            .unwrap();
        let text = g.to_file().unwrap();
        let back = Graph::from_file(&text).unwrap();
        assert_eq!(back, g);
        assert_eq!(back.node(a).unwrap().picture_bytes().unwrap(), png);
        assert_eq!(back.node(a).unwrap().picture_name, "face.png");
        // other JSON is refused
        assert!(Graph::from_file(r#"{"format":"x","graph":{}}"#).is_err());
        assert!(Graph::from_file("[1, 2]").is_err());
        // too big a picture is refused
        let big = vec![0u8; MAX_PICTURE_BYTES + 1];
        assert!(g.node_mut(a).unwrap().set_picture("big.jpg", &big).is_err());
    }

    #[test]
    fn ron_round_trip() {
        let mut g = Graph::default();
        let a = g.add("building", [10.0, 20.0]);
        let b = g.add("person", [0.0, 0.0]);
        g.node_mut(a)
            .unwrap()
            .values
            .insert("type".into(), "House".into());
        g.node_mut(a).unwrap().notes = "seen twice".into();
        g.link(b, a);
        g.links[0].label = "lives at".into();
        let graphs = vec![g];
        let text = ron::ser::to_string(&graphs).unwrap();
        let back: Vec<Graph> = ron::from_str(&text).unwrap();
        assert_eq!(back, graphs);
    }
}
