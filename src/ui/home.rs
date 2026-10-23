use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use libadwaita as adw;
use adw::prelude::*;
use glib::ControlFlow;
use gtk::prelude::*;

use crate::core::installation::InstalledApps;
use crate::core::models::{PackageSource, PackageSummary};
use crate::i18n::{t, tf};
use crate::ui::home_data::{self, Category, HomeApp};
use crate::ui::widgets::app_icon::app_icon;
use crate::ui::widgets::card;
use crate::ui::{AppContext, UiHandles};

/// Home page in the style of Bazaar/GNOME Software: banner carousel, category
/// tiles and curated picks.
#[derive(Clone)]
pub struct HomePage {
    pub root: gtk::Box,
    pub open_search_btn: gtk::Button,
    pub open_updates_btn: gtk::Button,
    pub open_installed_btn: gtk::Button,
    summary_label: gtk::Label,
    carousel: adw::Carousel,
    categories: gtk::Grid,
    picks: gtk::FlowBox,
    /// Install buttons by app, so installed apps can be marked.
    buttons: Rc<RefCell<Vec<(&'static HomeApp, gtk::Button)>>>,
    installed: Rc<RefCell<Option<InstalledApps>>>,
}

/// The source an app is installed from: its package (repo/AUR) or a Flatpak.
fn installed_via(installed: &InstalledApps, app: &HomeApp) -> Option<PackageSource> {
    installed.find(app.package, Some(app.flathub)).first().map(|installation| installation.source)
}

impl HomePage {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 24);
        content.add_css_class("page-root");
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);

        let title = gtk::Label::new(Some(t("Home")));
        title.add_css_class("title-1");
        title.add_css_class("page-title");
        title.set_xalign(0.0);
        content.append(&title);

        // Carousel with previous/next arrows and page dots below.
        let carousel = adw::Carousel::new();
        carousel.set_hexpand(true);
        carousel.set_height_request(280);
        carousel.set_spacing(12);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&carousel));
        for (icon, align, step) in [("go-previous-symbolic", gtk::Align::Start, -1i32), ("go-next-symbolic", gtk::Align::End, 1)] {
            let arrow = gtk::Button::from_icon_name(icon);
            arrow.add_css_class("banner-arrow");
            arrow.set_halign(align);
            arrow.set_valign(gtk::Align::Center);
            arrow.set_margin_start(16);
            arrow.set_margin_end(16);
            let carousel = carousel.clone();
            arrow.connect_clicked(move |_| step_carousel(&carousel, step));
            overlay.add_overlay(&arrow);
        }
        let dots = adw::CarouselIndicatorDots::new();
        dots.set_carousel(Some(&carousel));
        let banner_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        banner_box.append(&overlay);
        banner_box.append(&dots);
        content.append(&banner_box);

        let categories = gtk::Grid::new();
        categories.set_column_spacing(12);
        categories.set_row_spacing(12);
        categories.set_column_homogeneous(true);
        content.append(&categories);

        let picks_title = gtk::Label::new(Some(t("Our picks")));
        picks_title.add_css_class("title-2");
        picks_title.set_xalign(0.0);
        content.append(&picks_title);
        let picks = card::tile_grid();
        content.append(&picks);

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&content));
        root.append(&scroller);

        // Kept for the window's navigation wiring; the sidebar covers these pages.
        let open_search_btn = gtk::Button::with_label(t("Browse Packages"));
        let open_updates_btn = gtk::Button::with_label(t("Open Updates"));
        let open_installed_btn = gtk::Button::with_label(t("View Installed"));
        let summary_label = gtk::Label::new(None);

        Self {
            root,
            open_search_btn,
            open_updates_btn,
            open_installed_btn,
            summary_label,
            carousel,
            categories,
            picks,
            buttons: Rc::new(RefCell::new(Vec::new())),
            installed: Rc::new(RefCell::new(None)),
        }
    }

    pub fn bind(&self, ctx: AppContext, handles: UiHandles) {
        for (package, css_class) in home_data::BANNERS {
            if let Some(app) = home_data::app(package) {
                self.carousel.append(&self.banner(app, css_class, &ctx, &handles));
            }
        }
        let carousel = self.carousel.clone();
        glib::timeout_add_seconds_local(7, move || {
            step_carousel(&carousel, 1);
            ControlFlow::Continue
        });

        for (index, category) in Category::ALL.into_iter().enumerate() {
            let tile = self.category_tile(category, &ctx, &handles);
            self.categories.attach(&tile, (index % 3) as i32, (index / 3) as i32, 1, 1);
        }

        for package in home_data::PICKS {
            if let Some(app) = home_data::app(package) {
                self.picks.insert(&self.app_tile(app, &ctx, &handles), -1);
            }
        }

        self.refresh_installed(ctx.clone());
        let page = self.clone();
        let ctx_for_refresh = ctx.clone();
        handles.queue.connect_finished(move || page.refresh_installed(ctx_for_refresh.clone()));
        refresh_summary(self.summary_label.clone(), ctx);
    }

    fn banner(&self, app: &'static HomeApp, css_class: &str, ctx: &AppContext, handles: &UiHandles) -> gtk::Widget {
        let banner = gtk::Box::new(gtk::Orientation::Vertical, 12);
        banner.add_css_class("banner");
        banner.add_css_class(css_class);
        banner.set_hexpand(true);
        banner.set_vexpand(true);
        banner.set_valign(gtk::Align::Fill);

        let inner = gtk::Box::new(gtk::Orientation::Vertical, 10);
        inner.set_valign(gtk::Align::Center);
        inner.set_vexpand(true);
        let icon = app_icon(app.package, Some(app.flathub), 112);
        icon.set_halign(gtk::Align::Center);
        let name = gtk::Label::new(Some(app.name));
        name.add_css_class("banner-title");
        let summary = gtk::Label::new(Some(app.summary()));
        summary.add_css_class("banner-summary");
        inner.append(&icon);
        inner.append(&name);
        inner.append(&summary);
        banner.append(&inner);

        let (ctx, handles) = (ctx.clone(), handles.clone());
        let gesture = gtk::GestureClick::new();
        gesture.connect_pressed(move |_, _, _, _| open_details(app, &ctx, &handles));
        banner.add_controller(gesture);
        banner.upcast()
    }

    fn category_tile(&self, category: Category, ctx: &AppContext, handles: &UiHandles) -> gtk::Button {
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_halign(gtk::Align::Center);
        let icon = gtk::Image::from_icon_name(category.icon());
        icon.set_pixel_size(32);
        let label = gtk::Label::new(Some(category.label()));
        label.add_css_class("category-label");
        content.append(&icon);
        content.append(&label);

        let tile = gtk::Button::new();
        tile.add_css_class("category-tile");
        tile.add_css_class(category.css_class());
        tile.set_child(Some(&content));
        let (page, ctx, handles) = (self.clone(), ctx.clone(), handles.clone());
        tile.connect_clicked(move |_| page.show_category(category, &ctx, &handles));
        tile
    }

    fn app_tile(&self, app: &'static HomeApp, ctx: &AppContext, handles: &UiHandles) -> gtk::Box {
        let queue = handles.queue.clone();
        let (ctx_details, handles_details) = (ctx.clone(), handles.clone());
        let (tile, button) = card::build_tile(
            app.package,
            app.name,
            Some(app.flathub),
            app.summary(),
            None,
            move || queue.add_install(app.package.to_string(), PackageSource::Repo, None),
            move || open_details(app, &ctx_details, &handles_details),
        );
        if let Some(installed) = self.installed.borrow().as_ref() {
            card::mark_installed(&button, installed_via(installed, app));
        }
        self.buttons.borrow_mut().push((app, button));
        tile
    }

    fn show_category(&self, category: Category, ctx: &AppContext, handles: &UiHandles) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 24);
        content.add_css_class("page-root");
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let back = gtk::Button::with_label(t("Back"));
        back.set_halign(gtk::Align::Start);
        let nav_view = handles.nav_view.clone();
        back.connect_clicked(move |_| {
            nav_view.pop();
        });
        let title = gtk::Label::new(Some(category.label()));
        title.add_css_class("title-1");
        title.add_css_class("page-title");
        title.set_xalign(0.0);
        let grid = card::tile_grid();
        for app in home_data::APPS.iter().filter(|app| app.category == category) {
            grid.insert(&self.app_tile(app, ctx, handles), -1);
        }
        content.append(&back);
        content.append(&title);
        content.append(&grid);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_child(Some(&content));
        let page = adw::NavigationPage::builder().title(category.label()).child(&scroller).build();
        handles.nav_view.push(&page);
    }

    fn refresh_installed(&self, ctx: AppContext) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(ctx.installed_apps());
        });
        let (buttons, installed) = (self.buttons.clone(), self.installed.clone());
        glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
            Ok(apps) => {
                for (app, button) in buttons.borrow().iter() {
                    card::mark_installed(button, installed_via(&apps, app));
                }
                *installed.borrow_mut() = Some(apps);
                ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => ControlFlow::Break,
        });
    }
}

fn step_carousel(carousel: &adw::Carousel, step: i32) {
    let pages = carousel.n_pages() as i32;
    if pages == 0 {
        return;
    }
    let next = (carousel.position().round() as i32 + step).rem_euclid(pages);
    carousel.scroll_to(&carousel.nth_page(next as u32), true);
}

fn open_details(app: &'static HomeApp, ctx: &AppContext, handles: &UiHandles) {
    crate::ui::details::show_details(
        ctx,
        handles,
        PackageSummary {
            name: app.package.to_string(),
            summary: app.summary().to_string(),
            version: String::new(),
            source: PackageSource::Repo,
            installed: false,
            origin: None,
            installed_via: None,
        },
        Some(app.flathub.to_string()),
    );
}

fn refresh_summary(summary: gtk::Label, ctx: AppContext) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let pacman_count = ctx.pacman.list_installed().map(|v| v.len()).unwrap_or(0);
        let flatpak_count = ctx.flatpak.list_installed().map(|v| v.len()).unwrap_or(0);
        let _ = tx.send((pacman_count, flatpak_count));
    });
    glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
        Ok((pacman_count, flatpak_count)) => {
            summary.set_text(&tf(
                "Installed packages: {} (Pacman/AUR), {} Flatpak apps",
                &[&pacman_count, &flatpak_count],
            ));
            ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => ControlFlow::Break,
    });
}
