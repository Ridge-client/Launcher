//! Phosphor icons (phosphoricons.com, MIT, see assets/Phosphor-LICENSE.txt): the icon font is added as a fallback of the normal
//! font, so an icon can sit inline in any label or button text.

use eframe::egui::{self, FontData, FontFamily, FontInsert, FontPriority, InsertFontFamily};

pub const ERROR: &str = "\u{E4F8}"; // x-circle
pub const WARNING: &str = "\u{E4E2}"; // warning-circle
pub const PREVIOUS: &str = "\u{E138}"; // caret-left
pub const NEXT: &str = "\u{E13A}"; // caret-right
pub const OPEN: &str = "\u{E5DE}"; // arrow-square-out
pub const START: &str = "\u{E3D0}"; // play
pub const STOP: &str = "\u{E46C}"; // stop
pub const DELETE: &str = "\u{E4A6}"; // trash
pub const BROWSER: &str = "\u{E0F4}"; // browser
pub const RETRY: &str = "\u{E094}"; // arrows-clockwise
pub const DOWNLOAD: &str = "\u{E20C}"; // download-simple
pub const NEW_PERSONA: &str = "\u{E4D0}"; // user-plus
pub const CANCEL: &str = "\u{E4F6}"; // x
pub const SEARCH: &str = "\u{E30C}"; // magnifying-glass
pub const CLEAN: &str = "\u{EC54}"; // broom
pub const IMAGE: &str = "\u{E2CA}"; // image
pub const USER: &str = "\u{E4C2}"; // user
pub const EDIT: &str = "\u{E3B4}"; // pencil-simple
pub const SAVE: &str = "\u{E248}"; // floppy-disk
pub const CHECK: &str = "\u{E184}"; // check-circle
pub const ATTACH: &str = "\u{E39C}"; // paperclip
pub const FILE: &str = "\u{E230}"; // file
pub const FILE_TEXT: &str = "\u{E23A}"; // file-text
pub const FILE_PDF: &str = "\u{E702}"; // file-pdf
pub const FILE_AUDIO: &str = "\u{EA20}"; // file-audio
pub const FILE_VIDEO: &str = "\u{EA22}"; // file-video
pub const FILE_IMAGE: &str = "\u{EA24}"; // file-image
pub const FOLDER_OPEN: &str = "\u{E256}"; // folder-open
pub const NEW_FILE: &str = "\u{E236}"; // file-plus
pub const POINTER: &str = "\u{E1DC}"; // cursor
pub const HAND: &str = "\u{E298}"; // hand
pub const LINK: &str = "\u{E2E2}"; // link
pub const LAYOUT_BLOCK: &str = "\u{E296}"; // grid-four
pub const LAYOUT_TREE: &str = "\u{E67C}"; // tree-structure
pub const LAYOUT_CIRCLE: &str = "\u{E602}"; // circle-dashed
pub const LAYOUT_ORGANIC: &str = "\u{EB58}"; // graph
pub const LAYOUT_LIVE: &str = "\u{E5E4}"; // atom
pub const GRID: &str = "\u{E1FC}"; // dots-nine
pub const COPY: &str = "\u{E1CA}"; // copy
pub const FIT: &str = "\u{E0A4}"; // arrows-out-cardinal

pub fn install(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        "phosphor",
        FontData::from_static(include_bytes!("../../assets/Phosphor.ttf")),
        vec![
            InsertFontFamily {
                family: FontFamily::Proportional,
                priority: FontPriority::Lowest,
            },
            InsertFontFamily {
                family: FontFamily::Monospace,
                priority: FontPriority::Lowest,
            },
        ],
    ));
}

/// "<icon>  <text>", for buttons and labels.
pub fn with(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}
