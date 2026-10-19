//! A persona's identity sheet: everything a believable sock puppet needs written down.
//!
//! Single facts (name, birth, appearance, life) are plain fields. Things a persona can have
//! several of (documents, addresses, phones, accounts, jobs, schools, vehicles, bank cards) are
//! records: each keeps its own fields together, so a VIN stays with its car.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One record of a group: its values by `GroupField::key`.
pub type Record = BTreeMap<String, String>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// Profile picture: Ridge's own copy of the chosen image file.
    pub photo_file: String,
    // identity
    pub first_name: String,
    pub middle_name: String,
    pub last_name: String,
    pub nickname: String,
    pub gender: String,
    pub date_of_birth: String,
    pub place_of_birth: String,
    pub nationality: String,
    pub mother_maiden_name: String,
    // appearance
    pub height: String,
    pub weight: String,
    pub eye_colour: String,
    pub hair_colour: String,
    pub blood_type: String,
    // life
    pub marital_status: String,
    pub languages: String,
    pub religion: String,
    pub politics: String,
    pub interests: String,
    pub favourite_music: String,
    pub pets: String,
    pub bio: String,
    /// More middle names and nationalities, by field key.
    pub extra: BTreeMap<String, Vec<String>>,
    // records
    pub documents: Vec<Record>,
    pub addresses: Vec<Record>,
    pub phones: Vec<Record>,
    pub accounts: Vec<Record>,
    pub jobs: Vec<Record>,
    pub schools: Vec<Record>,
    pub vehicles: Vec<Record>,
    pub banks: Vec<Record>,
    // fields of sheets saved before records existed: read, moved into records by `migrate`,
    // never saved again
    #[serde(skip_serializing)]
    id_card_number: String,
    #[serde(skip_serializing)]
    passport_number: String,
    #[serde(skip_serializing)]
    social_security_number: String,
    #[serde(skip_serializing)]
    driver_license: String,
    #[serde(skip_serializing)]
    street: String,
    #[serde(skip_serializing)]
    city: String,
    #[serde(skip_serializing)]
    postcode: String,
    #[serde(skip_serializing)]
    state: String,
    #[serde(skip_serializing)]
    country: String,
    #[serde(skip_serializing)]
    phone: String,
    #[serde(skip_serializing)]
    email: String,
    #[serde(skip_serializing)]
    username: String,
    #[serde(skip_serializing)]
    password: String,
    #[serde(skip_serializing)]
    website: String,
    #[serde(skip_serializing)]
    occupation: String,
    #[serde(skip_serializing)]
    employer: String,
    #[serde(skip_serializing)]
    education: String,
    #[serde(skip_serializing)]
    car_make_model: String,
    #[serde(skip_serializing)]
    car_colour: String,
    #[serde(skip_serializing)]
    car_year: String,
    #[serde(skip_serializing)]
    licence_plate: String,
    #[serde(skip_serializing)]
    vin: String,
    #[serde(skip_serializing)]
    bank: String,
    #[serde(skip_serializing)]
    card_number: String,
}

/// One form field: its label, an example, and the value.
pub struct Field<'a> {
    /// Its name in the sheet, where its extra values are kept.
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub value: &'a mut String,
    /// Takes several values (the form shows a + to add one).
    pub repeatable: bool,
}

/// A kind of record: "Vehicles", made of make & model, colour, year…
pub struct Group {
    pub title: &'static str,
    /// What one record is called: "vehicle" ("Add vehicle").
    pub item: &'static str,
    pub fields: &'static [GroupField],
}

pub struct GroupField {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    /// Values to pick from a menu next to the box (it still takes any text).
    pub presets: &'static [&'static str],
    /// Part of the record's title in its collapsed header.
    pub in_title: bool,
}

/// A part of the sheet, in display order.
pub enum Part<'a> {
    Fields(&'static str, Vec<Field<'a>>),
    Records(&'static Group, &'a mut Vec<Record>),
}

const fn gf(key: &'static str, label: &'static str, hint: &'static str) -> GroupField {
    GroupField {
        key,
        label,
        hint,
        presets: &[],
        in_title: false,
    }
}

/// A field that names the record.
const fn title(key: &'static str, label: &'static str, hint: &'static str) -> GroupField {
    GroupField {
        in_title: true,
        ..gf(key, label, hint)
    }
}

/// A field that names the record, with values to pick from.
const fn kind(
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    presets: &'static [&'static str],
) -> GroupField {
    GroupField {
        presets,
        ..title(key, label, hint)
    }
}

pub const DOCUMENTS: Group = Group {
    title: "Documents",
    item: "document",
    fields: &[
        kind(
            "type",
            "Type",
            "Passport",
            &[
                "ID card",
                "Passport",
                "Driver's licence",
                "Social security number",
                "Residence permit",
                "Tax number",
            ],
        ),
        title("number", "Number", ""),
        gf("country", "Issuing country", ""),
        gf("issued", "Issued", "2019-06-01"),
        gf("expires", "Expires", "2029-06-01"),
    ],
};

pub const ADDRESSES: Group = Group {
    title: "Addresses",
    item: "address",
    fields: &[
        kind(
            "label",
            "Label",
            "Home",
            &["Home", "Work", "Previous", "Mailing"],
        ),
        title("street", "Street", "1234 Elm Street"),
        title("city", "City", "Columbus"),
        gf("postcode", "Postcode", "43004"),
        gf("state", "State / region", "Ohio"),
        gf("country", "Country", "United States"),
    ],
};

pub const PHONES: Group = Group {
    title: "Phones",
    item: "phone",
    fields: &[
        kind(
            "label",
            "Label",
            "Mobile",
            &["Mobile", "Home", "Work", "Burner"],
        ),
        title("number", "Number", "+1 614 555 0182"),
        gf("carrier", "Carrier", ""),
    ],
};

pub const ACCOUNTS: Group = Group {
    title: "Accounts",
    item: "account",
    fields: &[
        kind(
            "site",
            "Site or app",
            "Instagram",
            &[
                "Gmail",
                "Outlook",
                "Proton Mail",
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
                "Website",
            ],
        ),
        title("username", "Username", "bcarlson89"),
        gf("email", "Email", "bob.carlson@example.com"),
        gf("password", "Password", ""),
        gf("phone", "Phone used", ""),
        gf("url", "Profile URL", ""),
    ],
};

pub const JOBS: Group = Group {
    title: "Work",
    item: "job",
    fields: &[
        title("title", "Job", "Logistics coordinator"),
        title("employer", "Employer", ""),
        gf("city", "City", ""),
        gf("from", "From", "2018"),
        gf("to", "To", "now"),
    ],
};

pub const SCHOOLS: Group = Group {
    title: "Education",
    item: "school",
    fields: &[
        title("school", "School", "Ohio State University"),
        title("degree", "Degree / field", "BSc Supply chain"),
        gf("from", "From", "2007"),
        gf("to", "To", "2011"),
    ],
};

pub const VEHICLES: Group = Group {
    title: "Vehicles",
    item: "vehicle",
    fields: &[
        title("make_model", "Make & model", "Ford Focus"),
        title("colour", "Colour", "Silver"),
        gf("year", "Year", "2016"),
        title("plate", "Licence plate", ""),
        gf("vin", "VIN", ""),
    ],
};

pub const BANKS: Group = Group {
    title: "Bank & cards",
    item: "bank account",
    fields: &[
        title("bank", "Bank", ""),
        gf("account", "Account number / IBAN", ""),
        kind(
            "card_type",
            "Card",
            "Visa",
            &["Visa", "Mastercard", "American Express", "Debit card"],
        ),
        gf("card_number", "Card number", ""),
        gf("card_expiry", "Card expiry", "08/28"),
    ],
};

impl Group {
    /// "Ford Focus · Silver · AB-123-CD": the record's filled title fields.
    pub fn record_title(&self, record: &Record) -> String {
        let parts: Vec<&str> = self
            .fields
            .iter()
            .filter(|f| f.in_title)
            .filter_map(|f| record.get(f.key).map(|v| v.trim()))
            .filter(|v| !v.is_empty())
            .collect();
        if parts.is_empty() {
            format!("New {}", self.item)
        } else {
            parts.join("  ·  ")
        }
    }
}

/// Whether a record has anything filled in.
pub fn is_filled(record: &Record) -> bool {
    record.values().any(|v| !v.trim().is_empty())
}

impl Profile {
    /// The sheet's parts in display order, and the extra middle names and nationalities.
    pub fn parts(&mut self) -> (Vec<Part<'_>>, &mut BTreeMap<String, Vec<String>>) {
        // f!(label, hint, field) takes one value; r!(…) takes several
        macro_rules! f {
            ($label:expr, $hint:expr, $field:ident) => {
                Field {
                    key: stringify!($field),
                    label: $label,
                    hint: $hint,
                    value: &mut self.$field,
                    repeatable: false,
                }
            };
        }
        macro_rules! r {
            ($label:expr, $hint:expr, $field:ident) => {
                Field {
                    repeatable: true,
                    ..f!($label, $hint, $field)
                }
            };
        }
        let parts = vec![
            Part::Fields(
                "Identity",
                vec![
                    f!("First name", "Bob", first_name),
                    r!("Middle name", "James", middle_name),
                    f!("Last name", "Carlson", last_name),
                    f!("Nickname", "Bobby", nickname),
                    f!("Gender", "Male", gender),
                    f!("Date of birth", "1989-04-17", date_of_birth),
                    f!("Place of birth", "Dayton, Ohio", place_of_birth),
                    r!("Nationality", "American", nationality),
                    f!("Mother's maiden name", "Hughes", mother_maiden_name),
                ],
            ),
            Part::Records(&DOCUMENTS, &mut self.documents),
            Part::Records(&ADDRESSES, &mut self.addresses),
            Part::Records(&PHONES, &mut self.phones),
            Part::Records(&ACCOUNTS, &mut self.accounts),
            Part::Records(&JOBS, &mut self.jobs),
            Part::Records(&SCHOOLS, &mut self.schools),
            Part::Fields(
                "Appearance",
                vec![
                    f!("Height", "180 cm", height),
                    f!("Weight", "78 kg", weight),
                    f!("Eye colour", "Brown", eye_colour),
                    f!("Hair colour", "Dark blond", hair_colour),
                    f!("Blood type", "O+", blood_type),
                ],
            ),
            Part::Records(&VEHICLES, &mut self.vehicles),
            Part::Records(&BANKS, &mut self.banks),
            Part::Fields(
                "Life",
                vec![
                    f!("Marital status", "Single", marital_status),
                    f!("Languages", "English, Spanish", languages),
                    f!("Religion", "", religion),
                    f!("Politics", "", politics),
                    f!("Interests", "Fishing, college football", interests),
                    f!("Favourite music", "", favourite_music),
                    f!("Pets", "", pets),
                    f!("Bio", "", bio),
                ],
            ),
        ];
        (parts, &mut self.extra)
    }

    /// The value of field `key` and its extra ones, filled in only.
    pub fn values<'a>(&'a self, key: &str, value: &'a str) -> Vec<&'a str> {
        std::iter::once(value)
            .chain(
                self.extra
                    .get(key)
                    .into_iter()
                    .flatten()
                    .map(String::as_str),
            )
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .collect()
    }

    /// "Bob James Carlson", from whichever name parts are filled in.
    pub fn full_name(&self) -> String {
        let middle = self.extra.get("middle_name").into_iter().flatten();
        std::iter::once(&self.first_name)
            .chain(std::iter::once(&self.middle_name))
            .chain(middle)
            .chain(std::iter::once(&self.last_name))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Moves the fields of a sheet saved before records existed into records (once: they are
    /// not saved again).
    pub fn migrate(&mut self) {
        let old = Legacy {
            id_card_number: std::mem::take(&mut self.id_card_number),
            passport_number: std::mem::take(&mut self.passport_number),
            social_security_number: std::mem::take(&mut self.social_security_number),
            driver_license: std::mem::take(&mut self.driver_license),
            street: std::mem::take(&mut self.street),
            city: std::mem::take(&mut self.city),
            postcode: std::mem::take(&mut self.postcode),
            state: std::mem::take(&mut self.state),
            country: std::mem::take(&mut self.country),
            phone: std::mem::take(&mut self.phone),
            email: std::mem::take(&mut self.email),
            username: std::mem::take(&mut self.username),
            password: std::mem::take(&mut self.password),
            website: std::mem::take(&mut self.website),
            occupation: std::mem::take(&mut self.occupation),
            employer: std::mem::take(&mut self.employer),
            education: std::mem::take(&mut self.education),
            car_make_model: std::mem::take(&mut self.car_make_model),
            car_colour: std::mem::take(&mut self.car_colour),
            car_year: std::mem::take(&mut self.car_year),
            licence_plate: std::mem::take(&mut self.licence_plate),
            vin: std::mem::take(&mut self.vin),
            bank: std::mem::take(&mut self.bank),
            card_number: std::mem::take(&mut self.card_number),
        };
        let mut extra = std::mem::take(&mut self.extra);
        // more values of the old fields, from the version that had a + on each
        let mut take = |key: &str, first: String| -> Vec<String> {
            std::iter::once(first)
                .chain(extra.remove(key).unwrap_or_default())
                .filter(|v| !v.trim().is_empty())
                .collect()
        };
        let add = |list: &mut Vec<Record>, pairs: &[(&str, &str)]| {
            let record: Record = pairs
                .iter()
                .filter(|(_, v)| !v.trim().is_empty())
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            if is_filled(&record) {
                list.push(record);
            }
        };

        for (doc_type, values) in [
            ("ID card", take("id_card_number", old.id_card_number)),
            ("Passport", take("passport_number", old.passport_number)),
            (
                "Social security number",
                take("social_security_number", old.social_security_number),
            ),
            (
                "Driver's licence",
                take("driver_license", old.driver_license),
            ),
        ] {
            for number in values {
                add(
                    &mut self.documents,
                    &[("type", doc_type), ("number", &number)],
                );
            }
        }
        let address = [
            ("street", old.street.as_str()),
            ("city", &old.city),
            ("postcode", &old.postcode),
            ("state", &old.state),
            ("country", &old.country),
        ];
        // a "Home" label alone is not an address
        if address.iter().any(|(_, v)| !v.trim().is_empty()) {
            let mut pairs = vec![("label", "Home")];
            pairs.extend(address);
            add(&mut self.addresses, &pairs);
        }
        for number in take("phone", old.phone) {
            add(&mut self.phones, &[("number", &number)]);
        }
        add(
            &mut self.accounts,
            &[
                ("username", &old.username),
                ("email", &old.email),
                ("password", &old.password),
                ("url", &old.website),
            ],
        );
        add(
            &mut self.jobs,
            &[("title", &old.occupation), ("employer", &old.employer)],
        );
        for school in take("education", old.education) {
            add(&mut self.schools, &[("school", &school)]);
        }
        add(
            &mut self.vehicles,
            &[
                ("make_model", &old.car_make_model),
                ("colour", &old.car_colour),
                ("year", &old.car_year),
                ("plate", &old.licence_plate),
                ("vin", &old.vin),
            ],
        );
        add(
            &mut self.banks,
            &[("bank", &old.bank), ("card_number", &old.card_number)],
        );
        // only middle names and nationalities take several values now
        extra.retain(|key, _| key == "middle_name" || key == "nationality");
        self.extra = extra;
    }
}

/// The fields records replaced.
struct Legacy {
    id_card_number: String,
    passport_number: String,
    social_security_number: String,
    driver_license: String,
    street: String,
    city: String,
    postcode: String,
    state: String,
    country: String,
    phone: String,
    email: String,
    username: String,
    password: String,
    website: String,
    occupation: String,
    employer: String,
    education: String,
    car_make_model: String,
    car_colour: String,
    car_year: String,
    licence_plate: String,
    vin: String,
    bank: String,
    card_number: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_name_skips_empty_parts() {
        let mut p = Profile {
            first_name: " Bob ".into(),
            last_name: "Carlson".into(),
            ..Profile::default()
        };
        assert_eq!(p.full_name(), "Bob Carlson");
        p.middle_name = "James".into();
        p.extra
            .insert("middle_name".into(), vec!["Lee".into(), " ".into()]);
        assert_eq!(p.full_name(), "Bob James Lee Carlson");
        assert_eq!(p.values("middle_name", &p.middle_name), ["James", "Lee"]);
    }

    #[test]
    fn only_names_and_nationality_repeat() {
        let mut p = Profile::default();
        let (parts, _) = p.parts();
        let repeatable: Vec<&str> = parts
            .iter()
            .flat_map(|part| match part {
                Part::Fields(_, fields) => fields
                    .iter()
                    .filter(|f| f.repeatable)
                    .map(|f| f.key)
                    .collect(),
                Part::Records(..) => Vec::new(),
            })
            .collect();
        assert_eq!(repeatable, ["middle_name", "nationality"]);
    }

    #[test]
    fn record_titles() {
        let car: Record = [
            ("make_model", "Ford Focus"),
            ("colour", "Silver"),
            ("vin", "1FA"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(VEHICLES.record_title(&car), "Ford Focus  ·  Silver");
        assert_eq!(VEHICLES.record_title(&Record::new()), "New vehicle");
    }

    /// eframe saves sheets as RON: one saved by the last version (flat fields) must load, and
    /// a migrated one must load back the same.
    #[test]
    fn ron_round_trip() {
        #[derive(Serialize)]
        struct OldSheet {
            first_name: String,
            photo_url: String,
            passport_number: String,
            city: String,
            vin: String,
            extra: BTreeMap<String, Vec<String>>,
        }
        let old = OldSheet {
            first_name: "Bob".into(),
            photo_url: String::new(),
            passport_number: "P1".into(),
            city: "Columbus".into(),
            vin: "1FA".into(),
            extra: BTreeMap::from([("nationality".to_string(), vec!["French".to_string()])]),
        };
        let text = ron::ser::to_string(&old).unwrap();
        let mut p: Profile = ron::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
        p.migrate();
        assert_eq!(p.first_name, "Bob");
        assert_eq!(p.documents[0]["number"], "P1");
        assert_eq!(p.addresses[0]["city"], "Columbus");
        assert_eq!(p.vehicles[0]["vin"], "1FA");

        let text = ron::ser::to_string(&p).unwrap();
        let back: Profile = ron::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
        assert_eq!(back, p);
    }

    #[test]
    fn old_sheets_move_into_records() {
        let mut p: Profile = serde_json::from_str(
            r#"{"first_name":"Bob","photo_url":"https://x","passport_number":"P1",
                "street":"1 Elm St","city":"Columbus","phone":"555","username":"bob89",
                "email":"b@x.com","car_make_model":"Ford Focus","vin":"1FA","bank":"Chase",
                "extra":{"phone":["666"],"nationality":["French"],"religion":["x"]}}"#,
        )
        .unwrap();
        p.migrate();
        assert_eq!(p.first_name, "Bob");
        assert_eq!(p.documents[0]["type"], "Passport");
        assert_eq!(p.documents[0]["number"], "P1");
        assert_eq!(p.addresses[0]["city"], "Columbus");
        let phones: Vec<&str> = p.phones.iter().map(|r| r["number"].as_str()).collect();
        assert_eq!(phones, ["555", "666"]);
        assert_eq!(p.accounts[0]["username"], "bob89");
        assert_eq!(p.vehicles[0]["vin"], "1FA");
        assert_eq!(p.banks[0]["bank"], "Chase");
        assert!(p.jobs.is_empty() && p.schools.is_empty());
        assert_eq!(p.extra.keys().collect::<Vec<_>>(), ["nationality"]);

        // the old fields are gone from what is saved, and a second migrate changes nothing
        let saved = serde_json::to_string(&p).unwrap();
        assert!(!saved.contains("passport_number") && !saved.contains("car_make_model"));
        let before = p.clone();
        p.migrate();
        assert_eq!(p, before);
    }
}
