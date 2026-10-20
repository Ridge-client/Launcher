//! "Help" page, as the Cairn launcher has one: how each tab works, and where to report a problem.

use eframe::egui;

use super::{caps, card, page_title, primary_button, RidgeApp, NAV_MUTED, NAV_TEXT};

const ISSUES: &str = "https://github.com/Ridge-client/Launcher/issues/new";

/// Each tab: its name and how it works, a line at a time.
const TABS: [(&str, &[&str]); 10] = [
    (
        "OSINT tools",
        &[
            "Search the catalog by name, category or type; Open opens a tool in your browser.",
            "Ridge checks every site at each start: active, guarded (behind a check page) or down. Hide down sites keeps the list short.",
        ],
    ),
    (
        "Personas",
        &[
            "Create persona fills in an identity sheet next to ID generator sites; each persona keeps its history, logins, notes and files.",
            "Start opens the persona's own browser in Docker, after a VPN, timezone and protection check. Docker Desktop is needed; Ridge can install or start it.",
        ],
    ),
    (
        "Graph",
        &[
            "Add entities from the palette (people, accounts, places, money, infrastructure…), link them and arrange them with a layout.",
            "Save keeps the graph in a file (Ctrl+S); Ridge asks before closing one with unsaved changes.",
        ],
    ),
    (
        "Annotate",
        &[
            "Open a picture (or drop one on the window), drag to draw numbered boxes, caption them, and Export PNG for a report.",
            "Ctrl+scroll zooms; scroll or drag with the middle button to move around.",
        ],
    ),
    (
        "Depix",
        &[
            "Recovers text hidden by pixelation. Open the pixelated picture and drag over the pixelated text; Ridge finds the block size.",
            "Open a reference: type the text from Make the reference text in the same editor, font and size as the original, and screenshot it. Recover matches each block to the reference and shows the best fit on the right.",
            "It only works on mosaic-style pixelation (not blur) and the reference must match the original's font, size and colours.",
        ],
    ),
    (
        "Shadows",
        &[
            "Finds when a picture was taken from its shadows. Type the place, then draw on the two images:",
            "Photo: the shadow from an object's foot to its tip, the object from its foot to its top, and the horizon (where lines parallel to the ground meet). Set Photo's view to the lens: 70° for a phone's main camera.",
            "Map: a map or satellite view with north up; drag from where the camera stood toward what it saw.",
            "The candidates list every date and time the sun stood like that; two per year are normal (either side of the June solstice): pick with the season in the picture. Copy puts them in a report.",
        ],
    ),
    (
        "Tracking",
        &[
            "Add files… (or drop one on the map) opens a track: NMEA 0183 (GPS fixes and AIS position reports), GPX, KML/KMZ, or an ADS-B SBS log — a plane, a boat or a car's path.",
            "Each file gets its own colour; the checkbox hides it, Fit centres the map on it, and the basemap tiles are cached so revisiting a place doesn't ask OpenStreetMap again.",
        ],
    ),
    (
        "Overpass",
        &[
            "Asks OpenStreetMap's Overpass API for places and shows them on the map: write a query on the left (or pick one under Examples) and Run.",
            "{{bbox}} in a query stands for the part of the map in view and {{center}} for its middle. Click a result or a marker to see its tags; Export saves them as GeoJSON.",
        ],
    ),
    (
        "Versions",
        &[
            "What you are running and what changed in each release. An orange dot by Versions means a newer Ridge is out: Update to installs it and Ridge opens again.",
        ],
    ),
    (
        "Settings",
        &[
            "What the personas are kept away from (this PC, your home network, each other), and Run checks to prove it.",
        ],
    ),
];

pub(super) fn show(_app: &mut RidgeApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .id_salt("help")
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(28, 0))
                .show(ui, |ui| {
                    page_title(ui, "Help", "How each tab works, and where to ask.");
                    for (name, lines) in TABS {
                        card(ui, |ui| {
                            ui.label(caps(name, 15.0).strong().color(NAV_TEXT));
                            ui.add_space(6.0);
                            for line in lines {
                                ui.label(egui::RichText::new(*line).size(15.0).color(NAV_TEXT));
                                ui.add_space(2.0);
                            }
                        });
                        ui.add_space(12.0);
                    }
                    card(ui, |ui| {
                        ui.label(caps("Something wrong?", 15.0).strong().color(NAV_TEXT));
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                "Tell us what you did and what happened; a screenshot helps.",
                            )
                            .size(15.0)
                            .color(NAV_MUTED),
                        );
                        ui.add_space(10.0);
                        if ui.add(primary_button("Report a problem")).clicked() {
                            let _ = open::that(ISSUES);
                        }
                    });
                    ui.add_space(24.0);
                });
        });
}
