//! Window that shows an AUR package before it is built: hints, a few numbers
//! from the AUR, and the PKGBUILD or its changes since the installed version.
use std::sync::mpsc;
use std::time::Duration;

use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

use crate::core::aur_review::{self, Change, Level, Review};
use crate::i18n::{t, tf};

pub fn show(parent: &impl IsA<gtk::Window>, name: String) {
    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .default_width(760)
        .default_height(640)
        .title(tf("Check {}", &[&name]))
        .build();
    let header = adw::HeaderBar::new();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_margin_start(16);
    body.set_margin_end(16);
    let loading = gtk::Label::new(Some(t("Loading package information from the AUR …")));
    loading.add_css_class("dim-label");
    body.append(&loading);
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    layout.append(&header);
    layout.append(&body);
    window.set_content(Some(&layout));
    window.present();

    let (tx, rx) = mpsc::channel();
    let lookup = name.clone();
    std::thread::spawn(move || {
        let _ = tx.send(aur_review::review(&lookup).map_err(|err| err.to_string()));
    });
    glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
        Ok(result) => {
            body.remove(&loading);
            match result {
                Ok(review) => fill(&body, &review),
                Err(err) => {
                    let label = gtk::Label::new(Some(&tf("Could not load the package: {}", &[&err])));
                    label.set_wrap(true);
                    label.set_xalign(0.0);
                    body.append(&label);
                }
            }
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
    });
}

fn fill(body: &gtk::Box, review: &Review) {
    let note = gtk::Label::new(Some(t(
        "AUR packages are written by other users and built on this computer. These hints point to things worth a second look; they are no guarantee either way.",
    )));
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.add_css_class("dim-label");
    body.append(&note);

    let meta = &review.meta;
    let maintainer = meta.maintainer.clone().unwrap_or_else(|| t("nobody").to_string());
    let since = glib::DateTime::from_unix_local(meta.first_submitted as i64)
        .ok()
        .and_then(|date| date.format("%d.%m.%Y").ok())
        .map(|date| date.to_string())
        .unwrap_or_default();
    let facts = gtk::Label::new(Some(&tf(
        "Votes: {} · Popularity: {} · Maintainer: {} · In the AUR since {}",
        &[&meta.votes, &format!("{:.2}", meta.popularity), &maintainer, &since],
    )));
    facts.set_wrap(true);
    facts.set_xalign(0.0);
    body.append(&facts);

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    if review.hints.is_empty() {
        list.append(&adw::ActionRow::builder().title(t("No notable findings")).build());
    }
    for hint in &review.hints {
        let row = adw::ActionRow::builder().title(t(hint.text)).subtitle(&hint.detail).subtitle_lines(2).build();
        let icon = gtk::Image::from_icon_name(match hint.level {
            Level::Warning => "dialog-warning-symbolic",
            Level::Note => "dialog-information-symbolic",
        });
        if hint.level == Level::Warning {
            icon.add_css_class("warning");
        }
        row.add_prefix(&icon);
        list.append(&row);
    }
    body.append(&list);

    let heading = gtk::Label::new(Some(if review.previous.is_some() {
        t("Changes since your version")
    } else {
        t("Build instructions (PKGBUILD)")
    }));
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    body.append(&heading);

    let buffer = gtk::TextBuffer::new(None);
    let added = gtk::TextTag::builder().name("added").paragraph_background("rgba(46,160,67,0.25)").build();
    let removed = gtk::TextTag::builder().name("removed").paragraph_background("rgba(248,81,73,0.25)").build();
    buffer.tag_table().add(&added);
    buffer.tag_table().add(&removed);
    let lines: Vec<(Change, String)> = match &review.previous {
        Some(previous) => aur_review::diff(previous, &review.pkgbuild),
        None => review.pkgbuild.lines().map(|line| (Change::Same, line.to_string())).collect(),
    };
    for (change, line) in lines {
        let (prefix, tag) = match change {
            Change::Same => ("  ", None),
            Change::Added => ("+ ", Some(&added)),
            Change::Removed => ("- ", Some(&removed)),
        };
        let mut end = buffer.end_iter();
        let text = format!("{prefix}{line}\n");
        match tag {
            Some(tag) => buffer.insert_with_tags(&mut end, &text, &[tag]),
            None => buffer.insert(&mut end, &text),
        }
    }
    let view = gtk::TextView::with_buffer(&buffer);
    view.set_editable(false);
    view.set_monospace(true);
    view.set_cursor_visible(false);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_min_content_height(240);
    scroller.set_child(Some(&view));
    body.append(&scroller);
}
