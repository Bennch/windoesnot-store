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
    /// Big search field; Enter hands the text to the Search page.
    pub search: gtk::SearchEntry,
    browse: gtk::Box,
    quick: gtk::Box,
    quick_status: gtk::Label,
    quick_results: gtk::FlowBox,
    full_search: gtk::Button,
    popular_title: gtk::Label,
    popular: gtk::FlowBox,
    trending_title: gtk::Label,
    trending: gtk::FlowBox,
    /// Install buttons of Flathub-only tiles, by application id.
    flatpak_buttons: Rc<RefCell<Vec<(String, gtk::Button)>>>,
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

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(t("Search apps and packages")));
        search.add_css_class("home-search");
        search.set_hexpand(true);
        search.set_height_request(52);
        content.append(&search);

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
        let browse = gtk::Box::new(gtk::Orientation::Vertical, 24);
        browse.append(&banner_box);

        let categories = gtk::Grid::new();
        categories.set_column_spacing(12);
        categories.set_row_spacing(12);
        categories.set_column_homogeneous(true);
        browse.append(&categories);

        let section = |text: &str| {
            let label = gtk::Label::new(Some(text));
            label.add_css_class("title-2");
            label.set_xalign(0.0);
            label.set_visible(false);
            label
        };
        let popular_title = section(t("Popular"));
        let popular = card::tile_grid();
        popular.set_visible(false);
        let trending_title = section(t("Trending"));
        let trending = card::tile_grid();
        trending.set_visible(false);
        browse.append(&popular_title);
        browse.append(&popular);
        browse.append(&trending_title);
        browse.append(&trending);

        // Quick search: results appear here while typing and replace the browse area.
        let quick = gtk::Box::new(gtk::Orientation::Vertical, 12);
        quick.set_visible(false);
        let quick_status = gtk::Label::new(None);
        quick_status.set_xalign(0.0);
        quick_status.add_css_class("dim-label");
        let quick_results = card::tile_grid();
        let full_search = gtk::Button::with_label(t("Search properly"));
        full_search.set_halign(gtk::Align::Start);
        full_search.add_css_class("suggested-action");
        quick.append(&quick_status);
        quick.append(&quick_results);
        quick.append(&full_search);
        content.append(&quick);
        content.append(&browse);

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
            search,
            browse,
            quick,
            quick_status,
            quick_results,
            full_search,
            popular_title,
            popular,
            trending_title,
            trending,
            flatpak_buttons: Rc::new(RefCell::new(Vec::new())),
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

        self.load_flathub(&ctx, &handles);
        self.bind_quick_search(&ctx, &handles);

        self.refresh_installed(ctx.clone());
        let page = self.clone();
        let ctx_for_refresh = ctx.clone();
        handles.queue.connect_finished(move || page.refresh_installed(ctx_for_refresh.clone()));
        refresh_summary(self.summary_label.clone(), ctx);
    }

    /// Results while typing; "Search properly" opens the full Search page.
    fn bind_quick_search(&self, ctx: &AppContext, handles: &UiHandles) {
        let search = self.search.clone();
        self.full_search.connect_clicked(move |_| {
            search.emit_by_name::<()>("activate", &[]);
        });

        let page = self.clone();
        let (ctx, handles) = (ctx.clone(), handles.clone());
        let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let generation = Rc::new(std::cell::Cell::new(0u64));
        self.search.connect_search_changed(move |entry| {
            if let Some(id) = debounce.borrow_mut().take() {
                let _ = std::panic::catch_unwind(|| id.remove());
            }
            generation.set(generation.get() + 1);
            let query = entry.text().trim().to_string();
            let searching = !query.is_empty();
            page.browse.set_visible(!searching);
            page.quick.set_visible(searching);
            if !searching {
                clear_grid(&page.quick_results);
                return;
            }
            page.quick_status.set_text(&tf("Searching for \"{}\"...", &[&query]));
            let (page, ctx, handles) = (page.clone(), ctx.clone(), handles.clone());
            let (debounce_inner, generation) = (debounce.clone(), generation.clone());
            let request = generation.get();
            let id = glib::timeout_add_local(Duration::from_millis(300), move || {
                *debounce_inner.borrow_mut() = None;
                let (tx, rx) = mpsc::channel();
                let (ctx_thread, query_thread) = (ctx.clone(), query.clone());
                std::thread::spawn(move || {
                    let _ = tx.send(crate::ui::search_all(&ctx_thread, &query_thread));
                });
                let (page, ctx, handles, generation, query) = (page.clone(), ctx.clone(), handles.clone(), generation.clone(), query.clone());
                glib::timeout_add_local(Duration::from_millis(60), move || match rx.try_recv() {
                    Ok(results) => {
                        if generation.get() == request {
                            page.show_quick_results(&query, results, &ctx, &handles);
                        }
                        ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => ControlFlow::Break,
                });
                ControlFlow::Break
            });
            *debounce.borrow_mut() = Some(id);
        });
    }

    fn show_quick_results(&self, query: &str, results: Vec<PackageSummary>, ctx: &AppContext, handles: &UiHandles) {
        clear_grid(&self.quick_results);
        if results.is_empty() {
            self.quick_status.set_text(&tf("No results for \"{}\".", &[&query]));
            return;
        }
        let shown = results.len().min(8);
        self.quick_status.set_text(&tf("Best {} of {} results", &[&shown, &results.len()]));
        for pkg in results.into_iter().take(8) {
            let queue = handles.queue.clone();
            let (ctx_details, handles_details) = (ctx.clone(), handles.clone());
            let flathub_id = (pkg.source == PackageSource::Flatpak).then(|| pkg.name.clone());
            let (name, source, origin) = (pkg.name.clone(), pkg.source, pkg.origin.clone());
            let (pkg_details, id_details) = (pkg.clone(), flathub_id.clone());
            let badge = match pkg.source {
                PackageSource::Repo => "Pacman",
                PackageSource::Aur => "AUR",
                PackageSource::Flatpak => "Flatpak",
            };
            let (tile, button) = card::build_tile(
                &pkg.name,
                &pkg.name,
                flathub_id.as_deref(),
                &pkg.summary,
                Some(badge),
                move || queue.add_install(name.clone(), source, origin.clone()),
                move || crate::ui::details::show_details(&ctx_details, &handles_details, pkg_details.clone(), id_details.clone()),
            );
            if pkg.installed {
                card::mark_installed(&button, pkg.installed_via.or(Some(pkg.source)));
            }
            self.quick_results.insert(&tile, -1);
        }
    }

    /// Flathub's popular and trending apps, filled in once the network answers.
    /// Apps that the curated list offers from the repositories use that entry.
    fn load_flathub(&self, ctx: &AppContext, handles: &UiHandles) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send((fetch_flathub("popular"), fetch_flathub("trending")));
        });
        let (page, ctx, handles) = (self.clone(), ctx.clone(), handles.clone());
        glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
            Ok((popular, trending)) => {
                page.fill_flathub(&page.popular_title, &page.popular, &popular, &ctx, &handles);
                page.fill_flathub(&page.trending_title, &page.trending, &trending, &ctx, &handles);
                ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => ControlFlow::Break,
        });
    }

    fn fill_flathub(&self, title: &gtk::Label, grid: &gtk::FlowBox, hits: &[FlathubApp], ctx: &AppContext, handles: &UiHandles) {
        for hit in hits {
            if let Some(app) = home_data::APPS.iter().find(|app| app.flathub == hit.id) {
                grid.insert(&self.app_tile(app, ctx, handles), -1);
                continue;
            }
            let queue = handles.queue.clone();
            let (ctx_details, handles_details) = (ctx.clone(), handles.clone());
            let (id_install, id_details, summary_text) = (hit.id.clone(), hit.id.clone(), hit.summary.clone());
            let (tile, button) = card::build_tile(
                &hit.id,
                &hit.name,
                Some(&hit.id),
                &hit.summary,
                Some("Flatpak"),
                move || queue.add_install(id_install.clone(), PackageSource::Flatpak, Some("flathub".into())),
                move || {
                    crate::ui::details::show_details(
                        &ctx_details,
                        &handles_details,
                        PackageSummary {
                            name: id_details.clone(),
                            summary: summary_text.clone(),
                            version: String::new(),
                            source: PackageSource::Flatpak,
                            installed: false,
                            origin: Some("flathub".into()),
                            installed_via: None,
                        },
                        Some(id_details.clone()),
                    )
                },
            );
            if let Some(installed) = self.installed.borrow().as_ref() {
                card::mark_installed(&button, installed.find(&hit.id, Some(&hit.id)).first().map(|i| i.source));
            }
            self.flatpak_buttons.borrow_mut().push((hit.id.clone(), button));
            grid.insert(&tile, -1);
        }
        let any = !hits.is_empty();
        title.set_visible(any);
        grid.set_visible(any);
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
        let flatpak_buttons = self.flatpak_buttons.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
            Ok(apps) => {
                for (app, button) in buttons.borrow().iter() {
                    card::mark_installed(button, installed_via(&apps, app));
                }
                for (id, button) in flatpak_buttons.borrow().iter() {
                    card::mark_installed(button, apps.find(id, Some(id)).first().map(|i| i.source));
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

struct FlathubApp {
    id: String,
    name: String,
    summary: String,
}

/// Top apps of a Flathub collection ("popular" or "trending"); empty when offline.
fn fetch_flathub(collection: &str) -> Vec<FlathubApp> {
    let url = format!("https://flathub.org/api/v2/collection/{collection}");
    let Ok(response) = ureq::get(&url).timeout(Duration::from_secs(8)).call() else {
        return Vec::new();
    };
    let Ok(json) = response.into_string().map_err(|_| ()).and_then(|text| serde_json::from_str::<serde_json::Value>(&text).map_err(|_| ())) else {
        return Vec::new();
    };
    json["hits"]
        .as_array()
        .map(|hits| {
            hits.iter()
                .filter(|hit| hit["type"] == "desktop-application")
                .filter_map(|hit| {
                    Some(FlathubApp {
                        id: hit["app_id"].as_str()?.to_string(),
                        name: hit["name"].as_str()?.to_string(),
                        summary: hit["summary"].as_str().unwrap_or("").to_string(),
                    })
                })
                .take(12)
                .collect()
        })
        .unwrap_or_default()
}

fn clear_grid(grid: &gtk::FlowBox) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
}
