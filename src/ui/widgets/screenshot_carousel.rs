use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;

use gtk::prelude::*;
use gio;
use libadwaita as adw;

use crate::core::appstream::AppStreamClient;
use crate::i18n::{t, tf};

#[derive(Clone)]
pub struct ScreenshotCarousel {
    root: gtk::Box,
    carousel: adw::Carousel,
    children: Rc<RefCell<Vec<gtk::Widget>>>,
    pictures: Rc<RefCell<Vec<(gtk::Picture, gtk::Spinner)>>>,
}

impl ScreenshotCarousel {
    pub fn new() -> Self {
        let carousel = adw::Carousel::new();
        carousel.set_hexpand(true);
        carousel.set_vexpand(false);

        let dots = adw::CarouselIndicatorDots::new();
        dots.set_carousel(Some(&carousel));

        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.append(&carousel);
        root.append(&dots);

        Self {
            root,
            carousel,
            children: Rc::new(RefCell::new(Vec::new())),
            pictures: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn set_screenshots(&self, urls: Vec<String>) {
        for child in self.children.borrow_mut().drain(..) {
            self.carousel.remove(&child);
        }
        self.pictures.borrow_mut().clear();

        if urls.is_empty() {
            let label = gtk::Label::new(Some(t("No screenshots available")));
            label.add_css_class("dim-label");
            self.carousel.append(&label);
            self.children.borrow_mut().push(label.upcast());
            return;
        }

        for _ in &urls {
            let picture = gtk::Picture::new();
            picture.set_content_fit(gtk::ContentFit::Cover);
            picture.set_can_shrink(true);
            picture.set_size_request(640, 360);

            let spinner = gtk::Spinner::new();
            spinner.set_halign(gtk::Align::Center);
            spinner.set_valign(gtk::Align::Center);
            spinner.start();

            let overlay = gtk::Overlay::new();
            overlay.set_child(Some(&picture));
            overlay.add_overlay(&spinner);
            overlay.set_size_request(640, 360);

            self.carousel.append(&overlay);
            self.children.borrow_mut().push(overlay.upcast());
            self.pictures.borrow_mut().push((picture, spinner));
        }

        let pictures = self.pictures.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for (idx, url) in urls.iter().enumerate() {
                if let Some(path) = AppStreamClient::ensure_cached(url) {
                    let _ = tx.send((idx, path));
                }
            }
        });

        glib::idle_add_local(move || match rx.try_recv() {
            Ok((idx, path)) => {
                update_picture(&pictures, idx, path);
                glib::ControlFlow::Continue
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        });
    }
}

fn update_picture(pictures: &Rc<RefCell<Vec<(gtk::Picture, gtk::Spinner)>>>, idx: usize, path: PathBuf) {
    if let Some((picture, spinner)) = pictures.borrow().get(idx) {
        let file = gio::File::for_path(path);
        picture.set_file(Some(&file));
        spinner.stop();
        spinner.set_visible(false);
    }
}
