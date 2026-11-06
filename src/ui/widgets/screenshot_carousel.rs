use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;

use gtk::prelude::*;
use gio;

use crate::core::appstream::AppStreamClient;

const THUMB_WIDTH: i32 = 360;
const THUMB_HEIGHT: i32 = 210;

/// Screenshots as a row of thumbnails that scrolls sideways; a click opens
/// the full image in the default viewer. Hidden when there are none.
#[derive(Clone)]
pub struct ScreenshotCarousel {
    root: gtk::Box,
    strip: gtk::Box,
    pictures: Rc<RefCell<Vec<(gtk::Picture, gtk::Spinner)>>>,
    files: Rc<RefCell<Vec<Option<PathBuf>>>>,
}

impl ScreenshotCarousel {
    pub fn new() -> Self {
        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
        // Fixed height: the pictures' own natural size would otherwise leave a gap.
        scroller.set_propagate_natural_height(false);
        scroller.set_min_content_height(THUMB_HEIGHT + 16);
        scroller.set_max_content_height(THUMB_HEIGHT + 16);
        scroller.set_child(Some(&strip));

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_vexpand(false);
        root.set_valign(gtk::Align::Start);
        scroller.set_vexpand(false);
        scroller.set_valign(gtk::Align::Start);
        strip.set_valign(gtk::Align::Start);
        root.append(&scroller);
        root.set_visible(false);

        Self {
            root,
            strip,
            pictures: Rc::new(RefCell::new(Vec::new())),
            files: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn set_screenshots(&self, urls: Vec<String>) {
        while let Some(child) = self.strip.first_child() {
            self.strip.remove(&child);
        }
        self.pictures.borrow_mut().clear();
        *self.files.borrow_mut() = vec![None; urls.len()];
        self.root.set_visible(!urls.is_empty());
        if urls.is_empty() {
            return;
        }

        for (idx, _) in urls.iter().enumerate() {
            let picture = gtk::Picture::new();
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_can_shrink(true);
            picture.set_size_request(THUMB_WIDTH, THUMB_HEIGHT);

            let spinner = gtk::Spinner::new();
            spinner.set_halign(gtk::Align::Center);
            spinner.set_valign(gtk::Align::Center);
            spinner.start();

            let overlay = gtk::Overlay::new();
            overlay.add_css_class("card");
            overlay.set_size_request(THUMB_WIDTH, THUMB_HEIGHT);
            overlay.set_hexpand(false);
            overlay.set_vexpand(false);
            overlay.set_halign(gtk::Align::Start);
            overlay.set_valign(gtk::Align::Start);
            overlay.set_overflow(gtk::Overflow::Hidden);
            overlay.set_child(Some(&picture));
            overlay.add_overlay(&spinner);
            overlay.set_cursor_from_name(Some("pointer"));

            let files = self.files.clone();
            let click = gtk::GestureClick::new();
            click.connect_released(move |_, _, _, _| {
                if let Some(Some(path)) = files.borrow().get(idx) {
                    let uri = gio::File::for_path(path).uri();
                    let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
                }
            });
            overlay.add_controller(click);

            self.strip.append(&overlay);
            self.pictures.borrow_mut().push((picture, spinner));
        }

        let pictures = self.pictures.clone();
        let files = self.files.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for (idx, url) in urls.iter().enumerate() {
                if let Some(path) = AppStreamClient::ensure_cached(url) {
                    let _ = tx.send((idx, path));
                }
            }
        });

        glib::timeout_add_local(std::time::Duration::from_millis(60), move || loop {
            match rx.try_recv() {
                Ok((idx, path)) => {
                    if let Some(slot) = files.borrow_mut().get_mut(idx) {
                        *slot = Some(path.clone());
                    }
                    update_picture(&pictures, idx, path);
                }
                Err(mpsc::TryRecvError::Empty) => break glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => break glib::ControlFlow::Break,
            }
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
