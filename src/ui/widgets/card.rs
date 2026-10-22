use gtk::prelude::*;
use gtk::pango;
use std::rc::Rc;

use crate::core::models::PackageSummary;
use crate::i18n::t;
use crate::ui::widgets::app_icon::app_icon;

/// Search result tile for a package.
pub fn build_card<F, G>(pkg: &PackageSummary, on_action: F, on_details: G) -> gtk::Box
where
    F: Fn() + 'static,
    G: Fn() + 'static,
{
    let badge = match pkg.source {
        crate::core::models::PackageSource::Repo => "Pacman",
        crate::core::models::PackageSource::Aur => "AUR",
        crate::core::models::PackageSource::Flatpak => "Flatpak",
    };
    let (tile, _button) = build_tile(&pkg.name, &pkg.name, None, &pkg.summary, Some(badge), on_action, on_details);
    tile
}

/// Wide tile as in Bazaar/GNOME Software: icon, text and action in one row,
/// filling its grid cell so every tile in a row has the same width.
/// Returns the tile and its install button (e.g. to mark installed apps).
pub fn build_tile<F, G>(
    package: &str,
    title: &str,
    flathub_id: Option<&str>,
    summary_text: &str,
    badge_text: Option<&str>,
    on_action: F,
    on_details: G,
) -> (gtk::Box, gtk::Button)
where
    F: Fn() + 'static,
    G: Fn() + 'static,
{
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    root.add_css_class("card");
    root.add_css_class("package-card");
    root.set_hexpand(true);
    root.set_halign(gtk::Align::Fill);
    root.set_vexpand(false);
    root.set_valign(gtk::Align::Fill);

    let icon = app_icon(package, flathub_id, 56);
    icon.set_valign(gtk::Align::Center);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let name = gtk::Label::new(Some(title));
    name.add_css_class("title-4");
    name.set_xalign(0.0);
    name.set_ellipsize(pango::EllipsizeMode::End);
    name.set_width_chars(1);
    name.set_max_width_chars(16);
    name.set_tooltip_text(Some(title));

    let summary = gtk::Label::new(Some(summary_text));
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

    text.append(&name);
    text.append(&summary);
    if let Some(badge_text) = badge_text {
        let badge = gtk::Label::new(Some(badge_text));
        badge.add_css_class("pill");
        badge.set_xalign(0.0);
        badge.set_halign(gtk::Align::Start);
        text.append(&badge);
    }

    let button = gtk::Button::with_label(t("Install"));
    button.add_css_class("suggested-action");
    button.set_valign(gtk::Align::Center);
    button.connect_clicked(move |_| on_action());

    root.append(&icon);
    root.append(&text);
    root.append(&button);

    let details_fn = Rc::new(on_details);
    let gesture = gtk::GestureClick::new();
    let (tile, action) = (root.downgrade(), button.downgrade());
    gesture.connect_pressed(move |_, _, x, y| {
        let (Some(tile), Some(action)) = (tile.upgrade(), action.upgrade()) else { return };
        let on_button = tile
            .pick(x, y, gtk::PickFlags::DEFAULT)
            .is_some_and(|target| target == action.clone().upcast::<gtk::Widget>() || target.is_ancestor(&action));
        if !on_button {
            (details_fn.as_ref())();
        }
    });
    root.add_controller(gesture);

    (root, button)
}

/// Equal-width tile grid used by search and the home page.
pub fn tile_grid() -> gtk::FlowBox {
    let grid = gtk::FlowBox::new();
    grid.set_valign(gtk::Align::Start);
    grid.set_min_children_per_line(1);
    grid.set_max_children_per_line(3);
    grid.set_column_spacing(12);
    grid.set_row_spacing(12);
    grid.set_homogeneous(true);
    grid.set_selection_mode(gtk::SelectionMode::None);
    grid
}
