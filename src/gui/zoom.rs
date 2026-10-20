//! Zooming into a picture shown in a canvas (Annotate, Shadows), with the Graph tab's gestures:
//! Ctrl+scroll or a pinch zooms around the pointer, scrolling and the middle button pan. The
//! left button stays free for drawing.

use eframe::egui::{self, Pos2, Rect, Vec2};

use super::icons;

/// The most a picture is magnified, times its fitted size.
const MAX: f32 = 16.0;

/// How a picture sits in its canvas: 1 is fitted whole, and `pan` moves its centre (screen
/// points) from the canvas's centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Zoom {
    zoom: f32,
    pan: Vec2,
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

impl Zoom {
    /// The picture's place on screen and its scale (screen points per picture pixel).
    pub(super) fn place(&self, area: Rect, size: [f32; 2]) -> (Rect, f32) {
        let fit = ((area.width() - 24.0) / size[0])
            .min((area.height() - 24.0) / size[1])
            .max(0.01);
        let scale = fit * self.zoom;
        let shown = Rect::from_center_size(
            area.center() + self.pan,
            Vec2::new(size[0], size[1]) * scale,
        );
        (shown, scale)
    }

    pub(super) fn fitted(&self) -> bool {
        self.zoom <= 1.0
    }

    /// Zooms by `factor` keeping the picture point under `at` where it is.
    fn zoom_at(&mut self, factor: f32, at: Pos2, area: Rect, size: [f32; 2]) {
        let (shown, scale) = self.place(area, size);
        let under = (at - shown.min) / scale;
        self.zoom = (self.zoom * factor).clamp(1.0, MAX);
        let (_, scale) = self.place(area, size);
        let min = at - under * scale;
        self.pan = min + Vec2::new(size[0], size[1]) * scale / 2.0 - area.center();
        self.keep_in(area, size);
    }

    /// Never lets the picture leave the canvas: fitted it is centred, zoomed in it covers it.
    fn keep_in(&mut self, area: Rect, size: [f32; 2]) {
        if self.fitted() {
            *self = Self::default();
            return;
        }
        let (shown, _) = self.place(area, size);
        let slack = ((shown.size() - area.size()) / 2.0).max(Vec2::ZERO);
        self.pan = self.pan.clamp(-slack, slack);
    }

    /// The pointer's zoom and pan gestures over the canvas `response`.
    pub(super) fn interact(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        area: Rect,
        size: [f32; 2],
    ) {
        if let Some(p) = response.hover_pos() {
            let (zoom_delta, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
            if zoom_delta != 1.0 {
                self.zoom_at(zoom_delta, p, area, size);
            } else if scroll != Vec2::ZERO && !self.fitted() {
                self.pan += scroll;
                self.keep_in(area, size);
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            self.pan += response.drag_delta();
            self.keep_in(area, size);
        }
    }

    /// −, the zoom and +, and Fit: buttons for a toolbar.
    pub(super) fn buttons(&mut self, ui: &mut egui::Ui, area: Rect, size: [f32; 2]) {
        if ui
            .small_button("−")
            .on_hover_text("Zoom out (Ctrl+scroll)")
            .clicked()
        {
            self.zoom_at(1.0 / 1.5, area.center(), area, size);
        }
        ui.weak(format!("{:.0}%", self.zoom * 100.0))
            .on_hover_text("Of the fitted size · Ctrl+scroll or pinch to zoom, scroll or drag with the middle button to move");
        if ui
            .small_button("+")
            .on_hover_text("Zoom in (Ctrl+scroll)")
            .clicked()
        {
            self.zoom_at(1.5, area.center(), area, size);
        }
        if ui
            .add_enabled(!self.fitted(), egui::Button::new(icons::FIT).small())
            .on_hover_text("Fit the whole picture")
            .clicked()
        {
            *self = Self::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        min: Pos2::new(0.0, 0.0),
        max: Pos2::new(424.0, 324.0),
    };
    const SIZE: [f32; 2] = [800.0, 600.0];

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let mut z = Zoom::default();
        let (shown, scale) = z.place(AREA, SIZE);
        assert_eq!(scale, 0.5);
        let at = Pos2::new(100.0, 80.0);
        let under = (at - shown.min) / scale;
        z.zoom_at(2.0, at, AREA, SIZE);
        let (shown, scale) = z.place(AREA, SIZE);
        assert_eq!(scale, 1.0);
        let still = (at - shown.min) / scale;
        assert!((still - under).length() < 0.01, "{still:?} vs {under:?}");
    }

    #[test]
    fn the_picture_stays_in_the_canvas() {
        let mut z = Zoom::default();
        z.zoom_at(0.5, AREA.center(), AREA, SIZE);
        assert_eq!(z, Zoom::default());
        z.zoom_at(4.0, AREA.center(), AREA, SIZE);
        z.pan = Vec2::new(10_000.0, -10_000.0);
        z.keep_in(AREA, SIZE);
        let (shown, _) = z.place(AREA, SIZE);
        assert!(shown.contains_rect(AREA), "{shown:?}");
    }
}
