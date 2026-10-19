//! "Create persona" screen: the identity form on the left, generator sites from the OSINT list in
//! the built-in browser on the right, so values can be copied across.

use eframe::egui;

use super::super::browser::Browser;
use super::super::icons;
use super::{photo_thumbnail, save_persona, RidgeApp};
use crate::catalog::Catalog;
use crate::persona::profile::{is_filled, Group, Part, Record};
use crate::persona::slugify;

/// The catalog category whose sites help invent an identity.
const GENERATOR_CATEGORY: &str = "ID Generator";

/// (name, url) of every generator site, in catalog order.
fn generators(catalog: &Catalog) -> Vec<(String, String)> {
    catalog
        .categories
        .iter()
        .filter(|c| c.name == GENERATOR_CATEGORY)
        .flat_map(|c| c.tools.iter().map(|t| (t.name.clone(), t.url.clone())))
        .collect()
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui, frame: &eframe::Frame) {
    let idle = app.personas.busy.is_none();
    // an edited persona keeps its id (its Docker volume and containers are named after it)
    let editing = app.personas.editing.clone();
    let id = editing
        .clone()
        .unwrap_or_else(|| slugify(&app.personas.form.profile.full_name()));
    let taken = editing.is_none() && app.personas.saved.contains_key(&id);

    // the form sits exactly where the Profile panel is, so the details stay in place once the
    // persona is saved; the generator sites are on the left
    egui::Panel::right("persona_profile")
        .resizable(false)
        .exact_size(super::PROFILE_WIDTH)
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(if editing.is_some() {
                    "Edit persona"
                } else {
                    "Create a persona"
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            idle && !id.is_empty() && !taken,
                            egui::Button::new(
                                egui::RichText::new(icons::with(icons::SAVE, "Save persona"))
                                    .strong(),
                            ),
                        )
                        .on_hover_text("Saves it to your list; start it from there")
                        .on_disabled_hover_text(if taken {
                            "A persona with this name already exists"
                        } else {
                            "Fill in at least a first or last name"
                        })
                        .clicked()
                    {
                        save_persona(app, id.clone());
                    }
                    if ui.button(icons::with(icons::CANCEL, "Cancel")).clicked() {
                        app.personas.creating = false;
                        app.personas.editing = None;
                    }
                });
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("persona_form")
                .auto_shrink(false)
                .show(ui, |ui| form(app, ui, &id));
        });

    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 0,
            right: 10,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.weak(
                "These sites generate names, faces, IDs and more: copy what you need into the \
                 form on the right. They load through your own connection, so turn your VPN on \
                 first.",
            );
            ui.add_space(4.0);
            generator_panel(app, ui, frame);
        });
}

/// Picture extensions the file dialog offers and the thumbnail can read.
pub(super) const PHOTO_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

/// Profile picture: thumbnail, "Choose image…" (native file dialog) and "Remove".
fn photo_picker(app: &mut RidgeApp, ui: &mut egui::Ui) {
    ui.strong("Profile picture");
    ui.horizontal(|ui| {
        let path = app.personas.form.profile.photo_file.clone();
        photo_thumbnail(&mut app.personas.photos, ui, &path, 96.0);
        ui.vertical(|ui| {
            if ui
                .button(icons::with(icons::IMAGE, "Choose image…"))
                .clicked()
            {
                if let Some(file) = rfd::FileDialog::new()
                    .set_title("Choose a profile picture")
                    .add_filter("Images", &PHOTO_EXTENSIONS)
                    .pick_file()
                {
                    app.personas.form.profile.photo_file = file.display().to_string();
                }
            }
            if !path.is_empty() {
                let name = std::path::Path::new(&path)
                    .file_name()
                    .map_or(path.clone(), |n| n.to_string_lossy().into_owned());
                ui.weak(name).on_hover_text(&path);
                if ui.button(icons::with(icons::CANCEL, "Remove")).clicked() {
                    app.personas.form.profile.photo_file.clear();
                }
            }
        });
    });
}

fn form(app: &mut RidgeApp, ui: &mut egui::Ui, id: &str) {
    photo_picker(app, ui);
    ui.add_space(6.0);
    let f = &mut app.personas.form;
    ui.horizontal(|ui| {
        ui.label("Persona ID:");
        if id.is_empty() {
            ui.weak("set from the name");
        } else {
            ui.monospace(id);
        }
    });
    let (parts, extra) = f.profile.parts();
    for part in parts {
        ui.add_space(10.0);
        match part {
            Part::Fields(section, fields) => {
                ui.strong(section);
                egui::Grid::new(section)
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        for field in fields {
                            ui.label(field.label);
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(field.value)
                                        .hint_text(field.hint)
                                        .desired_width(FIELD_W),
                                );
                                if field.repeatable
                                    && ui
                                        .small_button("+")
                                        .on_hover_text(format!(
                                            "Add another {}",
                                            field.label.to_lowercase()
                                        ))
                                        .clicked()
                                {
                                    extra
                                        .entry(field.key.to_string())
                                        .or_default()
                                        .push(String::new());
                                }
                            });
                            ui.end_row();
                            if let Some(values) = extra.get_mut(field.key) {
                                extra_rows(ui, values);
                                if values.is_empty() {
                                    extra.remove(field.key);
                                }
                            }
                        }
                    });
            }
            Part::Records(group, records) => records_form(ui, group, records),
        }
    }

    ui.add_space(10.0);
    ui.strong("Browser");
    egui::Grid::new("persona_browser")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Timezone");
            ui.add(
                egui::TextEdit::singleline(&mut f.tz)
                    .hint_text("e.g. America/New_York")
                    .desired_width(FIELD_W),
            );
            ui.end_row();
        });
    ui.add_space(12.0);
}

/// Width of a form value, leaving room for its + or − button.
const FIELD_W: f32 = 250.0;

/// A field's extra values under it, each with a − to remove it.
fn extra_rows(ui: &mut egui::Ui, values: &mut Vec<String>) {
    let mut remove = None;
    for (i, value) in values.iter_mut().enumerate() {
        ui.label("");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(value).desired_width(FIELD_W));
            if ui.small_button("−").on_hover_text("Remove").clicked() {
                remove = Some(i);
            }
        });
        ui.end_row();
    }
    if let Some(i) = remove {
        values.remove(i);
    }
}

/// A group's records, each in a block that folds away under its title ("Ford Focus · Silver"),
/// then "+ Add vehicle".
fn records_form(ui: &mut egui::Ui, group: &Group, records: &mut Vec<Record>) {
    ui.strong(group.title);
    let mut remove = None;
    for (i, record) in records.iter_mut().enumerate() {
        egui::CollapsingHeader::new(group.record_title(record))
            .id_salt((group.title, i))
            // a new, empty record opens to be filled in
            .default_open(!is_filled(record))
            .show(ui, |ui| {
                egui::Grid::new((group.title, i, "fields"))
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        for field in group.fields {
                            ui.label(field.label);
                            ui.horizontal(|ui| {
                                let value = record.entry(field.key.to_string()).or_default();
                                ui.add(
                                    egui::TextEdit::singleline(value)
                                        .hint_text(field.hint)
                                        .desired_width(FIELD_W),
                                );
                                if !field.presets.is_empty() {
                                    ui.menu_button("▾", |ui| {
                                        for preset in field.presets {
                                            if ui.button(*preset).clicked() {
                                                *value = preset.to_string();
                                                ui.close();
                                            }
                                        }
                                    })
                                    .response
                                    .on_hover_text("Pick one");
                                }
                            });
                            ui.end_row();
                        }
                    });
                if ui
                    .small_button(icons::with(
                        icons::DELETE,
                        &format!("Remove {}", group.item),
                    ))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
    }
    if let Some(i) = remove {
        records.remove(i);
    }
    // the values of empty boxes are not kept
    for record in records.iter_mut() {
        record.retain(|_, v| !v.is_empty());
    }
    if ui.button(format!("+  Add {}", group.item)).clicked() {
        records.push(Record::new());
    }
}

fn host(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .trim_start_matches("www.")
        .to_string()
}

fn generator_panel(app: &mut RidgeApp, ui: &mut egui::Ui, frame: &eframe::Frame) {
    let sites = generators(&app.catalog);
    if sites.is_empty() {
        app.personas.browser.hide();
        ui.weak("No generator sites in the catalog.");
        return;
    }
    // nothing is loaded until a site is picked
    let sel = app.personas.generator.filter(|&i| i < sites.len());
    let current = sel.map(|i| &sites[i]);

    let picker = ui.horizontal(|ui| {
        let list = egui::ComboBox::from_id_salt("generator")
            .width(380.0)
            .height(520.0)
            .selected_text(current.map_or("Choose a site…".to_string(), |(n, u)| {
                format!("{n}  ·  {}", host(u))
            }))
            .show_ui(ui, |ui| {
                for (i, (name, url)) in sites.iter().enumerate() {
                    ui.selectable_value(
                        &mut app.personas.generator,
                        Some(i),
                        format!("{name}  ·  {}", host(url)),
                    );
                }
            });
        if ui
            .add_enabled(
                sel.is_some_and(|i| i > 0),
                egui::Button::new(icons::PREVIOUS),
            )
            .on_hover_text("Previous site")
            .clicked()
        {
            app.personas.generator = sel.map(|i| i - 1);
        }
        if ui
            .add_enabled(
                sel.is_some_and(|i| i + 1 < sites.len()),
                egui::Button::new(icons::NEXT),
            )
            .on_hover_text("Next site")
            .on_disabled_hover_text("Choose a site first")
            .clicked()
        {
            app.personas.generator = sel.map(|i| i + 1);
        }
        if let Some((_, url)) = current {
            if Browser::SUPPORTED
                && ui
                    .button(icons::RETRY)
                    .on_hover_text("Reload the page")
                    .clicked()
            {
                app.personas.browser.reload();
            }
            if ui
                .button(icons::with(icons::OPEN, "Open in my browser"))
                .on_hover_text(url)
                .clicked()
            {
                let _ = open::that_detached(url);
            }
        }
        // the list drops down over the page area; a native webview would hide it
        list.inner.is_some()
    });
    let list_open = picker.inner;
    ui.add_space(4.0);

    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, egui::Sense::hover());
    let p = &mut app.personas;
    let covered = list_open;
    match current {
        Some((_, url)) if Browser::SUPPORTED && p.browser.error().is_none() && !covered => {
            p.browser.show(frame, &app.ctx, rect, url);
        }
        _ => p.browser.hide(),
    }
    if current.is_none() && Browser::SUPPORTED && p.browser.error().is_none() {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        child.add_space(40.0);
        child.vertical_centered(|ui| {
            ui.label(egui::RichText::new(icons::BROWSER).size(40.0).weak());
            ui.label("Choose a site above to open it here.");
            ui.weak("Generators for names, faces, documents and more.");
        });
    }
    if !Browser::SUPPORTED || p.browser.error().is_some() {
        // no built-in browser here: list the sites to open in the system browser
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        if let Some(err) = p.browser.error() {
            child.colored_label(egui::Color32::from_rgb(218, 54, 51), err);
        }
        egui::ScrollArea::vertical()
            .id_salt("generator_list")
            .show(&mut child, |ui| {
                for (name, url) in &sites {
                    ui.horizontal(|ui| {
                        if ui
                            .button(icons::with(icons::OPEN, "Open"))
                            .on_hover_text(url)
                            .clicked()
                        {
                            let _ = open::that_detached(url);
                        }
                        ui.label(egui::RichText::new(name).strong());
                        ui.weak(host(url));
                    });
                }
            });
    }
}
