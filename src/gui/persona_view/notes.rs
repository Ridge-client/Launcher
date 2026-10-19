//! A persona's notes, pinned under its Profile: free text and attached files (pictures, videos,
//! recordings, PDFs…). Ridge keeps its own copy of each file, so the notes keep it if the
//! original moves.

use std::path::{Path, PathBuf};

use anyhow::Context;
use eframe::egui;
use serde::{Deserialize, Serialize};

use super::super::icons;
use super::{photo_thumbnail, RidgeApp};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Notes {
    pub(super) text: String,
    /// Ridge's copies of the attached files, in the order they were added.
    pub(super) files: Vec<String>,
}

/// Pictures the thumbnail can show; other files get an icon for their kind.
const PICTURES: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];
const TILE: f32 = 104.0;
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0xe8, 0xa3, 0x5a);

/// Where Ridge keeps persona `id`'s attachments (removed with the persona).
fn files_dir(id: &str) -> Option<PathBuf> {
    eframe::storage_dir("Ridge").map(|d| d.join("notes").join(id))
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or(path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// The icon for a file's kind.
fn kind_icon(path: &str) -> &'static str {
    match extension(path).as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg" | "heic" | "tif" | "tiff" => {
            icons::FILE_IMAGE
        }
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" | "wmv" => icons::FILE_VIDEO,
        "mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac" | "opus" | "wma" => icons::FILE_AUDIO,
        "pdf" => icons::FILE_PDF,
        "txt" | "md" | "csv" | "json" | "log" | "doc" | "docx" | "rtf" | "odt" | "html" => {
            icons::FILE_TEXT
        }
        _ => icons::FILE,
    }
}

/// Copies `src` into the persona's attachment folder.
fn keep_file(id: &str, src: &Path) -> anyhow::Result<String> {
    let dir = files_dir(id).context("no data folder for attachments")?;
    Ok(copy_into(&dir, src)?.display().to_string())
}

/// Copies `src` into `dir` under its own name, or "name (2).ext"… if that is taken.
fn copy_into(dir: &Path, src: &Path) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dir).context("creating the attachment folder")?;
    let stem = src
        .file_stem()
        .map_or("file".into(), |s| s.to_string_lossy().into_owned());
    let ext = src
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut dest = dir.join(format!("{stem}{ext}"));
    let mut n = 2;
    while dest.exists() {
        dest = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    std::fs::copy(src, &dest).with_context(|| format!("copying {}", src.display()))?;
    Ok(dest)
}

/// Deletes Ridge's copies of a deleted persona's attachments (never the originals).
pub(super) fn forget(id: &str) {
    if let Some(dir) = files_dir(id) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Attaches `files` to persona `id`'s notes; says which could not be copied.
fn attach(app: &mut RidgeApp, id: &str, files: Vec<PathBuf>) {
    let mut failed = Vec::new();
    for file in files {
        match keep_file(id, &file) {
            Ok(copy) => {
                if let Some(saved) = app.personas.saved.get_mut(id) {
                    saved.notes.files.push(copy);
                }
            }
            Err(e) => failed.push(format!("{e:#}")),
        }
    }
    if failed.is_empty() {
        app.personas.errors.remove(id);
    } else {
        app.personas.errors.insert(
            id.to_string(),
            ("File not attached".into(), failed.join("\n")),
        );
    }
}

/// The Notes block of persona `id`: its text, then its files.
pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui, id: &str) {
    if !app.personas.saved.contains_key(id) {
        return;
    }
    // files dropped on the window while this persona is shown are attached to it
    let (hovering, dropped) = ui.input(|i| {
        (
            !i.raw.hovered_files.is_empty(),
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect::<Vec<_>>(),
        )
    });
    if !dropped.is_empty() {
        attach(app, id, dropped);
    }

    let count = app
        .personas
        .saved
        .get(id)
        .map_or(0, |s| s.notes.files.len());
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.strong("Notes");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(icons::with(
                    icons::ATTACH,
                    &format!("Attachments ({count})"),
                ))
                .on_hover_text("Pictures, videos, recordings, PDFs or any file")
                .clicked()
            {
                app.personas.attachments_open = Some(id.to_string());
            }
        });
    });
    if hovering {
        ui.colored_label(ACCENT, "Drop to attach to this persona's notes");
    }
    if let Some((short, detail)) = app.personas.errors.get(id) {
        ui.colored_label(egui::Color32::from_rgb(218, 54, 51), short)
            .on_hover_text(detail);
    }

    let Some(saved) = app.personas.saved.get_mut(id) else {
        return;
    };
    // the text box fills the block; it scrolls inside when the text is longer
    let text_h = (ui.available_height() - 8.0).max(80.0);
    egui::ScrollArea::vertical()
        .id_salt(("notes_text", id))
        .max_height(text_h)
        .show(ui, |ui| {
            ui.add_sized(
                [ui.available_width(), text_h],
                egui::TextEdit::multiline(&mut saved.notes.text).hint_text(
                    "Anything about this persona: backstory, logins to set up, what it posted…",
                ),
            );
        });
}

/// The pop-up listing persona `id`'s attachments: add, open and remove them.
pub(super) fn attachments_dialog(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let Some(id) = app.personas.attachments_open.clone() else {
        return;
    };
    let Some(saved) = app.personas.saved.get(&id) else {
        app.personas.attachments_open = None;
        return;
    };
    let name = saved.profile.full_name();
    let files = saved.notes.files.clone();
    let (mut close, mut pick, mut remove) = (false, false, None);
    let modal = egui::Modal::new(egui::Id::unique("persona_attachments")).show(ui.ctx(), |ui| {
        ui.set_width(640.0);
        ui.horizontal(|ui| {
            ui.heading(format!("Attachments · {name}"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(icons::with(icons::CANCEL, "Close")).clicked() {
                    close = true;
                }
                if ui
                    .button(icons::with(icons::ATTACH, "Attach…"))
                    .on_hover_text("Or drop files on the window")
                    .clicked()
                {
                    pick = true;
                }
            });
        });
        ui.weak("Click a file to open it; right-click to remove it.");
        ui.add_space(8.0);
        if files.is_empty() {
            ui.weak("Nothing attached yet: pictures, videos, recordings, PDFs or any file.");
            ui.add_space(8.0);
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(("attachments", &id))
            .max_height(460.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, path) in files.iter().enumerate() {
                        if file_tile(app, ui, path) {
                            remove = Some(i);
                        }
                    }
                });
            });
    });
    if modal.should_close() {
        close = true;
    }
    if pick {
        if let Some(chosen) = rfd::FileDialog::new()
            .set_title("Attach files to the notes")
            .pick_files()
        {
            attach(app, &id, chosen);
        }
    }
    if let Some(i) = remove {
        if let Some(saved) = app.personas.saved.get_mut(&id) {
            let path = saved.notes.files.remove(i);
            // only Ridge's own copy
            if files_dir(&id).is_some_and(|dir| Path::new(&path).starts_with(dir)) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    if close {
        app.personas.attachments_open = None;
    }
}

/// One attachment: thumbnail or icon, its name; a click opens it. Returns whether "Remove" was
/// chosen (right click).
fn file_tile(app: &mut RidgeApp, ui: &mut egui::Ui, path: &str) -> bool {
    let name = file_name(path);
    let mut remove = false;
    let tile = ui
        .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
            ui.set_width(TILE + 8.0);
            ui.vertical_centered(|ui| {
                if PICTURES.contains(&extension(path).as_str()) {
                    photo_thumbnail(&mut app.personas.photos, ui, path, TILE);
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(TILE, TILE), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 8.0, ui.visuals().extreme_bg_color);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        kind_icon(path),
                        egui::FontId::proportional(TILE * 0.5),
                        ui.visuals().text_color(),
                    );
                }
                ui.add(
                    egui::Label::new(egui::RichText::new(&name).small())
                        .truncate()
                        .selectable(false),
                );
            });
        })
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{name}\nClick to open · right-click to remove"));
    if tile.clicked() {
        let _ = open::that_detached(path);
    }
    tile.context_menu(|ui| {
        if ui.button(icons::with(icons::OPEN, "Open")).clicked() {
            let _ = open::that_detached(path);
            ui.close();
        }
        if ui.button(icons::with(icons::DELETE, "Remove")).clicked() {
            remove = true;
            ui.close();
        }
    });
    remove
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_by_kind() {
        assert_eq!(kind_icon("a/b/scan.PDF"), icons::FILE_PDF);
        assert_eq!(kind_icon("clip.mp4"), icons::FILE_VIDEO);
        assert_eq!(kind_icon("call.m4a"), icons::FILE_AUDIO);
        assert_eq!(kind_icon("face.jpg"), icons::FILE_IMAGE);
        assert_eq!(kind_icon("notes.txt"), icons::FILE_TEXT);
        assert_eq!(kind_icon("archive.zip"), icons::FILE);
        assert_eq!(kind_icon("no-extension"), icons::FILE);
    }

    #[test]
    fn copies_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("ridge-notes-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("scan.pdf");
        std::fs::write(&src, b"x").unwrap();
        let names: Vec<String> = (0..3)
            .map(|_| {
                let copy = copy_into(&dir.join("copies"), &src).unwrap();
                file_name(&copy.display().to_string())
            })
            .collect();
        assert_eq!(names, ["scan.pdf", "scan (2).pdf", "scan (3).pdf"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
