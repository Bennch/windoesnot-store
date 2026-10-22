use gtk::prelude::*;
use gtk::pango;
use std::rc::Rc;

use crate::core::models::PackageSummary;
use crate::i18n::{t, tf};

pub fn build_card<F, G>(pkg: &PackageSummary, on_action: F, on_details: G) -> gtk::Box
where
    F: Fn() + 'static,
    G: Fn() + 'static,
{
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.add_css_class("card");
    root.add_css_class("package-card");
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.set_size_request(260, -1);
    root.set_hexpand(false);
    root.set_vexpand(false);
    root.set_halign(gtk::Align::Start);

    let icon = gtk::Image::from_icon_name("application-x-executable");
    icon.set_pixel_size(64);
    icon.set_halign(gtk::Align::Center);

    let name = gtk::Label::new(Some(&pkg.name));
    name.add_css_class("title-4");
    name.set_xalign(0.0);
    name.set_wrap(true);
    name.set_wrap_mode(pango::WrapMode::WordChar);
    name.set_lines(1);
    name.set_ellipsize(pango::EllipsizeMode::End);
    name.set_max_width_chars(28);

    let summary = gtk::Label::new(Some(&pkg.summary));
    summary.add_css_class("dim-label");
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.set_lines(2);
    summary.set_wrap_mode(pango::WrapMode::WordChar);
    summary.set_ellipsize(pango::EllipsizeMode::End);
    summary.set_max_width_chars(36);

    let badge = gtk::Label::new(Some(match pkg.source {
        crate::core::models::PackageSource::Repo => "Pacman",
        crate::core::models::PackageSource::Aur => "AUR",
        crate::core::models::PackageSource::Flatpak => "Flatpak",
    }));
    badge.add_css_class("pill");
    badge.set_xalign(0.0);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    actions.set_halign(gtk::Align::End);

    let button = gtk::Button::with_label(t("Install"));
    button.add_css_class("suggested-action");
    button.connect_clicked(move |_| on_action());
    let details_fn = Rc::new(on_details);
    let details_btn = gtk::Button::with_label(t("Details"));
    details_btn.add_css_class("flat");
    details_btn.connect_clicked({
        let details_fn = details_fn.clone();
        move |_| (details_fn.as_ref())()
    });

    root.append(&icon);
    root.append(&name);
    root.append(&summary);
    root.append(&badge);
    actions.append(&details_btn);
    actions.append(&button);
    root.append(&actions);

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
