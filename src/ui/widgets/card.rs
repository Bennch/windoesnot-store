use gtk::prelude::*;
use gtk::pango;
use std::rc::Rc;

use crate::core::models::PackageSummary;
use crate::i18n::t;

pub fn build_card<F, G>(pkg: &PackageSummary, on_action: F, on_details: G) -> gtk::Box
where
    F: Fn() + 'static,
    G: Fn() + 'static,
{
    // Wide tile as in Bazaar/GNOME Software: icon, text and action in one row,
    // filling its grid cell so every tile in a row has the same width.
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    root.add_css_class("card");
    root.add_css_class("package-card");
    root.set_hexpand(true);
    root.set_halign(gtk::Align::Fill);
    root.set_vexpand(false);
    root.set_valign(gtk::Align::Fill);

    let icon = gtk::Image::from_icon_name(&package_icon_name(&pkg.name));
    icon.set_pixel_size(56);
    icon.set_valign(gtk::Align::Center);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let name = gtk::Label::new(Some(&pkg.name));
    name.add_css_class("title-4");
    name.set_xalign(0.0);
    name.set_ellipsize(pango::EllipsizeMode::End);
    name.set_width_chars(1);
    name.set_max_width_chars(16);
    name.set_tooltip_text(Some(&pkg.name));

    let summary = gtk::Label::new(Some(&pkg.summary));
    summary.add_css_class("dim-label");
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.set_lines(2);
    summary.set_wrap_mode(pango::WrapMode::WordChar);
    summary.set_ellipsize(pango::EllipsizeMode::End);
    summary.set_width_chars(1);
    // GtkFlowBox sizes columns by the widest tile's natural width: keep it narrow
    // so two or three tiles fit per row.
    summary.set_max_width_chars(20);

    let badge = gtk::Label::new(Some(match pkg.source {
        crate::core::models::PackageSource::Repo => "Pacman",
        crate::core::models::PackageSource::Aur => "AUR",
        crate::core::models::PackageSource::Flatpak => "Flatpak",
    }));
    badge.add_css_class("pill");
    badge.set_xalign(0.0);
    badge.set_halign(gtk::Align::Start);

    let button = gtk::Button::with_label(t("Install"));
    button.add_css_class("suggested-action");
    button.set_valign(gtk::Align::Center);
    button.connect_clicked(move |_| on_action());
    let details_fn = Rc::new(on_details);

    text.append(&name);
    text.append(&summary);
    text.append(&badge);
    root.append(&icon);
    root.append(&text);
    root.append(&button);

    let gesture = gtk::GestureClick::new();
    gesture.connect_pressed({
        let details_fn = details_fn.clone();
        move |_, _, _, _| {
            (details_fn.as_ref())();
        }
    });
    root.add_controller(gesture);

    root
}

/// Uses the icon theme's own icon when the package name matches one, e.g. installed apps.
fn package_icon_name(package: &str) -> String {
    if let Some(display) = gtk::gdk::Display::default() {
        let theme = gtk::IconTheme::for_display(&display);
        for candidate in [package, package.trim_end_matches("-bin"), package.trim_end_matches("-git")] {
            if theme.has_icon(candidate) {
                return candidate.to_string();
            }
        }
    }
    "application-x-executable".to_string()
}
