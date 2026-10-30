use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use gtk::prelude::*;

use crate::core::models::{PackageSource, PackageSummary};
use crate::ui::details;
use crate::ui::widgets::app_icon::app_icon;
use crate::ui::{AppContext, UiHandles};
use crate::i18n::t;

#[derive(Clone)]
pub struct InstalledPage {
    pub root: gtk::Box,
    list: gtk::ListBox,
    search: gtk::SearchEntry,
    filter: gtk::DropDown,
    update_all: gtk::Button,
    refresh_button: gtk::Button,
    loading: gtk::Box,
    spinner: gtk::Spinner,
    all: Rc<RefCell<Vec<PackageSummary>>>,
    connected: Rc<std::cell::Cell<bool>>,
}

impl InstalledPage {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("page-root");
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_hexpand(true);
        root.set_vexpand(true);

        let title = gtk::Label::new(Some(t("Installed")));
        title.add_css_class("title-2");
        title.add_css_class("page-title");
        title.set_xalign(0.0);
        root.append(&title);

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        controls.add_css_class("page-controls");
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(t("Search installed packages")));
        search.set_hexpand(true);

        let filter = gtk::DropDown::from_strings(&[t("All"), t("Pacman"), t("AUR"), t("Flatpak")]);
        filter.set_selected(0);

        let update_all = gtk::Button::with_label(t("Update All"));
        update_all.add_css_class("suggested-action");
        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some(t("Refresh installed")));

        controls.append(&search);
        controls.append(&filter);
        controls.append(&update_all);
        controls.append(&refresh_button);
        root.append(&controls);

        let loading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let spinner = gtk::Spinner::new();
        loading.append(&spinner);
        loading.append(&gtk::Label::new(Some(t("Loading installed apps…"))));
        loading.set_visible(false);
        root.append(&loading);

        let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header_row.add_css_class("table-header");
        header_row.append(&header_label(t("Package"), true, 0));
        header_row.append(&header_label(t("Version"), false, 16));
        header_row.append(&header_label(t("Source"), false, 10));
        header_row.append(&header_label(t("Status"), false, 10));
        header_row.append(&header_label(t("Actions"), false, 12));
        root.append(&header_row);

        let list = gtk::ListBox::new();
        list.add_css_class("package-list");
        list.set_selection_mode(gtk::SelectionMode::None);

        let scroller = gtk::ScrolledWindow::new();
        scroller.add_css_class("content-scroller");
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list));

        root.append(&scroller);

        Self {
            root,
            list,
            search,
            filter,
            update_all,
            refresh_button,
            loading,
            spinner,
            all: Rc::new(RefCell::new(Vec::new())),
            connected: Rc::new(std::cell::Cell::new(false)),
        }
    }

    pub fn refresh(&self, ctx: AppContext, handles: UiHandles) {
        self.loading.set_visible(true);
        self.spinner.start();
        self.refresh_button.set_sensitive(false);
        let page = self.clone();
        let all_ref = self.all.clone();
        let (tx, rx) = mpsc::channel();
        let ctx_thread = ctx.clone();
        std::thread::spawn(move || {
            let mut installed = ctx_thread.pacman.list_installed().unwrap_or_default();
            let mut flatpaks = ctx_thread.flatpak.list_installed().unwrap_or_default();
            installed.append(&mut flatpaks);
            let _ = tx.send(installed);
        });

        let list = self.list.clone();
        let search = self.search.clone();
        let filter = self.filter.clone();
        let update_all = self.update_all.clone();
        let refresh_button = self.refresh_button.clone();
        let connected = self.connected.clone();
        let loading = self.loading.clone();
        let spinner = self.spinner.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(40), move || {
            match rx.try_recv() {
                Ok(packages) => {
                    spinner.stop();
                    loading.set_visible(false);
                    refresh_button.set_sensitive(true);
                    *all_ref.borrow_mut() = packages.clone();
                    render_list(&list, &packages, &handles, &ctx, 0, "");
                    if !connected.get() {
                        connected.set(true);
                        let handles_for_search = handles.clone();
                        let ctx_for_search = ctx.clone();
                        let all_for_search = all_ref.clone();
                        let list_for_search = list.clone();
                        let filter_for_search = filter.clone();
                        search.connect_search_changed(move |entry| {
                            let query = entry.text().to_string().to_lowercase();
                            let items = all_for_search.borrow();
                            render_list(
                                &list_for_search,
                                &items,
                                &handles_for_search,
                                &ctx_for_search,
                                filter_for_search.selected(),
                                &query,
                            );
                        });

                        let handles_for_btn = handles.clone();
                        update_all.connect_clicked(move |_| {
                            handles_for_btn.queue.add_upgrade_all();
                        });

                        let ctx_for_refresh = ctx.clone();
                        let handles_for_refresh = handles.clone();
                        let page_for_refresh = page.clone();
                        refresh_button.connect_clicked(move |_| {
                            page_for_refresh.refresh(ctx_for_refresh.clone(), handles_for_refresh.clone());
                        });

                        let all_for_filter = all_ref.clone();
                        let list_for_filter = list.clone();
                        let handles_for_filter = handles.clone();
                        let ctx_for_filter = ctx.clone();
                        let search_for_filter = search.clone();
                        filter.connect_selected_notify(move |f| {
                            let query = search_for_filter.text().to_string().to_lowercase();
                            let items = all_for_filter.borrow();
                            render_list(
                                &list_for_filter,
                                &items,
                                &handles_for_filter,
                                &ctx_for_filter,
                                f.selected(),
                                &query,
                            );
                        });
                    }
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    spinner.stop();
                    loading.set_visible(false);
                    refresh_button.set_sensitive(true);
                    glib::ControlFlow::Break
                },
            }
        });
    }
}

fn build_row(pkg: PackageSummary, handles: &UiHandles, ctx: &AppContext) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("package-row");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.add_css_class("package-row-inner");
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(8);
    content.set_margin_end(8);

    let name_col = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    name_col.set_hexpand(true);
    let icon = app_icon(&pkg.name, (pkg.source == PackageSource::Flatpak).then_some(pkg.name.as_str()), 32);
    icon.set_valign(gtk::Align::Center);
    name_col.append(&icon);
    let name = gtk::Label::new(Some(&pkg.name));
    name.add_css_class("title-4");
    name.set_xalign(0.0);
    name_col.append(&name);
    content.append(&name_col);

    let version = gtk::Label::new(Some(&pkg.version));
    version.add_css_class("dim-label");
    version.set_xalign(0.0);
    version.set_width_chars(16);
    content.append(&version);

    let source_badge = gtk::Label::new(Some(match pkg.source {
        crate::core::models::PackageSource::Repo => "Pacman",
        crate::core::models::PackageSource::Aur => "AUR",
        crate::core::models::PackageSource::Flatpak => "Flatpak",
    }));
    source_badge.add_css_class("pill");
    source_badge.set_width_chars(9);
    content.append(&source_badge);

    let status_badge = gtk::Label::new(Some(t("Installed")));
    status_badge.add_css_class("pill-secondary");
    status_badge.set_width_chars(9);
    content.append(&status_badge);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    actions.set_width_request(170);
    let details_btn = gtk::Button::with_label(t("Details"));
    details_btn.add_css_class("flat");
    let remove_btn = gtk::Button::with_label(t("Remove"));
    remove_btn.add_css_class("destructive-action");
    actions.append(&details_btn);
    actions.append(&remove_btn);
    content.append(&actions);

    row.set_child(Some(&content));

    let queue = handles.queue.clone();
    let pkg_name = pkg.name.clone();
    let pkg_source = pkg.source;
    remove_btn.connect_clicked(move |_| {
        queue.add_remove(pkg_name.clone(), pkg_source);
    });

    let handles = handles.clone();
    let ctx = ctx.clone();
    details_btn.connect_clicked(move |_| {
        details::show_details(&ctx, &handles, pkg.clone(), None);
    });

    row
}

fn header_label(text: &str, expand: bool, width_chars: i32) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("table-header-label");
    label.set_xalign(0.0);
    label.set_hexpand(expand);
    if width_chars > 0 {
        label.set_width_chars(width_chars);
    }
    label
}

fn render_list(
    list: &gtk::ListBox,
    packages: &[PackageSummary],
    handles: &UiHandles,
    ctx: &AppContext,
    filter_idx: u32,
    query: &str,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for pkg in packages {
        if !query.is_empty() && !pkg.name.to_lowercase().contains(query) {
            continue;
        }
        let matches_filter = match filter_idx {
            1 => pkg.source == crate::core::models::PackageSource::Repo,
            2 => pkg.source == crate::core::models::PackageSource::Aur,
            3 => pkg.source == crate::core::models::PackageSource::Flatpak,
            _ => true,
        };
        if !matches_filter {
            continue;
        }
        let row = build_row(pkg.clone(), handles, ctx);
        list.append(&row);
    }
}
