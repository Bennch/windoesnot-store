use std::sync::mpsc;
use std::time::Duration;

use gtk::prelude::*;
use glib::ControlFlow;

use crate::core::cache::find_logo_path;
use crate::ui::widgets::card;
use crate::ui::AppContext;

#[derive(Clone)]
pub struct HomePage {
    pub root: gtk::Box,
    pub open_search_btn: gtk::Button,
    pub open_updates_btn: gtk::Button,
    pub open_installed_btn: gtk::Button,
    summary_label: gtk::Label,
}

impl HomePage {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("page-root");
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_hexpand(true);
        root.set_vexpand(true);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let logo = logo_image(64);
        let title = gtk::Label::new(Some("WinDoesNot Store"));
        title.add_css_class("title-1");
        title.set_xalign(0.0);
        header.append(&logo);
        header.append(&title);
        root.append(&header);

        let summary_label = gtk::Label::new(Some("Loading package summary..."));
        summary_label.set_xalign(0.0);
        summary_label.set_wrap(true);
        summary_label.add_css_class("dim-label");
        root.append(&summary_label);

        let quick_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        quick_actions.add_css_class("page-controls");
        let open_search_btn = gtk::Button::with_label("Browse Packages");
        let open_updates_btn = gtk::Button::with_label("Open Updates");
        open_updates_btn.add_css_class("suggested-action");
        let open_installed_btn = gtk::Button::with_label("View Installed");
        quick_actions.append(&open_search_btn);
        quick_actions.append(&open_updates_btn);
        quick_actions.append(&open_installed_btn);
        root.append(&quick_actions);

        let category_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        for label in ["Graphics", "Development", "Media", "Games", "Utilities"] {
            let chip = gtk::Label::new(Some(label));
            chip.add_css_class("pill");
            category_row.append(&chip);
        }
        root.append(&category_row);

        let title = gtk::Label::new(Some("Featured"));
        title.add_css_class("title-2");
        title.set_xalign(0.0);
        root.append(&title);

        let featured = gtk::FlowBox::new();
        featured.set_valign(gtk::Align::Start);
        featured.set_min_children_per_line(1);
        featured.set_max_children_per_line(3);
        featured.set_column_spacing(12);
        featured.set_row_spacing(12);
        featured.set_homogeneous(true);
        featured.set_selection_mode(gtk::SelectionMode::None);
        root.append(&featured);

        let popular = gtk::Label::new(Some("Popular"));
        popular.add_css_class("title-2");
        popular.set_xalign(0.0);
        root.append(&popular);

        let popular_grid = gtk::FlowBox::new();
        popular_grid.set_valign(gtk::Align::Start);
        popular_grid.set_min_children_per_line(1);
        popular_grid.set_max_children_per_line(3);
        popular_grid.set_column_spacing(12);
        popular_grid.set_row_spacing(12);
        popular_grid.set_homogeneous(true);
        popular_grid.set_selection_mode(gtk::SelectionMode::None);
        root.append(&popular_grid);

        // Lightweight placeholders so the page doesn't look empty.
        let placeholder = card::build_card(
            &crate::core::models::PackageSummary {
                name: "Discover Apps".to_string(),
                summary: "Search and install applications from repo and AUR."
                    .to_string(),
                version: String::from("-"),
                source: crate::core::models::PackageSource::Repo,
                installed: false,
                origin: None,
            },
            || {},
            || {},
        );
        featured.insert(&placeholder, -1);

        Self {
            root,
            open_search_btn,
            open_updates_btn,
            open_installed_btn,
            summary_label,
        }
    }

    pub fn bind(&self, ctx: AppContext) {
        refresh_summary(self.summary_label.clone(), ctx.clone());
        let summary = self.summary_label.clone();
        glib::timeout_add_local(Duration::from_secs(900), move || {
            refresh_summary(summary.clone(), ctx.clone());
            ControlFlow::Continue
        });
    }
}

fn refresh_summary(summary: gtk::Label, ctx: AppContext) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let pacman_count = ctx.pacman.list_installed().map(|v| v.len()).unwrap_or(0);
        let flatpak_count = ctx.flatpak.list_installed().map(|v| v.len()).unwrap_or(0);
        let _ = tx.send((pacman_count, flatpak_count));
    });

    glib::idle_add_local(move || match rx.try_recv() {
        Ok((pacman_count, flatpak_count)) => {
            summary.set_text(&format!(
                "Installed packages: {pacman_count} (Pacman/AUR), {flatpak_count} Flatpak apps"
            ));
            ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => ControlFlow::Break,
    });
}

fn logo_image(size: i32) -> gtk::Image {
    // Prefer the installed WinDoesNot brand icon; keep upstream's logo as fallback.
    if let Some(display) = gtk::gdk::Display::default() {
        if gtk::IconTheme::for_display(&display).has_icon("windoesnot") {
            let image = gtk::Image::from_icon_name("windoesnot");
            image.set_pixel_size(size);
            return image;
        }
    }
    if let Some(path) = find_logo_path() {
        let image = gtk::Image::from_file(path);
        image.set_pixel_size(size);
        return image;
    }
    let image = gtk::Image::from_icon_name("io.github.ahmoodio.aurora");
    image.set_pixel_size(size);
    image
}
