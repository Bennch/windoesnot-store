use gtk::prelude::*;

use crate::core::icons;

/// App icon for a package: the icon theme first (e.g. Win11, installed apps),
/// then the AppStream catalogues, else a letter tile in the accent colour.
pub fn app_icon(package: &str, flathub_id: Option<&str>, size: i32) -> gtk::Widget {
    if let Some(display) = gtk::gdk::Display::default() {
        let theme = gtk::IconTheme::for_display(&display);
        let themed = icons::base_names(package)
            .into_iter()
            .chain(flathub_id.map(str::to_string))
            .find(|name| theme.has_icon(name));
        if let Some(name) = themed {
            let image = gtk::Image::from_icon_name(&name);
            image.set_pixel_size(size);
            return image.upcast();
        }
    }
    if let Some(path) = icons::icon_file(package, flathub_id) {
        let image = gtk::Image::from_file(path);
        image.set_pixel_size(size);
        return image.upcast();
    }
    letter_tile(package, size)
}

fn letter_tile(package: &str, size: i32) -> gtk::Widget {
    let letter = package
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    let label = gtk::Label::new(Some(&letter));
    label.add_css_class("letter-icon");
    label.set_size_request(size, size);
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    label.upcast()
}
