use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::core::models::PackageSummary;
use crate::ui::widgets::card;
use crate::ui::{run_search, AppContext, UiHandles};
use crate::i18n::{t, tf};

const INITIAL_RESULT_LIMIT: usize = 90;
const RESULT_PAGE_SIZE: usize = 90;

#[derive(Clone)]
pub struct SearchPage {
    pub root: gtk::Box,
    pub entry: gtk::SearchEntry,
    source_filter: gtk::DropDown,
    state_filter: gtk::DropDown,
    results: gtk::FlowBox,
    status: gtk::Label,
    show_more_btn: gtk::Button,
    all_results: Rc<RefCell<Vec<PackageSummary>>>,
    result_limit: Rc<RefCell<usize>>,
}

impl SearchPage {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("page-root");
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_hexpand(true);
        root.set_vexpand(true);

        let title = gtk::Label::new(Some(t("Search")));
        title.add_css_class("title-2");
        title.add_css_class("page-title");
        title.set_xalign(0.0);
        root.append(&title);

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        controls.add_css_class("page-controls");
        let entry = gtk::SearchEntry::new();
        entry.set_placeholder_text(Some(t("Search packages")));
        entry.set_hexpand(true);

        let source_filter = gtk::DropDown::from_strings(&[t("All Sources"), t("Pacman"), t("AUR"), t("Flatpak")]);
        source_filter.set_selected(0);
        let state_filter = gtk::DropDown::from_strings(&[t("All States"), t("Installed"), t("Not Installed")]);
        state_filter.set_selected(0);

        controls.append(&entry);
        controls.append(&source_filter);
        controls.append(&state_filter);
        root.append(&controls);

        let status = gtk::Label::new(Some(t("Type a package name to search.")));
        status.set_xalign(0.0);
        status.add_css_class("dim-label");
        root.append(&status);

        let show_more_btn = gtk::Button::with_label(t("See More"));
        show_more_btn.set_halign(gtk::Align::Center);
        show_more_btn.set_visible(false);
        root.append(&show_more_btn);

        let results = gtk::FlowBox::new();
        results.set_valign(gtk::Align::Start);
        results.set_min_children_per_line(1);
        results.set_max_children_per_line(3);
        results.set_column_spacing(12);
        // Rows of equal-width tiles, as in the reference store layout.
        results.set_row_spacing(12);
        results.set_homogeneous(true);
        results.set_selection_mode(gtk::SelectionMode::None);
        results.set_hexpand(true);
        results.set_vexpand(true);

        let scroller = gtk::ScrolledWindow::new();
        scroller.add_css_class("content-scroller");
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_child(Some(&results));
        root.append(&scroller);

        Self {
            root,
            entry,
            source_filter,
            state_filter,
            results,
            status,
            show_more_btn,
            all_results: Rc::new(RefCell::new(Vec::new())),
            result_limit: Rc::new(RefCell::new(INITIAL_RESULT_LIMIT)),
        }
    }

    pub fn bind_search(&self, ctx: AppContext, handles: UiHandles, stack: gtk::Stack) {
        let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let entry = self.entry.clone();
        let page = self.clone();
        let ctx_for_search = ctx.clone();
        let handles_for_search = handles.clone();
        entry.connect_search_changed(move |entry| {
            if let Some(id) = debounce.borrow_mut().take() {
                let _ = std::panic::catch_unwind(|| id.remove());
            }
            let query = entry.text().to_string();
            let ctx = ctx_for_search.clone();
            let handles = handles_for_search.clone();
            let page = page.clone();
            let stack = stack.clone();
            let debounce_inner = debounce.clone();
            let id = glib::timeout_add_local(std::time::Duration::from_millis(350), move || {
                *debounce_inner.borrow_mut() = None;
                let query = query.trim().to_string();
                if query.is_empty() {
                    page.reset_result_limit();
                    page.status.set_text(t("Type a package name to search."));
                    page.clear_results();
                    return glib::ControlFlow::Break;
                }
                stack.set_visible_child_name("search");
                page.reset_result_limit();
                page.status.set_text(&tf("Searching for \"{}\"...", &[&query]));
                run_search(query, ctx.clone(), page.clone(), handles.clone());
                glib::ControlFlow::Break
            });
            *debounce.borrow_mut() = Some(id);
        });

        let ctx_for_filter = ctx.clone();
        let handles_for_filter = handles.clone();
        let page = self.clone();
        self.source_filter.connect_selected_notify(move |_| {
            page.reset_result_limit();
            page.render_filtered(&ctx_for_filter, &handles_for_filter);
        });

        let ctx_for_state = ctx.clone();
        let handles_for_state = handles.clone();
        let page = self.clone();
        self.state_filter.connect_selected_notify(move |_| {
            page.reset_result_limit();
            page.render_filtered(&ctx_for_state, &handles_for_state);
        });

        let ctx_for_more = ctx.clone();
        let handles_for_more = handles.clone();
        let page = self.clone();
        self.show_more_btn.connect_clicked(move |_| {
            page.expand_result_limit();
            page.render_filtered(&ctx_for_more, &handles_for_more);
        });
    }

    pub fn set_results(&self, results: Vec<PackageSummary>, ctx: &AppContext, handles: &UiHandles) {
        *self.all_results.borrow_mut() = results;
        self.reset_result_limit();
        self.render_filtered(ctx, handles);
    }

    fn render_filtered(&self, ctx: &AppContext, handles: &UiHandles) {
        self.clear_results();

        let selected_source = self.source_filter.selected();
        let selected_state = self.state_filter.selected();
        let results: Vec<PackageSummary> = self
            .all_results
            .borrow()
            .iter()
            .cloned()
            .filter(|pkg| match selected_source {
                1 => pkg.source == crate::core::models::PackageSource::Repo,
                2 => pkg.source == crate::core::models::PackageSource::Aur,
                3 => pkg.source == crate::core::models::PackageSource::Flatpak,
                _ => true,
            })
            .filter(|pkg| match selected_state {
                1 => pkg.installed,
                2 => !pkg.installed,
                _ => true,
            })
            .collect();

        if results.is_empty() {
            self.show_more_btn.set_visible(false);
            self.status.set_text(t("No results found for selected filters."));
            return;
        }

        let total = results.len();
        let limit = *self.result_limit.borrow();
        let shown = total.min(limit);
        self.status
            .set_text(&tf("{} of {} results", &[&shown, &total]));
        self.show_more_btn.set_visible(shown < total);

        for pkg in results.into_iter().take(limit) {
            let queue = handles.queue.clone();
            let handles_for_details = handles.clone();
            let ctx_for_details = ctx.clone();
            let pkg_for_action = pkg.clone();
            let pkg_for_details = pkg.clone();
            let row = card::build_card(
                &pkg,
                move || {
                    queue.add_install(
                        pkg_for_action.name.clone(),
                        pkg_for_action.source,
                        pkg_for_action.origin.clone(),
                    );
                },
                move || {
                    crate::ui::details::show_details(
                        &ctx_for_details,
                        &handles_for_details,
                        pkg_for_details.clone(),
                    );
                },
            );
            self.results.insert(&row, -1);
        }
    }

    pub fn clear_results(&self) {
        self.show_more_btn.set_visible(false);
        while let Some(child) = self.results.first_child() {
            self.results.remove(&child);
        }
    }

    fn reset_result_limit(&self) {
        *self.result_limit.borrow_mut() = INITIAL_RESULT_LIMIT;
    }

    fn expand_result_limit(&self) {
        *self.result_limit.borrow_mut() += RESULT_PAGE_SIZE;
    }
}
