use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use glib::clone;
use glib::ControlFlow;
use gtk::prelude::*;
use gtk::gdk;
use libadwaita as adw;
use adw::prelude::*;

use crate::core::appstream::AppStreamClient;
use crate::core::cache::{ensure_cache_dirs, load_settings, InstalledCache};
use crate::core::models::{
    ActionKind, PackageSource, PackageSummary, Settings, TerminalMode, ThemeMode,
    TransactionAction, TransactionQueue,
};
use crate::core::providers::aur::Aur;
use crate::core::providers::flatpak::Flatpak;
use crate::core::providers::pacman::Pacman;
use crate::core::providers::{AurProvider, FlatpakProvider, PacmanProvider};
use crate::core::runner::{CommandRunner, LogEvent, RunningHandle};
use crate::core::transactions::{plan_transactions, TransactionPlan};
use crate::i18n::{t, tf};

pub mod details;
pub mod home;
pub mod home_data;
pub mod installed;
pub mod search;
pub mod settings;
pub mod updates;
pub mod widgets;

#[derive(Clone)]
pub struct AppContext {
    pub pacman: Arc<dyn PacmanProvider>,
    pub aur: Arc<dyn AurProvider>,
    pub flatpak: Arc<dyn FlatpakProvider>,
    pub appstream: Arc<AppStreamClient>,
    pub settings: Arc<Mutex<Settings>>,
    pub queue: Arc<Mutex<TransactionQueue>>,
    pub runner: Arc<CommandRunner>,
    pub transaction_in_progress: Arc<Mutex<bool>>,
    pub pacman_installed: Arc<InstalledCache>,
    pub flatpak_installed: Arc<InstalledCache>,
    pub flatpak_available: Arc<Mutex<bool>>,
}

#[derive(Clone)]
pub struct UiHandles {
    pub nav_view: adw::NavigationView,
    pub log_drawer: widgets::log_drawer::LogDrawer,
    pub queue: QueueController,
    pub toasts: adw::ToastOverlay,
}

#[derive(Clone)]
pub struct QueueController {
    ctx: AppContext,
    button: gtk::Button,
    log_drawer: widgets::log_drawer::LogDrawer,
    parent: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
}

impl QueueController {
    pub fn new(
        ctx: AppContext,
        button: gtk::Button,
        log_drawer: widgets::log_drawer::LogDrawer,
        parent: adw::ApplicationWindow,
        toasts: adw::ToastOverlay,
    ) -> Self {
        Self {
            ctx,
            button,
            log_drawer,
            parent,
            toasts,
        }
    }

    fn update_label(&self) {
        let len = self.ctx.queue.lock().unwrap().len();
        self.button.set_label(&tf("Queue ({})", &[&len]));
    }

    fn toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        self.toasts.add_toast(toast);
    }

    pub fn add_install(&self, name: String, source: PackageSource, origin: Option<String>) {
        let mut queue = self.ctx.queue.lock().unwrap();
        queue.push(TransactionAction {
            name,
            source,
            kind: ActionKind::Install,
            origin,
        });
        drop(queue);
        self.update_label();
        self.toast("Added to queue");
    }

    pub fn add_remove(&self, name: String, source: PackageSource) {
        let mut queue = self.ctx.queue.lock().unwrap();
        queue.push(TransactionAction {
            name,
            source,
            kind: ActionKind::Remove,
            origin: None,
        });
        drop(queue);
        self.update_label();
        self.toast("Added to queue");
    }

    pub fn add_upgrade_repo(&self) {
        let mut queue = self.ctx.queue.lock().unwrap();
        queue.push(TransactionAction {
            name: String::from("system"),
            source: PackageSource::Repo,
            kind: ActionKind::Upgrade,
            origin: None,
        });
        drop(queue);
        self.update_label();
        self.toast("System upgrade queued");
    }

    pub fn add_upgrade_packages(&self, actions: Vec<TransactionAction>) {
        if actions.is_empty() {
            self.toast("No packages selected");
            return;
        }
        let total = actions.len();
        let mut queue = self.ctx.queue.lock().unwrap();
        let mut added = 0usize;
        for action in actions {
            if queue.push(action) {
                added += 1;
            }
        }
        drop(queue);
        self.update_label();
        if added == 0 {
            self.toast("Selected updates already queued");
        } else if added == total {
            self.toast("Selected updates queued");
        } else {
            self.toast(&tf(
                "Queued {} update(s), skipped {} duplicates",
                &[&added, &(total - added)],
            ));
        }
    }

    pub fn add_upgrade_all(&self) {
        let mut queue = self.ctx.queue.lock().unwrap();
        queue.push(TransactionAction {
            name: String::from("system"),
            source: PackageSource::Repo,
            kind: ActionKind::Upgrade,
            origin: None,
        });
        queue.push(TransactionAction {
            name: String::from("aur"),
            source: PackageSource::Aur,
            kind: ActionKind::Upgrade,
            origin: None,
        });
        queue.push(TransactionAction {
            name: String::from("flatpak"),
            source: PackageSource::Flatpak,
            kind: ActionKind::Upgrade,
            origin: None,
        });
        drop(queue);
        self.update_label();
        self.toast("All updates queued");
    }

    pub fn show_review_dialog(&self) {
        let queue = self.ctx.queue.lock().unwrap().clone();
        if queue.is_empty() {
            let dialog = adw::MessageDialog::new(
                Some(&self.parent),
                Some("Queue is empty"),
                Some("Add install/remove actions first."),
            );
            dialog.add_response("ok", t("OK"));
            dialog.connect_response(None, |d: &adw::MessageDialog, _| d.close());
            dialog.present();
            return;
        }

        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        for action in &queue.actions {
            let row = gtk::Label::new(Some(&format!(
                "{:?} {} ({:?})",
                action.kind, action.name, action.source
            )));
            row.set_xalign(0.0);
            content.append(&row);
        }

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_min_content_height(180);
        scroller.set_max_content_height(420);
        scroller.set_child(Some(&content));

        let dialog = adw::MessageDialog::new(
            Some(&self.parent),
            Some("Review Transactions"),
            Some("Confirm before executing."),
        );
        dialog.set_extra_child(Some(&scroller));
        dialog.add_response("cancel", t("Cancel"));
        dialog.add_response("execute", t("Execute"));
        dialog.set_response_appearance("execute", adw::ResponseAppearance::Suggested);

        let ctx = self.ctx.clone();
        let log_drawer = self.log_drawer.clone();
        let parent = self.parent.clone();
        let button = self.button.clone();
        let toasts = self.toasts.clone();

        dialog.connect_response(None, move |d: &adw::MessageDialog, resp| {
            if resp == "execute" {
                let plan = plan_transactions(&queue, &ctx.settings.lock().unwrap());
                let started = run_plan(plan, &ctx, &log_drawer, &parent, &toasts);
                if started {
                    ctx.queue.lock().unwrap().clear();
                    button.set_label(t("Queue (0)"));
                }
            }
            d.close();
        });
        dialog.present();
    }
}

pub fn build_ui(app: &adw::Application) {
    let _ = ensure_cache_dirs();

    let settings = load_settings();
    let initial_theme = settings.theme;
    let settings_arc = Arc::new(Mutex::new(settings));
    let ctx = AppContext {
        pacman: Arc::new(Pacman::default()),
        aur: Arc::new(Aur::new(settings_arc.clone())),
        flatpak: Arc::new(Flatpak::default()),
        appstream: Arc::new(AppStreamClient::default()),
        settings: settings_arc,
        queue: Arc::new(Mutex::new(TransactionQueue::default())),
        runner: Arc::new(CommandRunner::default()),
        transaction_in_progress: Arc::new(Mutex::new(false)),
        pacman_installed: Arc::new(InstalledCache::new(std::time::Duration::from_secs(30))),
        flatpak_installed: Arc::new(InstalledCache::new(std::time::Duration::from_secs(30))),
        flatpak_available: Arc::new(Mutex::new(crate::core::cache::command_exists("flatpak"))),
    };

    // App icons for not-installed packages come from AppStream catalogues.
    crate::core::icons::warm_up();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("WinDoesNot Store")
        .default_width(1200)
        .default_height(800)
        .build();

    if let Some(display) = gdk::Display::default() {
        let icon_theme = gtk::IconTheme::for_display(&display);
        icon_theme.add_search_path("assets/icons");
    }
    // Freedesktop name: every icon theme (e.g. Win11) supplies its own store icon.
    window.set_icon_name(Some("system-software-install"));

    let header = adw::HeaderBar::new();
    header.add_css_class("aurora-header");

    let queue_button = gtk::Button::with_label(t("Queue (0)"));
    queue_button.add_css_class("suggested-action");
    queue_button.add_css_class("queue-button");
    queue_button.set_halign(gtk::Align::End);
    queue_button.set_valign(gtk::Align::Start);
    queue_button.set_margin_top(30);
    queue_button.set_margin_end(30);

    let sidebar = gtk::ListBox::new();
    sidebar.add_css_class("navigation-sidebar");
    sidebar.add_css_class("aurora-nav");
    sidebar.set_selection_mode(gtk::SelectionMode::Single);
    sidebar.set_activate_on_single_click(true);

    sidebar.append(&build_nav_row("go-home-symbolic", t("Home")));
    sidebar.append(&build_nav_row("system-search-symbolic", t("Search")));
    sidebar.append(&build_nav_row("view-app-grid-symbolic", t("Installed")));
    sidebar.append(&build_nav_row("system-update-symbolic", t("Updates")));
    sidebar.append(&build_nav_row("preferences-system-symbolic", t("Settings")));

    let sidebar_root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    sidebar_root.add_css_class("sidebar-root");
    sidebar_root.set_size_request(250, -1);
    sidebar_root.set_hexpand(false);
    sidebar_root.set_vexpand(true);

    let sidebar_brand = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    sidebar_brand.add_css_class("sidebar-brand");
    let brand_icon = gtk::Image::from_icon_name("windoesnot");
    brand_icon.set_pixel_size(30);
    let brand_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let brand_title = gtk::Label::new(Some(t("Store")));
    brand_title.add_css_class("sidebar-brand-title");
    brand_title.set_xalign(0.0);
    let brand_subtitle = gtk::Label::new(Some(t("Apps & updates")));
    brand_subtitle.add_css_class("dim-label");
    brand_subtitle.add_css_class("sidebar-brand-subtitle");
    brand_subtitle.set_xalign(0.0);
    brand_text.append(&brand_title);
    brand_text.append(&brand_subtitle);
    // Settings-style heading only: no logo, subtitle or hint.
    brand_icon.set_visible(false);
    brand_subtitle.set_visible(false);
    // WinDoesNot wordmark followed by "Store"; the variant follows light/dark.
    let brand_logo = gtk::Picture::new();
    brand_logo.add_css_class("brand-logo");
    brand_logo.set_can_shrink(true);
    brand_logo.set_content_fit(gtk::ContentFit::Contain);
    brand_logo.set_size_request(118, -1);
    brand_logo.set_valign(gtk::Align::Center);
    BRAND_LOGO.with(|slot| *slot.borrow_mut() = Some(brand_logo.clone()));
    brand_text.set_valign(gtk::Align::Center);
    sidebar_brand.append(&brand_icon);
    sidebar_brand.append(&brand_logo);
    sidebar_brand.append(&brand_text);

    let sidebar_hint = gtk::Label::new(Some(t("One transaction at a time")));
    sidebar_hint.add_css_class("dim-label");
    sidebar_hint.add_css_class("sidebar-hint");
    sidebar_hint.set_wrap(true);
    sidebar_hint.set_xalign(0.0);

    sidebar_root.append(&sidebar_brand);
    sidebar_root.append(&sidebar);
    sidebar_hint.set_visible(false);
    sidebar_root.append(&sidebar_hint);

    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);

    let home_page = home::HomePage::new();
    let search_page = search::SearchPage::new();
    let installed_page = installed::InstalledPage::new();
    let updates_page = updates::UpdatesPage::new();
    let settings_page = settings::SettingsPage::new();

    stack.add_named(&home_page.root, Some("home"));
    stack.add_named(&search_page.root, Some("search"));
    stack.add_named(&installed_page.root, Some("installed"));
    stack.add_named(&updates_page.root, Some("updates"));
    stack.add_named(&settings_page.root, Some("settings"));
    stack.set_visible_child_name("home");

    let nav_view = adw::NavigationView::new();
    let stack_overlay = gtk::Overlay::new();
    stack_overlay.set_child(Some(&stack));
    stack_overlay.add_overlay(&queue_button);
    let main_page = adw::NavigationPage::builder()
        .title("WinDoesNot Store")
        .child(&stack_overlay)
        .build();
    nav_view.push(&main_page);

    let split = adw::NavigationSplitView::new();
    split.set_min_sidebar_width(250.0);
    split.set_max_sidebar_width(250.0);
    split.set_hexpand(true);
    split.set_vexpand(true);
    let sidebar_page = adw::NavigationPage::builder()
        .title("Navigation")
        .child(&sidebar_root)
        .build();
    let content_page = adw::NavigationPage::builder()
        .title("Content")
        .child(&nav_view)
        .build();
    split.set_sidebar(Some(&sidebar_page));
    split.set_content(Some(&content_page));
    sidebar.select_row(sidebar.row_at_index(0).as_ref());

    let log_drawer = widgets::log_drawer::LogDrawer::new();
    let toast_overlay = adw::ToastOverlay::new();

    let queue_controller = QueueController::new(
        ctx.clone(),
        queue_button.clone(),
        log_drawer.clone(),
        window.clone(),
        toast_overlay.clone(),
    );

    let handles = UiHandles {
        nav_view: nav_view.clone(),
        log_drawer: log_drawer.clone(),
        queue: queue_controller.clone(),
        toasts: toast_overlay.clone(),
    };

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_css_class("aurora-toolbar");
    // No header bar: the WinDoesNot window title bar (hyprbars) is enough,
    // like the Quickshell settings window.
    let _ = &header;
    toolbar_view.set_content(Some(&split));
    toolbar_view.set_vexpand(true);

    let content_overlay = gtk::Overlay::new();
    content_overlay.set_hexpand(true);
    content_overlay.set_vexpand(true);
    content_overlay.set_child(Some(&toolbar_view));

    let log_widget = log_drawer.widget().clone();
    log_widget.set_hexpand(true);
    log_widget.set_halign(gtk::Align::Fill);
    log_widget.set_valign(gtk::Align::End);
    content_overlay.add_overlay(&log_widget);
    content_overlay.set_measure_overlay(&log_widget, false);

    toast_overlay.set_child(Some(&content_overlay));
    toast_overlay.set_hexpand(true);
    toast_overlay.set_vexpand(true);
    window.set_content(Some(&toast_overlay));

    apply_theme(initial_theme);
    crate::desktop::watch(reapply_theme);
    // The System palette depends on the effective light/dark state.
    adw::StyleManager::default().connect_dark_notify(|_| {
        CURRENT_THEME.with(|current| setup_css(current.get()));
    });

    let stack_for_home_search = stack.clone();
    home_page.open_search_btn.connect_clicked(move |_| {
        stack_for_home_search.set_visible_child_name("search");
    });

    let stack_for_home_updates = stack.clone();
    let updates_for_home = updates_page.clone();
    let ctx_for_home_updates = ctx.clone();
    let toasts_for_home_updates = handles.toasts.clone();
    home_page.open_updates_btn.connect_clicked(move |_| {
        stack_for_home_updates.set_visible_child_name("updates");
        updates_for_home.refresh(
            ctx_for_home_updates.clone(),
            Some(toasts_for_home_updates.clone()),
        );
    });

    let stack_for_home_installed = stack.clone();
    let installed_for_home = installed_page.clone();
    let ctx_for_home_installed = ctx.clone();
    let handles_for_home_installed = handles.clone();
    home_page.open_installed_btn.connect_clicked(move |_| {
        stack_for_home_installed.set_visible_child_name("installed");
        installed_for_home.refresh(
            ctx_for_home_installed.clone(),
            handles_for_home_installed.clone(),
        );
    });


    queue_button.connect_clicked(clone!(@strong queue_controller => move |_| {
        queue_controller.show_review_dialog();
    }));

    let ctx_for_sidebar = ctx.clone();
    let stack_for_sidebar = stack.clone();
    let handles_for_sidebar = handles.clone();
    let nav_for_sidebar = nav_view.clone();
    let main_page_for_sidebar = main_page.clone();
    sidebar.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            let _ = nav_for_sidebar.pop_to_page(&main_page_for_sidebar);
            let index = row.index();
            match index {
                0 => stack_for_sidebar.set_visible_child_name("home"),
                1 => stack_for_sidebar.set_visible_child_name("search"),
                2 => {
                    stack_for_sidebar.set_visible_child_name("installed");
                    installed_page.refresh(ctx_for_sidebar.clone(), handles_for_sidebar.clone());
                }
                3 => stack_for_sidebar.set_visible_child_name("updates"),
                4 => stack_for_sidebar.set_visible_child_name("settings"),
                _ => {}
            }
        }
    });

    updates_page.connect_apply_all(clone!(@strong queue_controller => move || {
        queue_controller.add_upgrade_all();
    }));

    updates_page.connect_apply_selected(clone!(@strong queue_controller => move |actions| {
        queue_controller.add_upgrade_packages(actions);
    }));

    updates_page.bind(ctx.clone());
    settings_page.bind(ctx.clone());
    search_page.bind_search(ctx.clone(), handles.clone(), stack.clone());
    home_page.bind(ctx.clone(), handles.clone());

    let updates_page_refresh = updates_page.clone();
    let ctx_updates = ctx.clone();
    let toasts_updates = handles.toasts.clone();
    updates_page_refresh.refresh(ctx_updates.clone(), Some(toasts_updates.clone()));
    glib::timeout_add_local(Duration::from_secs(1800), move || {
        updates_page_refresh.refresh(ctx_updates.clone(), Some(toasts_updates.clone()));
        ControlFlow::Continue
    });

    window.present();
}

static SEARCH_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn run_search(query: String, ctx: AppContext, search_page: search::SearchPage, handles: UiHandles) {
    // A slow earlier search (e.g. "l" while typing) must not overwrite newer results.
    let generation = SEARCH_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Vec<PackageSummary>>();
    let ctx_thread = ctx.clone();
    std::thread::spawn(move || {
        // Snap whether flatpak is installed once per call so providers
        // and the installed-set lookup agree.
        let flatpak_enabled = *ctx_thread
            .flatpak_available
            .lock()
            .unwrap();

        // Pull the providers and caches we need into locals so the
        // `move` closures below can capture them without dragging
        // the whole AppContext along.
        let pacman_provider = ctx_thread.pacman.clone();
        let aur_provider = ctx_thread.aur.clone();
        let flatpak_provider = ctx_thread.flatpak.clone();
        let pacman_installed_cache = ctx_thread.pacman_installed.clone();
        let flatpak_installed_cache = ctx_thread.flatpak_installed.clone();

        // Internal channel: sub-threads → collector loop (same thread).
        let (chunk_tx, chunk_rx) = std::sync::mpsc::channel::<SearchChunk>();

        std::thread::scope(|s| {
            // Pacman search
            let tx_p = chunk_tx.clone();
            let pacman_provider_c = pacman_provider.clone();
            let query_c = query.clone();
            s.spawn(move || {
                let result = pacman_provider_c.search(&query_c).unwrap_or_default();
                let _ = tx_p.send(SearchChunk::Pacman(result));
            });

            // AUR search
            let tx_a = chunk_tx.clone();
            let aur_provider_c = aur_provider.clone();
            let query_c = query.clone();
            s.spawn(move || {
                let result = aur_provider_c.search(&query_c).unwrap_or_default();
                let _ = tx_a.send(SearchChunk::Aur(result));
            });

            // Flatpak search (skipped if flatpak isn't installed)
            if flatpak_enabled {
                let tx_f = chunk_tx.clone();
                let flatpak_provider_c = flatpak_provider.clone();
                let query_c = query.clone();
                s.spawn(move || {
                    let result = flatpak_provider_c.search(&query_c).unwrap_or_default();
                    let _ = tx_f.send(SearchChunk::Flatpak(result));
                });
            }

            // Installed-set refreshes (cached, but pre-warm so the
            // first search shows correct installed badges).
            let pacman_provider_c = pacman_provider.clone();
            s.spawn(move || {
                let _ = pacman_installed_cache.get_or_refresh(|| {
                    Ok(pacman_provider_c
                        .list_installed()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|p| p.name)
                        .collect())
                });
            });
            if flatpak_enabled {
                let flatpak_provider_c = flatpak_provider.clone();
                s.spawn(move || {
                    let _ = flatpak_installed_cache.get_or_refresh(|| {
                        Ok(flatpak_provider_c
                            .list_installed()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|p| p.name)
                            .collect())
                    });
                });
            }
        });
        drop(chunk_tx);

        // 2) Collect chunks until all expected providers have reported
        //    in (or the channel closes for some other reason).
        let expected = if flatpak_enabled { 3 } else { 2 };
        let mut pacman_results = Vec::new();
        let mut aur_results = Vec::new();
        let mut flatpak_results = Vec::new();
        let mut received = 0usize;
        while received < expected {
            match chunk_rx.recv() {
                Ok(SearchChunk::Pacman(v)) => {
                    pacman_results = v;
                    received += 1;
                }
                Ok(SearchChunk::Aur(v)) => {
                    aur_results = v;
                    received += 1;
                }
                Ok(SearchChunk::Flatpak(v)) => {
                    flatpak_results = v;
                    received += 1;
                }
                Err(_) => break,
            }
        }

        // 3) Apply installed-badges from the (now warm) cache.
        let pacman_installed = ctx_thread
            .pacman_installed
            .get_or_refresh(|| {
                Ok(ctx_thread
                    .pacman
                    .list_installed()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.name)
                    .collect())
            });
        let flatpak_installed = if flatpak_enabled {
            ctx_thread.flatpak_installed.get_or_refresh(|| {
                Ok(ctx_thread
                    .flatpak
                    .list_installed()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.name)
                    .collect())
            })
        } else {
            HashSet::new()
        };

        for pkg in &mut pacman_results {
            pkg.installed = pacman_installed.contains(&pkg.name);
        }
        for pkg in &mut aur_results {
            pkg.installed = pacman_installed.contains(&pkg.name);
        }
        for pkg in &mut flatpak_results {
            pkg.installed = flatpak_installed.contains(&pkg.name);
        }

        let mut dedup: HashMap<(PackageSource, String), crate::core::models::PackageSummary> =
            HashMap::new();
        for pkg in pacman_results
            .into_iter()
            .chain(aur_results.into_iter())
            .chain(flatpak_results.into_iter())
        {
            let key = (pkg.source, pkg.name.clone());
            dedup.insert(key, pkg);
        }

        let mut results: Vec<_> = dedup.into_values().collect();
        sort_by_relevance(&mut results, &query);
        let _ = done_tx.send(results);
    });

    glib::timeout_add_local(std::time::Duration::from_millis(40), move || match done_rx.try_recv() {
        Ok(results) => {
            if SEARCH_GENERATION.load(std::sync::atomic::Ordering::SeqCst) == generation {
                search_page.set_results(results, &ctx, &handles);
            }
            ControlFlow::Break
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => ControlFlow::Continue,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => ControlFlow::Break,
    });
}

/// Exact name matches first, then names starting with the query, then the rest;
/// repository packages before AUR and Flatpak at equal relevance.
fn sort_by_relevance(results: &mut [PackageSummary], query: &str) {
    let query = query.trim().to_lowercase();
    results.sort_by_cached_key(|pkg| {
        let name = pkg.name.to_lowercase();
        let rank = if name == query {
            0
        } else if name.starts_with(&query) {
            1
        } else if name.contains(&query) {
            2
        } else {
            3
        };
        let source = match pkg.source {
            PackageSource::Repo => 0,
            PackageSource::Aur => 1,
            PackageSource::Flatpak => 2,
        };
        (rank, source, name.len(), name)
    });
}

enum SearchChunk {
    Pacman(Vec<PackageSummary>),
    Aur(Vec<PackageSummary>),
    Flatpak(Vec<PackageSummary>),
}



fn run_plan(
    plan: TransactionPlan,
    ctx: &AppContext,
    log_drawer: &widgets::log_drawer::LogDrawer,
    parent: &adw::ApplicationWindow,
    toasts: &adw::ToastOverlay,
) -> bool {
    if plan.commands.is_empty() {
        return false;
    }

    {
        let mut in_progress = ctx.transaction_in_progress.lock().unwrap();
        if *in_progress {
            toasts.add_toast(adw::Toast::new(
                "A transaction is already running. Wait for it to finish.",
            ));
            return false;
        }
        *in_progress = true;
    }

    let active_managers = match active_package_managers() {
        Ok(active) => active,
        Err(err) => {
            *ctx.transaction_in_progress.lock().unwrap() = false;
            toasts.add_toast(adw::Toast::new(t("Failed to check package manager status")));
            log_drawer.set_visible(true);
            log_drawer.append_line(
                &tf("Failed to check active package managers: {}", &[&err]),
                ctx.runner.log_limit,
            );
            return false;
        }
    };

    if !active_managers.is_empty() {
        *ctx.transaction_in_progress.lock().unwrap() = false;
        toasts.add_toast(adw::Toast::new(
            "Another package manager process is already running",
        ));
        log_drawer.set_visible(true);
        log_drawer.append_line(
            &tf(
                "Refusing to start: active package manager process detected: {}",
                &[&active_managers.join(", ")],
            ),
            ctx.runner.log_limit,
        );
        return false;
    }

    if Path::new("/var/lib/pacman/db.lck").exists() {
        *ctx.transaction_in_progress.lock().unwrap() = false;
        toasts.add_toast(adw::Toast::new(t("Pacman lock file present")));
        log_drawer.set_visible(true);
        log_drawer.append_line(
            "Refusing to start because /var/lib/pacman/db.lck exists. Use the Clear Lock button in Logs.",
            ctx.runner.log_limit,
        );
        return false;
    }

    log_drawer.clear();
    log_drawer.set_visible(true);

    let commands = Rc::new(RefCell::new(plan.commands));
    let ctx_clone = ctx.clone();
    let log_drawer = log_drawer.clone();
    let parent = parent.clone();
    let toasts = toasts.clone();
    let prompt_open = Rc::new(RefCell::new(false));
    let lock_hint_shown = Rc::new(RefCell::new(false));
    let in_progress = ctx_clone.transaction_in_progress.clone();

    // Slot that always points at the handle of the currently-running
    // command. The log-drawer Cancel button calls this, which terminates
    // the child / process group. The cell is cleared once the transaction
    // finishes (success or failure) so the button can't kill something
    // that no longer exists.
    let current_handle: Rc<RefCell<Option<RunningHandle>>> = Rc::new(RefCell::new(None));
    {
        let log_drawer_for_cancel = log_drawer.clone();
        let current_handle_for_cancel = current_handle.clone();
        log_drawer.set_cancel_handler(Some(move || {
            if let Some(handle) = current_handle_for_cancel.borrow().as_ref() {
                handle.cancel();
            }
            log_drawer_for_cancel.set_cancel_handler(None::<fn()>);
        }));
    }

    let next: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let next_clone = next.clone();

    *next.borrow_mut() = Some(Box::new(move || {
        let mut cmds = commands.borrow_mut();
        if cmds.is_empty() {
            *in_progress.lock().unwrap() = false;
            log_drawer.set_cancel_handler(None::<fn()>);
            let dialog = adw::MessageDialog::new(
                Some(&parent),
                Some("Transactions complete"),
                Some("All actions finished."),
            );
            dialog.add_response("ok", t("OK"));
            dialog.connect_response(None, |d: &adw::MessageDialog, _| d.close());
            dialog.present();
            toasts.add_toast(adw::Toast::new(t("Transactions complete")));
            return;
        }
        let cmd = cmds.remove(0);
        let command_trace = format!("$ {}", cmd.display_line());
        let (tx, rx) = mpsc::channel();
        let (input_tx, input_rx) = mpsc::channel();
        let runner = ctx_clone.runner.clone();
        let log_limit = runner.log_limit;
        log_drawer.append_line(&command_trace, log_limit);

        let (terminal_mode, terminal_emulator) = {
            let settings = ctx_clone.settings.lock().unwrap();
            (settings.terminal_mode, settings.terminal_emulator)
        };

        let start_result = match terminal_mode {
            TerminalMode::External => {
                log_drawer.append_line(
                    &tf(
                        "Launching command in external terminal ({})",
                        &[&terminal_emulator.label()],
                    ),
                    log_limit,
                );
                runner.run_external_terminal(cmd, terminal_emulator, tx)
            }
            TerminalMode::Integrated => runner.run_streaming(cmd, tx, Some(input_rx)),
        };

        let handle = match start_result {
            Ok(h) => h,
            Err(err) => {
                *in_progress.lock().unwrap() = false;
                log_drawer.set_cancel_handler(None::<fn()>);
                toasts.add_toast(adw::Toast::new(t("Failed to start command")));
                log_drawer.append_line(&tf("Failed to start command: {}", &[&err]), log_limit);
                return;
            }
        };
        *current_handle.borrow_mut() = Some(handle);

        let next_inner = next_clone.clone();
        let log_drawer = log_drawer.clone();
        let toasts = toasts.clone();
        let parent = parent.clone();
        let prompt_open = prompt_open.clone();
        let lock_hint_shown = lock_hint_shown.clone();
        let in_progress = in_progress.clone();
        let current_handle = current_handle.clone();
        let allow_prompt_dialog = terminal_mode == TerminalMode::Integrated;
    glib::idle_add_local(move || match rx.try_recv() {
            Ok(event) => {
                match event {
                    LogEvent::Line(line) => {
                        if allow_prompt_dialog && should_prompt(&line) && !*prompt_open.borrow() {
                            *prompt_open.borrow_mut() = true;
                            show_prompt_dialog(
                                &parent,
                                &line,
                                input_tx.clone(),
                                prompt_open.clone(),
                            );
                        }
                        log_drawer.append_line(&line, log_limit);
                        if !*lock_hint_shown.borrow() {
                            let lower = line.to_lowercase();
                            if lower.contains("unable to lock database")
                                || lower.contains("could not lock database")
                                || lower.contains("/var/lib/pacman/db.lck")
                            {
                                *lock_hint_shown.borrow_mut() = true;
                                log_drawer.append_line(
                                    "Hint: pacman lock detected. If no active package manager is running, remove it with: sudo rm -f /var/lib/pacman/db.lck",
                                    log_limit,
                                );
                                toasts.add_toast(adw::Toast::new(t("Pacman lock file detected")));
                            }
                        }
                    }
                    LogEvent::Finished(code) => {
                        *current_handle.borrow_mut() = None;
                        if code != 0 {
                            *in_progress.lock().unwrap() = false;
                            log_drawer.set_cancel_handler(None::<fn()>);
                            toasts.add_toast(adw::Toast::new(&tf(
                                "Command failed ({})",
                                &[&code],
                            )));
                        } else if let Some(next) = &*next_inner.borrow() {
                            next();
                        } else {
                            *in_progress.lock().unwrap() = false;
                            log_drawer.set_cancel_handler(None::<fn()>);
                        }
                        return ControlFlow::Break;
                    }
                }
                ControlFlow::Continue
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                *in_progress.lock().unwrap() = false;
                log_drawer.set_cancel_handler(None::<fn()>);
                ControlFlow::Break
            }
        });
    }) as Box<dyn Fn()>);

    {
        let next_ref = next.borrow();
        if let Some(next_fn) = next_ref.as_ref() {
            next_fn();
        }
    }
    true
}

fn active_package_managers() -> Result<Vec<String>, String> {
    let mut active = Vec::new();
    let names = ["pacman", "yay", "paru", "pamac", "pkcon", "packagekitd", "aurora-helper"];
    for name in names {
        match Command::new("pgrep").arg("-x").arg(name).status() {
            Ok(status) if status.success() => active.push(name.to_string()),
            Ok(_) => {}
            Err(err) => return Err(format!("pgrep failed for {name}: {err}")),
        }
    }
    Ok(active)
}

fn should_prompt(line: &str) -> bool {
    let l = line.to_lowercase();
    l.contains("packages to cleanbuild")
        || l.contains("proceed with installation")
        || l.contains("proceed with transaction")
        || l.contains("enter a number")
        || l.contains("[y/n]")
        || l.contains("[y]")
        || l.ends_with("?")
}

fn show_prompt_dialog(
    parent: &adw::ApplicationWindow,
    prompt: &str,
    input_tx: mpsc::Sender<String>,
    prompt_open: Rc<RefCell<bool>>,
) {
    // Clean the prompt text: strip trailing ": y", ": n", ": " etc.
    let clean_prompt = prompt
        .trim_end_matches(|c: char| c == ':' || c == ' ')
        .to_string();

    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some("Input Required"),
        Some(&clean_prompt),
    );

    let lower = clean_prompt.to_lowercase();
    let is_yn = lower.contains("[y/n]") || lower.contains("[y/n]");

    if is_yn {
        let _ = input_tx.send("y".to_string());
        *prompt_open.borrow_mut() = false;
        return;
    } else {
        // Generic prompt with all options
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some(t("Enter response (e.g., y, n, 1)")));
        dialog.set_extra_child(Some(&entry));

        dialog.add_response("n", t("No"));
        dialog.add_response("ab", t("Abort"));
        dialog.add_response("a", t("All"));
        dialog.add_response("i", t("Installed"));
        dialog.add_response("no", t("NotInstalled"));
        dialog.add_response("cancel", t("Cancel"));
        dialog.add_response("y", t("Yes"));
        dialog.set_response_appearance("y", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("y"));
        dialog.set_close_response("cancel");

        let prompt_open_send = prompt_open.clone();
        let input_tx_send = input_tx.clone();
        dialog.connect_response(Some("send"), move |d, _| {
            let text = entry.text().to_string();
            let _ = input_tx_send.send(text);
            *prompt_open_send.borrow_mut() = false;
            d.close();
        });
        dialog.connect_response(Some("cancel"), move |d, _| {
            d.close();
        });
    }

    let input_tx_quick = input_tx.clone();
    let prompt_open_quick = prompt_open.clone();
    dialog.connect_response(Some("y"), move |d, _| {
        let _ = input_tx_quick.send("y".to_string());
        *prompt_open_quick.borrow_mut() = false;
        d.close();
    });
    let input_tx_quick2 = input_tx.clone();
    let prompt_open_quick2 = prompt_open.clone();
    dialog.connect_response(Some("n"), move |d, _| {
        let _ = input_tx_quick2.send("n".to_string());
        *prompt_open_quick2.borrow_mut() = false;
        d.close();
    });
    let prompt_open_a = prompt_open.clone();
    let input_tx_a = input_tx.clone();
    dialog.connect_response(Some("a"), move |d, _| {
        let _ = input_tx_a.send("a".to_string());
        *prompt_open_a.borrow_mut() = false;
        d.close();
    });
    let prompt_open_i = prompt_open.clone();
    let input_tx_i = input_tx.clone();
    dialog.connect_response(Some("i"), move |d, _| {
        let _ = input_tx_i.send("i".to_string());
        *prompt_open_i.borrow_mut() = false;
        d.close();
    });
    let prompt_open_no = prompt_open.clone();
    let input_tx_no = input_tx.clone();
    dialog.connect_response(Some("no"), move |d, _| {
        let _ = input_tx_no.send("no".to_string());
        *prompt_open_no.borrow_mut() = false;
        d.close();
    });
    let prompt_open_ab = prompt_open.clone();
    let input_tx_ab = input_tx.clone();
    dialog.connect_response(Some("ab"), move |d, _| {
        let _ = input_tx_ab.send("ab".to_string());
        *prompt_open_ab.borrow_mut() = false;
        d.close();
    });
    dialog.present();
}

thread_local! {
    static AURORA_CSS_PROVIDER: RefCell<Option<gtk::CssProvider>> = RefCell::new(None);
}

struct ThemePalette {
    toolbar_from: &'static str,
    toolbar_to: &'static str,
    header_bg: &'static str,
    header_border: &'static str,
    sidebar_from: &'static str,
    sidebar_to: &'static str,
    sidebar_border: &'static str,
    nav_selected_from: &'static str,
    nav_selected_to: &'static str,
    nav_selected_shadow: &'static str,
    card_from: &'static str,
    card_to: &'static str,
    card_border: &'static str,
    page_bg: &'static str,
    table_header_bg: &'static str,
    table_header_border: &'static str,
    table_header_text: &'static str,
    scroller_border: &'static str,
    scroller_bg: &'static str,
    row_border: &'static str,
    row_bg: &'static str,
    row_hover_bg: &'static str,
    pill_from: &'static str,
    pill_to: &'static str,
    pill_secondary_from: &'static str,
    pill_secondary_to: &'static str,
    pill_secondary_fg: &'static str,
}

fn palette_for_theme(theme: ThemeMode) -> ThemePalette {
    match theme {
        ThemeMode::System => {
            if adw::StyleManager::default().is_dark() {
                palette_for_theme(ThemeMode::Dark)
            } else {
                palette_for_theme(ThemeMode::Light)
            }
        }
        // WinDoesNot palette: colours of the Quickshell settings window.
        ThemeMode::Light => ThemePalette {
            toolbar_from: "#f2f2f2",
            toolbar_to: "#f2f2f2",
            header_bg: "#f2f2f2",
            header_border: "#f2f2f2",
            sidebar_from: "#e6e6e6",
            sidebar_to: "#e6e6e6",
            sidebar_border: "#e6e6e6",
            nav_selected_from: "#c8c8c8",
            nav_selected_to: "#c8c8c8",
            nav_selected_shadow: "rgba(0, 0, 0, 0)",
            card_from: "#f5f5f5",
            card_to: "#f5f5f5",
            card_border: "#d0d0d0",
            page_bg: "#f2f2f2",
            table_header_bg: "#f5f5f5",
            table_header_border: "#d0d0d0",
            table_header_text: "#666666",
            scroller_border: "#d0d0d0",
            scroller_bg: "#f2f2f2",
            row_border: "#d0d0d0",
            row_bg: "#f5f5f5",
            row_hover_bg: "#e6e6e6",
            pill_from: "#fe6f2d",
            pill_to: "#fe6f2d",
            pill_secondary_from: "#d0d0d0",
            pill_secondary_to: "#d0d0d0",
            pill_secondary_fg: "#444444",
        },
        // WinDoesNot palette: colours of the Quickshell settings window.
        ThemeMode::Dark => ThemePalette {
            toolbar_from: "#202020",
            toolbar_to: "#202020",
            header_bg: "#202020",
            header_border: "#202020",
            sidebar_from: "#171717",
            sidebar_to: "#171717",
            sidebar_border: "#171717",
            nav_selected_from: "#2a2a2a",
            nav_selected_to: "#2a2a2a",
            nav_selected_shadow: "rgba(0, 0, 0, 0)",
            card_from: "#141414",
            card_to: "#141414",
            card_border: "#3a3a3a",
            page_bg: "#202020",
            table_header_bg: "#141414",
            table_header_border: "#3a3a3a",
            table_header_text: "#888888",
            scroller_border: "#3a3a3a",
            scroller_bg: "#202020",
            row_border: "#3a3a3a",
            row_bg: "#141414",
            row_hover_bg: "#1e1e1e",
            pill_from: "#fe6f2d",
            pill_to: "#fe6f2d",
            pill_secondary_from: "#3a3a3a",
            pill_secondary_to: "#3a3a3a",
            pill_secondary_fg: "#cccccc",
        },
        ThemeMode::Ocean => ThemePalette {
            toolbar_from: "rgba(4, 28, 46, 0.96)",
            toolbar_to: "rgba(5, 45, 71, 0.96)",
            header_bg: "rgba(5, 26, 43, 0.88)",
            header_border: "rgba(74, 167, 207, 0.24)",
            sidebar_from: "rgba(6, 30, 50, 0.95)",
            sidebar_to: "rgba(3, 23, 40, 0.95)",
            sidebar_border: "rgba(67, 171, 210, 0.27)",
            nav_selected_from: "#0aa0d6",
            nav_selected_to: "#1dc4f0",
            nav_selected_shadow: "rgba(15, 152, 211, 0.30)",
            card_from: "rgba(11, 42, 64, 0.94)",
            card_to: "rgba(9, 33, 54, 0.92)",
            card_border: "rgba(74, 171, 209, 0.30)",
            page_bg: "rgba(7, 33, 52, 0.45)",
            table_header_bg: "rgba(8, 41, 63, 0.78)",
            table_header_border: "rgba(72, 163, 201, 0.22)",
            table_header_text: "rgba(177, 226, 242, 0.90)",
            scroller_border: "rgba(73, 163, 202, 0.22)",
            scroller_bg: "rgba(5, 28, 44, 0.44)",
            row_border: "rgba(71, 163, 202, 0.24)",
            row_bg: "rgba(10, 37, 58, 0.76)",
            row_hover_bg: "rgba(13, 48, 72, 0.86)",
            pill_from: "rgba(18, 156, 223, 0.95)",
            pill_to: "rgba(35, 196, 237, 0.95)",
            pill_secondary_from: "rgba(42, 193, 156, 0.92)",
            pill_secondary_to: "rgba(70, 224, 181, 0.92)",
            pill_secondary_fg: "#08271f",
        },
        ThemeMode::Emerald => ThemePalette {
            toolbar_from: "rgba(10, 34, 23, 0.96)",
            toolbar_to: "rgba(14, 49, 32, 0.96)",
            header_bg: "rgba(9, 29, 20, 0.88)",
            header_border: "rgba(93, 178, 128, 0.23)",
            sidebar_from: "rgba(11, 36, 24, 0.95)",
            sidebar_to: "rgba(8, 26, 18, 0.95)",
            sidebar_border: "rgba(91, 182, 130, 0.28)",
            nav_selected_from: "#1ba36f",
            nav_selected_to: "#2ec68a",
            nav_selected_shadow: "rgba(26, 157, 103, 0.30)",
            card_from: "rgba(14, 47, 31, 0.94)",
            card_to: "rgba(10, 36, 24, 0.92)",
            card_border: "rgba(96, 178, 130, 0.30)",
            page_bg: "rgba(10, 35, 23, 0.46)",
            table_header_bg: "rgba(13, 43, 29, 0.78)",
            table_header_border: "rgba(89, 170, 124, 0.22)",
            table_header_text: "rgba(190, 233, 203, 0.90)",
            scroller_border: "rgba(91, 173, 126, 0.22)",
            scroller_bg: "rgba(8, 29, 19, 0.44)",
            row_border: "rgba(93, 173, 126, 0.24)",
            row_bg: "rgba(13, 40, 27, 0.76)",
            row_hover_bg: "rgba(17, 54, 35, 0.86)",
            pill_from: "rgba(29, 176, 110, 0.95)",
            pill_to: "rgba(54, 209, 139, 0.95)",
            pill_secondary_from: "rgba(64, 195, 126, 0.92)",
            pill_secondary_to: "rgba(106, 227, 165, 0.92)",
            pill_secondary_fg: "#0a2a1a",
        },
        ThemeMode::Sunset => ThemePalette {
            toolbar_from: "rgba(46, 23, 20, 0.96)",
            toolbar_to: "rgba(62, 31, 24, 0.96)",
            header_bg: "rgba(44, 21, 18, 0.88)",
            header_border: "rgba(205, 121, 94, 0.25)",
            sidebar_from: "rgba(48, 24, 20, 0.95)",
            sidebar_to: "rgba(37, 18, 15, 0.95)",
            sidebar_border: "rgba(207, 120, 94, 0.28)",
            nav_selected_from: "#e06a3f",
            nav_selected_to: "#ff965c",
            nav_selected_shadow: "rgba(207, 104, 67, 0.32)",
            card_from: "rgba(63, 32, 25, 0.94)",
            card_to: "rgba(51, 25, 20, 0.92)",
            card_border: "rgba(194, 117, 91, 0.30)",
            page_bg: "rgba(42, 22, 18, 0.47)",
            table_header_bg: "rgba(57, 28, 22, 0.78)",
            table_header_border: "rgba(189, 114, 87, 0.24)",
            table_header_text: "rgba(243, 209, 193, 0.90)",
            scroller_border: "rgba(190, 115, 88, 0.24)",
            scroller_bg: "rgba(38, 19, 16, 0.44)",
            row_border: "rgba(193, 117, 90, 0.26)",
            row_bg: "rgba(57, 29, 23, 0.76)",
            row_hover_bg: "rgba(71, 37, 28, 0.86)",
            pill_from: "rgba(225, 104, 63, 0.96)",
            pill_to: "rgba(255, 149, 86, 0.96)",
            pill_secondary_from: "rgba(255, 140, 98, 0.92)",
            pill_secondary_to: "rgba(255, 181, 124, 0.92)",
            pill_secondary_fg: "#3a170d",
        },
        ThemeMode::Graphite => ThemePalette {
            toolbar_from: "rgba(23, 27, 36, 0.96)",
            toolbar_to: "rgba(30, 36, 48, 0.96)",
            header_bg: "rgba(19, 23, 31, 0.88)",
            header_border: "rgba(124, 137, 162, 0.20)",
            sidebar_from: "rgba(24, 29, 39, 0.95)",
            sidebar_to: "rgba(18, 22, 31, 0.95)",
            sidebar_border: "rgba(125, 138, 164, 0.24)",
            nav_selected_from: "#647aa4",
            nav_selected_to: "#86a0cf",
            nav_selected_shadow: "rgba(96, 119, 166, 0.28)",
            card_from: "rgba(29, 35, 48, 0.94)",
            card_to: "rgba(23, 28, 39, 0.92)",
            card_border: "rgba(121, 136, 165, 0.28)",
            page_bg: "rgba(20, 25, 35, 0.46)",
            table_header_bg: "rgba(27, 33, 45, 0.78)",
            table_header_border: "rgba(120, 135, 164, 0.20)",
            table_header_text: "rgba(204, 214, 234, 0.88)",
            scroller_border: "rgba(122, 136, 166, 0.20)",
            scroller_bg: "rgba(17, 21, 30, 0.44)",
            row_border: "rgba(122, 136, 166, 0.22)",
            row_bg: "rgba(25, 31, 43, 0.76)",
            row_hover_bg: "rgba(33, 41, 56, 0.86)",
            pill_from: "rgba(104, 128, 176, 0.95)",
            pill_to: "rgba(133, 161, 210, 0.95)",
            pill_secondary_from: "rgba(120, 168, 180, 0.92)",
            pill_secondary_to: "rgba(146, 196, 208, 0.92)",
            pill_secondary_fg: "#0e1b1f",
        },
    }
}

fn themed_css(theme: ThemeMode) -> String {
    let palette = palette_for_theme(theme);
    let mut css = r#"
        @define-color accent_bg_color #fe6f2d;
        @define-color accent_color #fe783e;
        @define-color accent_fg_color #ffffff;
        :root {
            --accent-bg-color: #fe6f2d;
            --accent-color: #fe783e;
            --accent-fg-color: #ffffff;
        }
        /* Layout and colours follow the Quickshell settings window (Settings.qml). */
        window, .aurora-toolbar {
            font-family: sans-serif;
            background-color: $PAGE_BG$;
            background-image: none;
            color: $TEXT$;
        }
        .sidebar-root {
            background-color: $SIDEBAR_FROM$;
            background-image: none;
            border: none;
            border-radius: 0;
        }
        .sidebar-brand {
            padding: 40px 20px 30px 20px;
        }
        .sidebar-brand-title {
            font-size: 20px;
            font-weight: 700;
            color: $TEXT$;
        }
        .aurora-nav {
            background: transparent;
            border: none;
            padding: 0 10px 30px 10px;
        }
        .aurora-nav row {
            margin: 0 0 8px 0;
            padding: 0;
            border-radius: 6px;
            min-height: 45px;
            color: $NAV_TEXT$;
            background-color: transparent;
            background-image: none;
            box-shadow: none;
            outline: none;
        }
        .aurora-nav row:hover {
            background-color: $NAV_HOVER$;
        }
        .aurora-nav row:selected,
        .aurora-nav row:selected:hover {
            background-color: $NAV_SELECTED_FROM$;
            background-image: none;
            color: $NAV_TEXT$;
            box-shadow: none;
        }
        .nav-row {
            padding: 0 10px 0 4px;
        }
        .nav-indicator {
            border-radius: 2px;
            background-color: transparent;
        }
        .aurora-nav row:selected .nav-indicator {
            background-color: #fe6f2d;
        }
        .nav-label {
            font-size: 14px;
            font-weight: 400;
        }
        .page-root {
            background-color: transparent;
            border-radius: 0;
            padding: 18px;
        }
        .page-title {
            font-size: 24px;
            font-weight: 700;
            color: $TEXT$;
            padding-bottom: 14px;
            border-bottom: 1px solid $DIVIDER$;
        }
        .title-1 {
            font-size: 24px;
            font-weight: 700;
            color: $TEXT$;
        }
        .title-2, .title-3, .title-4 {
            color: $TEXT$;
        }
        flowbox, flowboxchild, flowboxchild:hover, flowboxchild:selected, list.package-list {
            background-color: transparent;
            background-image: none;
            box-shadow: none;
        }
        .page-controls {
            padding: 4px 0;
        }
        button {
            border-radius: 10px;
            min-height: 34px;
            padding: 0 14px;
            font-size: 13px;
            background-color: transparent;
            background-image: none;
            border: 1px solid $BUTTON_BORDER$;
            color: $NAV_TEXT$;
            box-shadow: none;
        }
        button:hover {
            background-color: $NAV_HOVER$;
        }
        button.suggested-action {
            background-color: #fe6f2d;
            border-color: #fe6f2d;
            color: #ffffff;
        }
        button.suggested-action:hover {
            background-color: shade(#fe6f2d, 1.08);
        }
        button.flat, .aurora-nav button {
            border-color: transparent;
        }
        .queue-button {
            font-weight: 600;
        }
        entry, searchbar entry, .search-entry, dropdown > button, spinbutton {
            border-radius: 6px;
            min-height: 34px;
            background-color: $CARD_FROM$;
            border: 1px solid $CARD_BORDER$;
            box-shadow: none;
        }
        .card {
            background-color: $CARD_FROM$;
            background-image: none;
            border-radius: 10px;
            border: 1px solid $CARD_BORDER$;
            box-shadow: none;
            padding: 16px;
        }
        .package-card {
            min-height: 96px;
            padding: 14px 16px;
        }
        .package-card:hover {
            background-color: $ROW_HOVER_BG$;
        }
        .table-header {
            padding: 6px 12px;
            border-radius: 6px;
            background-color: transparent;
            border: none;
            border-bottom: 1px solid $DIVIDER$;
        }
        .table-header-label {
            color: $DIM$;
            font-weight: 600;
            font-size: 12px;
        }
        .table-subtext {
            font-size: 12px;
            color: $DIM$;
        }
        .content-scroller {
            border: none;
            border-radius: 0;
            background-color: transparent;
        }
        .package-row,
        .update-row {
            border-radius: 10px;
            margin: 4px 0;
            border: 1px solid $ROW_BORDER$;
            background-color: $ROW_BG$;
        }
        .package-row:hover,
        .update-row:hover {
            background-color: $ROW_HOVER_BG$;
        }
        .package-row-inner,
        .update-row-inner {
            padding: 8px 12px;
        }
        .pill {
            background-color: alpha(#fe6f2d, 0.18);
            background-image: none;
            color: #fe6f2d;
            border-radius: 9px;
            padding: 1px 8px;
            font-weight: 700;
            font-size: 10px;
        }
        .pill-secondary {
            background-color: $PILL_SECONDARY_FROM$;
            background-image: none;
            color: $PILL_SECONDARY_FG$;
            border-radius: 9px;
            padding: 1px 8px;
            font-weight: 700;
            font-size: 10px;
        }
        .log-drawer-root {
            background-color: $SIDEBAR_FROM$;
            border-top: 1px solid $DIVIDER$;
        }
        .log-drawer-header {
            background-color: $SIDEBAR_FROM$;
            border-bottom: 1px solid $DIVIDER$;
        }
        .log-drawer-scroller,
        .log-drawer-scroller > viewport,
        .log-drawer-scroller textview,
        .log-drawer-scroller textview text {
            background-color: $CARD_FROM$;
            color: $TEXT$;
            font-family: monospace;
        }
        .log-resize-handle {
            min-height: 6px;
            padding: 0;
            margin: 0;
            background-color: $DIVIDER$;
        }
        .dim-label {
            color: $DIM$;
        }
        separator {
            background-color: $DIVIDER$;
        }
        .brand-logo {
            margin-right: 2px;
        }
        .banner {
            border-radius: 12px;
            padding: 24px;
        }
        .banner-title {
            font-size: 28px;
            font-weight: 800;
            color: #ffffff;
        }
        .banner-summary {
            font-size: 14px;
            color: rgba(255, 255, 255, 0.85);
        }
        .banner-steam { background-image: linear-gradient(135deg, #0b1f3a, #1d4475); }
        .banner-gimp { background-image: linear-gradient(135deg, #3a2414, #7a4a24); }
        .banner-obs { background-image: linear-gradient(135deg, #17162b, #39306f); }
        .banner-office { background-image: linear-gradient(135deg, #0f3a2c, #1a7352); }
        .banner-localsend { background-image: linear-gradient(135deg, #0c3640, #137a86); }
        button.banner-arrow {
            border-radius: 999px;
            min-width: 40px;
            min-height: 40px;
            padding: 0;
            border: none;
            background-color: rgba(0, 0, 0, 0.35);
            color: #ffffff;
        }
        button.banner-arrow:hover {
            background-color: rgba(0, 0, 0, 0.55);
        }
        button.category-tile {
            min-height: 72px;
            border-radius: 10px;
            border: none;
            color: #ffffff;
        }
        .category-label {
            font-size: 17px;
            font-weight: 800;
        }
        button.cat-create { background-image: linear-gradient(135deg, #7b4dbb, #4a5fd6); }
        button.cat-work { background-image: linear-gradient(135deg, #b8661a, #e0a13a); }
        button.cat-games { background-image: linear-gradient(135deg, #d4508f, #6a4ad8); }
        button.cat-social { background-image: linear-gradient(135deg, #d63c6e, #ea7a52); }
        button.cat-media { background-image: linear-gradient(135deg, #168a5f, #2bb57f); }
        button.cat-develop { background-image: linear-gradient(135deg, #34373f, #50545f); }
        button.category-tile:hover { opacity: 0.9; }
        .letter-icon {
            background-color: alpha(#fe6f2d, 0.18);
            color: #fe6f2d;
            border-radius: 12px;
            font-size: 26px;
            font-weight: 800;
        }
    "#.to_string();

    let replacements = [
        ("$TOOLBAR_FROM$", palette.toolbar_from),
        ("$TOOLBAR_TO$", palette.toolbar_to),
        ("$HEADER_BG$", palette.header_bg),
        ("$HEADER_BORDER$", palette.header_border),
        ("$SIDEBAR_FROM$", palette.sidebar_from),
        ("$SIDEBAR_TO$", palette.sidebar_to),
        ("$SIDEBAR_BORDER$", palette.sidebar_border),
        ("$NAV_SELECTED_FROM$", palette.nav_selected_from),
        ("$NAV_SELECTED_TO$", palette.nav_selected_to),
        ("$NAV_SELECTED_SHADOW$", palette.nav_selected_shadow),
        ("$CARD_FROM$", palette.card_from),
        ("$CARD_TO$", palette.card_to),
        ("$CARD_BORDER$", palette.card_border),
        ("$PAGE_BG$", palette.page_bg),
        ("$TABLE_HEADER_BG$", palette.table_header_bg),
        ("$TABLE_HEADER_BORDER$", palette.table_header_border),
        ("$TABLE_HEADER_TEXT$", palette.table_header_text),
        ("$SCROLLER_BORDER$", palette.scroller_border),
        ("$SCROLLER_BG$", palette.scroller_bg),
        ("$ROW_BORDER$", palette.row_border),
        ("$ROW_BG$", palette.row_bg),
        ("$ROW_HOVER_BG$", palette.row_hover_bg),
        ("$PILL_FROM$", palette.pill_from),
        ("$PILL_TO$", palette.pill_to),
        ("$PILL_SECONDARY_FROM$", palette.pill_secondary_from),
        ("$PILL_SECONDARY_TO$", palette.pill_secondary_to),
        ("$PILL_SECONDARY_FG$", palette.pill_secondary_fg),
    ];
    for (from, to) in replacements {
        css = css.replace(from, to);
    }
    let light = match theme {
        ThemeMode::Light => true,
        ThemeMode::System => !adw::StyleManager::default().is_dark(),
        _ => false,
    };
    // Text, divider and hover colours from Settings.qml (dark / light).
    let (text, nav_text, dim, divider, nav_hover, button_border) = if light {
        ("#1a1a1a", "#555555", "#666666", "#d0d0d0", "#dedede", "#cccccc")
    } else {
        ("#ffffff", "#aaaaaa", "#888888", "#3a3a3a", "#1e1e1e", "#555555")
    };
    for (from, to) in [
        ("$TEXT$", text),
        ("$NAV_TEXT$", nav_text),
        ("$DIM$", dim),
        ("$DIVIDER$", divider),
        ("$NAV_HOVER$", nav_hover),
        ("$BUTTON_BORDER$", button_border),
    ] {
        css = css.replace(from, to);
    }
    if matches!(theme, ThemeMode::System | ThemeMode::Light | ThemeMode::Dark) {
        // The WinDoesNot palettes use the desktop accent from the shared settings.
        let accent = crate::desktop::accent();
        let (r, g, b) = crate::desktop::accent_rgb(&accent);
        let foreground = crate::desktop::accent_foreground(&accent);
        css = css
            .replace("#fe6f2d", &accent)
            .replace("#fe783e", &accent)
            .replace("rgba(254, 111, 45,", &format!("rgba({r}, {g}, {b},"))
            .replace("@define-color accent_fg_color #ffffff;", &format!("@define-color accent_fg_color {foreground};"))
            .replace("--accent-fg-color: #ffffff;", &format!("--accent-fg-color: {foreground};"))
            .replace("color: #f5f9ff;", &format!("color: {foreground};"));
        // Runs after the accent swap above, so match the substituted colour.
        css = css.replace(
            &format!("border-color: {accent};\n            color: #ffffff;"),
            &format!("border-color: {accent};\n            color: {foreground};"),
        );
    }
    css
}

fn setup_css(theme: ThemeMode) {
    let Some(display) = gdk::Display::default() else {
        return;
    };

    AURORA_CSS_PROVIDER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let provider = slot.get_or_insert_with(|| {
            let provider = gtk::CssProvider::new();
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            provider
        });
        provider.load_from_data(&themed_css(theme));
    });
    let light = match theme {
        ThemeMode::Light => true,
        ThemeMode::System => !adw::StyleManager::default().is_dark(),
        _ => false,
    };
    update_brand_logo(!light);
}

fn build_nav_row(icon_name: &str, title: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.add_css_class("nav-row");

    // Accent bar of the active entry, as in the Quickshell settings.
    let indicator = gtk::Box::new(gtk::Orientation::Vertical, 0);
    indicator.add_css_class("nav-indicator");
    indicator.set_size_request(3, 22);
    indicator.set_valign(gtk::Align::Center);
    content.append(&indicator);

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(20);

    let label = gtk::Label::new(Some(title));
    label.add_css_class("nav-label");
    label.set_xalign(0.0);
    label.set_hexpand(true);

    content.append(&icon);
    content.append(&label);
    row.set_child(Some(&content));
    row
}

thread_local! {
    static CURRENT_THEME: std::cell::Cell<ThemeMode> = std::cell::Cell::new(ThemeMode::System);
    static BRAND_LOGO: RefCell<Option<gtk::Picture>> = RefCell::new(None);
}

/// Light wordmark on dark surfaces, dark wordmark on light ones.
fn update_brand_logo(dark: bool) {
    let bytes: &'static [u8] = if dark {
        include_bytes!("../../assets/brand/windoesnot-logo-hell.png")
    } else {
        include_bytes!("../../assets/brand/windoesnot-logo-dunkel.png")
    };
    let Ok(texture) = gdk::Texture::from_bytes(&glib::Bytes::from_static(bytes)) else {
        return;
    };
    BRAND_LOGO.with(|slot| {
        if let Some(picture) = slot.borrow().as_ref() {
            picture.set_paintable(Some(&texture));
        }
    });
}

/// Re-applies the active theme, e.g. after the desktop accent or light/dark mode changed.
pub(crate) fn reapply_theme() {
    apply_theme(CURRENT_THEME.with(|current| current.get()));
}

pub(crate) fn apply_theme(theme: ThemeMode) {
    CURRENT_THEME.with(|current| current.set(theme));
    let manager = adw::StyleManager::default();
    match theme {
        // "System" follows the WinDoesNot desktop setting, else the global colour scheme.
        ThemeMode::System => manager.set_color_scheme(match crate::desktop::read_prefs().dark {
            Some(true) => adw::ColorScheme::ForceDark,
            Some(false) => adw::ColorScheme::ForceLight,
            None => adw::ColorScheme::Default,
        }),
        ThemeMode::Light => manager.set_color_scheme(adw::ColorScheme::ForceLight),
        ThemeMode::Dark
        | ThemeMode::Ocean
        | ThemeMode::Emerald
        | ThemeMode::Sunset
        | ThemeMode::Graphite => manager.set_color_scheme(adw::ColorScheme::ForceDark),
    }
    setup_css(theme);
}
