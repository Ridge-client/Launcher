//! The browser built into the "Create persona" screen: a WebView2 webview placed over an area of
//! Ridge's window. Ridge ships for Windows only; a development build elsewhere has no webview and
//! the screen falls back to a list of sites with Open buttons.
//!
//! The webview is a native child window drawn on top of egui, so it is hidden whenever it must not
//! cover anything: another tab, another screen, or a dialog.

use eframe::egui;

#[derive(Default)]
pub(super) struct Browser {
    #[cfg(windows)]
    view: Option<native::View>,
    error: Option<String>,
}

impl Browser {
    /// Whether this platform can embed a browser at all.
    pub(super) const SUPPORTED: bool = cfg!(windows);

    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Shows the browser over `rect`, creating it on first use and loading `url` when it changed.
    pub(super) fn show(
        &mut self,
        frame: &eframe::Frame,
        ctx: &egui::Context,
        rect: egui::Rect,
        url: &str,
    ) {
        #[cfg(windows)]
        {
            if self.view.is_none() && self.error.is_none() {
                match native::View::new(frame, url) {
                    Ok(v) => self.view = Some(v),
                    Err(e) => self.error = Some(e),
                }
            }
            if let Some(view) = &mut self.view {
                view.place(rect, ctx.pixels_per_point());
                view.load(url);
            }
        }
        #[cfg(not(windows))]
        let _ = (frame, ctx, rect, url);
    }

    pub(super) fn hide(&mut self) {
        #[cfg(windows)]
        if let Some(view) = &mut self.view {
            view.hide();
        }
    }

    pub(super) fn reload(&mut self) {
        #[cfg(windows)]
        if let Some(view) = &self.view {
            view.reload();
        }
    }
}

#[cfg(windows)]
mod native {
    use eframe::egui;
    use wry::dpi::{PhysicalPosition, PhysicalSize};
    use wry::{Rect, WebContext, WebView, WebViewBuilder};

    pub(super) struct View {
        webview: WebView,
        // cookies and cache of the built-in browser live in Ridge's data folder
        // (%APPDATA%\Ridge on Windows), which the uninstaller removes
        _context: Box<WebContext>,
        url: String,
        bounds: Option<(i32, i32, u32, u32)>,
        visible: bool,
    }

    impl View {
        pub(super) fn new(frame: &eframe::Frame, url: &str) -> Result<Self, String> {
            let dir = eframe::storage_dir("Ridge").map(|d| d.join("webview"));
            let mut context = Box::new(WebContext::new(dir));
            let webview = WebViewBuilder::new_with_web_context(&mut context)
                .with_url(url)
                .with_visible(false)
                .build_as_child(frame)
                .map_err(|e| format!("could not open the built-in browser: {e}"))?;
            Ok(Self {
                webview,
                _context: context,
                url: url.to_string(),
                bounds: None,
                visible: false,
            })
        }

        /// Moves the webview over `rect` (egui points) and shows it.
        pub(super) fn place(&mut self, rect: egui::Rect, pixels_per_point: f32) {
            let px = |v: f32| (v * pixels_per_point).round();
            let bounds = (
                px(rect.min.x) as i32,
                px(rect.min.y) as i32,
                px(rect.width()).max(1.0) as u32,
                px(rect.height()).max(1.0) as u32,
            );
            if self.bounds != Some(bounds) {
                let _ = self.webview.set_bounds(Rect {
                    position: PhysicalPosition::new(bounds.0, bounds.1).into(),
                    size: PhysicalSize::new(bounds.2, bounds.3).into(),
                });
                self.bounds = Some(bounds);
            }
            if !self.visible {
                let _ = self.webview.set_visible(true);
                self.visible = true;
            }
        }

        pub(super) fn load(&mut self, url: &str) {
            if self.url != url {
                let _ = self.webview.load_url(url);
                self.url = url.to_string();
            }
        }

        pub(super) fn reload(&self) {
            let _ = self.webview.reload();
        }

        pub(super) fn hide(&mut self) {
            if self.visible {
                let _ = self.webview.set_visible(false);
                // give the keyboard back to Ridge's own widgets
                let _ = self.webview.focus_parent();
                self.visible = false;
            }
        }
    }
}
