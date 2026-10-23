use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;

use gtk::prelude::*;
use libadwaita as adw;

use crate::core::models::{ActionKind, AurHelperKind, PackageSource, TransactionAction};
use crate::ui::AppContext;
use crate::i18n::{t, tf};

#[derive(Clone)]
pub struct UpdatesPage {
    pub root: gtk::Box,
    check_button: gtk::Button,
    select_all_button: gtk::Button,
    clear_selection_button: gtk::Button,
    apply_button: gtk::Button,
    apply_selected_button: gtk::Button,
    list: gtk::ListBox,
    status: gtk::Label,
    error_label: gtk::Label,
    generation: Rc<std::cell::Cell<u64>>,
    search: gtk::SearchEntry,
    source_filter: gtk::DropDown,
    rows: Rc<RefCell<Vec<(gtk::CheckButton, TransactionAction, String)>>>,
    all_updates: Rc<RefCell<Vec<(TransactionAction, String)>>>,
}

impl UpdatesPage {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("page-root");
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_hexpand(true);
        root.set_vexpand(true);

        let title = gtk::Label::new(Some(t("Updates")));
        title.add_css_class("title-2");
        title.add_css_class("page-title");
        title.set_xalign(0.0);
        root.append(&title);

        let info = gtk::Label::new(Some(t(
            "Check and apply updates. System upgrades run through the helper.",
        )));
        info.add_css_class("dim-label");
        info.set_wrap(true);
        info.set_xalign(0.0);
        root.append(&info);

        let status = gtk::Label::new(Some(t("No update data")));
        status.add_css_class("dim-label");
        status.set_xalign(0.0);
        root.append(&status);
        let error_label = gtk::Label::new(None);
        error_label.set_wrap(true);
        error_label.set_xalign(0.0);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        root.append(&error_label);

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(t("Filter updates")));
        root.append(&search);
        let source_filter =
            gtk::DropDown::from_strings(&[t("All Sources"), t("Pacman"), t("AUR"), t("Flatpak")]);
        source_filter.set_selected(0);
        root.append(&source_filter);

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.add_css_class("page-controls");
        let check_button = gtk::Button::with_label(t("Check Updates"));
        let select_all_button = gtk::Button::with_label(t("Select All"));
        let clear_selection_button = gtk::Button::with_label(t("Select None"));
        let apply_selected_button = gtk::Button::with_label(t("Update Selected"));
        let apply_button = gtk::Button::with_label(t("Update All"));
        // WinDoesNot: no per-package updates. Updating a subset (e.g. hyprutils alone)
        // is a partial upgrade and breaks plugins built against the exact Hyprland ABI.
        apply_button.add_css_class("suggested-action");
        buttons.append(&check_button);
        buttons.append(&apply_button);
        root.append(&buttons);

        let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header_row.add_css_class("table-header");
        header_row.append(&header_label(t("Package"), true, 0));
        header_row.append(&header_label(t("Source"), false, 10));
        header_row.append(&header_label(t("Action"), false, 10));
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
            check_button,
            select_all_button,
            clear_selection_button,
            apply_button,
            apply_selected_button,
            list,
            status,
            error_label,
            generation: Rc::new(std::cell::Cell::new(0)),
            search,
            source_filter,
            rows: Rc::new(RefCell::new(Vec::new())),
            all_updates: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn bind(&self, ctx: AppContext) {
        self.refresh(ctx.clone(), None);

        let rows_for_select_all = self.rows.clone();
        self.select_all_button.connect_clicked(move |_| {
            for (check, _, _) in rows_for_select_all.borrow().iter() {
                check.set_active(true);
            }
        });

        let rows_for_clear = self.rows.clone();
        self.clear_selection_button.connect_clicked(move |_| {
            for (check, _, _) in rows_for_clear.borrow().iter() {
                check.set_active(false);
            }
        });

        let list = self.list.clone();
        let status = self.status.clone();
        let rows = self.rows.clone();
        let all_updates = self.all_updates.clone();
        let search = self.search.clone();
        let source_filter = self.source_filter.clone();
        let generation = self.generation.clone();
        let error_label = self.error_label.clone();
        self.check_button.connect_clicked(move |_| {
            let generation = generation.clone();
            generation.set(generation.get() + 1);
            let request = generation.get();
            let error_label = error_label.clone();
            let list = list.clone();
            let status = status.clone();
            let rows = rows.clone();
            let all_updates = all_updates.clone();
            let search = search.clone();
            let source_filter = source_filter.clone();
            let ctx = ctx.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                // An explicit check refreshes the search copy regardless of its age.
                if let Err(err) = crate::core::syncdb::refresh_now() {
                    eprintln!("windoesnot-store: update check without fresh database: {err}");
                }
                let items = collect_updates(&ctx);
                let _ = tx.send(items);
            });

            glib::idle_add_local(move || {
                match rx.try_recv() {
                    Ok((items, error)) => {
                        if generation.get() != request { return glib::ControlFlow::Break; }
                        error_label.set_visible(error.is_some());
                        error_label.set_text(error.as_deref().unwrap_or(""));
                        *all_updates.borrow_mut() = items;
                        render_updates(
                            &list,
                            &rows,
                            &all_updates.borrow(),
                            &search.text(),
                            source_filter.selected(),
                            &status,
                        );
                        glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                }
            });
        });

        let list = self.list.clone();
        let rows = self.rows.clone();
        let status = self.status.clone();
        let all_updates = self.all_updates.clone();
        let source_filter = self.source_filter.clone();
        self.search.connect_search_changed(move |entry| {
            let query = entry.text().to_string();
            let items = all_updates.borrow();
            render_updates(
                &list,
                &rows,
                &items,
                &query,
                source_filter.selected(),
                &status,
            );
        });

        let list = self.list.clone();
        let rows = self.rows.clone();
        let status = self.status.clone();
        let all_updates = self.all_updates.clone();
        let search = self.search.clone();
        self.source_filter.connect_selected_notify(move |f| {
            let query = search.text().to_string();
            let items = all_updates.borrow();
            render_updates(&list, &rows, &items, &query, f.selected(), &status);
        });
    }

    pub fn refresh(&self, ctx: AppContext, notify: Option<adw::ToastOverlay>) {
        let generation = self.generation.clone();
        generation.set(generation.get() + 1);
        let request = generation.get();
        let error_label = self.error_label.clone();
        let list = self.list.clone();
        let status = self.status.clone();
        let rows = self.rows.clone();
        let all_updates = self.all_updates.clone();
        let search = self.search.clone();
        let source_filter = self.source_filter.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let items = collect_updates(&ctx);
            let _ = tx.send(items);
        });

        let notify = notify.clone();
        glib::idle_add_local(move || match rx.try_recv() {
            Ok((items, error)) => {
                if generation.get() != request { return glib::ControlFlow::Break; }
                error_label.set_visible(error.is_some());
                error_label.set_text(error.as_deref().unwrap_or(""));
                *all_updates.borrow_mut() = items;
                render_updates(
                    &list,
                    &rows,
                    &all_updates.borrow(),
                    &search.text(),
                    source_filter.selected(),
                    &status,
                );
                if let Some(toasts) = notify.as_ref() {
                    let count = all_updates.borrow().len();
                    if count > 0 {
                        toasts.add_toast(adw::Toast::new(&tf(
                            "{} updates available",
                            &[&count],
                        )));
                    }
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        });
    }

    pub fn connect_apply_all<F: Fn() + 'static>(&self, f: F) {
        self.apply_button.connect_clicked(move |_| f());
    }

    pub fn connect_apply_selected<F: Fn(Vec<TransactionAction>) + 'static>(&self, f: F) {
        let rows = self.rows.clone();
        self.apply_selected_button.connect_clicked(move |_| {
            let selected: Vec<TransactionAction> = rows
                .borrow()
                .iter()
                .filter(|(check, _, _)| check.is_active())
                .map(|(_, action, _)| action.clone())
                .collect();
            f(selected);
        });
    }
}

fn collect_updates(ctx: &AppContext) -> (Vec<(TransactionAction, String)>, Option<String>) {
    let mut error = None;
    let mut items = Vec::new();
    items.extend(collect_pacman_updates());
    if ctx.settings.lock().map(|s| s.aur_available()).unwrap_or(false) {
        match collect_aur_updates(ctx) {
            Ok(aur) => items.extend(aur),
            Err(_) => error = Some(t("AUR updates could not be checked. Please try again later.").to_string()),
        }
    }
    items.extend(collect_flatpak_updates());
    (items, error)
}

fn render_updates(
    list: &gtk::ListBox,
    rows: &Rc<RefCell<Vec<(gtk::CheckButton, TransactionAction, String)>>>,
    items: &[(TransactionAction, String)],
    query: &str,
    source_filter_idx: u32,
    status: &gtk::Label,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    rows.borrow_mut().clear();

    let q = query.trim().to_lowercase();
    let filtered: Vec<(TransactionAction, String)> = items
        .iter()
        .filter(|(action, _)| match source_filter_idx {
            1 => action.source == PackageSource::Repo,
            2 => action.source == PackageSource::Aur,
            3 => action.source == PackageSource::Flatpak,
            _ => true,
        })
        .filter(|(_, display)| q.is_empty() || display.to_lowercase().contains(&q))
        .cloned()
        .collect();

    if filtered.is_empty() {
        if items.is_empty() {
            status.set_text(t("System is up to date"));
        } else {
            status.set_text(t("No updates match selected filters"));
        }
        return;
    }

    status.set_text(&tf(
        "{} updates shown ({} total)",
        &[&filtered.len(), &items.len()],
    ));
    for (action, display) in filtered {
        let check = gtk::CheckButton::new();
        check.set_active(true);
        check.set_visible(false);
        check.set_margin_end(2);

        let name_col = gtk::Box::new(gtk::Orientation::Vertical, 2);
        name_col.set_hexpand(true);

        let name = gtk::Label::new(Some(&action.name));
        name.set_xalign(0.0);
        name.add_css_class("title-5");

        let detail = gtk::Label::new(Some(&display));
        detail.set_xalign(0.0);
        detail.add_css_class("dim-label");
        detail.add_css_class("table-subtext");
        detail.set_wrap(true);

        let source_badge = gtk::Label::new(Some(match action.source {
            PackageSource::Repo => "Pacman",
            PackageSource::Aur => "AUR",
            PackageSource::Flatpak => "Flatpak",
        }));
        source_badge.add_css_class("pill");
        source_badge.set_width_chars(9);

        // Every row on this page is an available update, whatever the queued action kind.
        let mode_badge = gtk::Label::new(Some(match action.kind {
            ActionKind::Remove => "Remove",
            ActionKind::Install | ActionKind::Upgrade => "Update",
        }));
        mode_badge.add_css_class("pill-secondary");
        mode_badge.set_width_chars(9);

        name_col.append(&name);
        name_col.append(&detail);

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row_box.add_css_class("update-row-inner");
        row_box.set_margin_top(6);
        row_box.set_margin_bottom(6);
        row_box.set_margin_start(6);
        row_box.set_margin_end(6);
        row_box.append(&check);
        row_box.append(&name_col);
        row_box.append(&source_badge);
        row_box.append(&mode_badge);

        let row = gtk::ListBoxRow::new();
        row.add_css_class("update-row");
        row.set_child(Some(&row_box));
        list.append(&row);

        rows.borrow_mut().push((check, action, display));
    }
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

fn collect_pacman_updates() -> Vec<(TransactionAction, String)> {
    // Compare against the user's sync copy, like checkupdates; -Qu on the system
    // databases would only see what the last full upgrade downloaded.
    let output = Command::new("pacman")
        .args(["-Qu", "--dbpath"])
        .arg(crate::core::syncdb::query_dbpath())
        .env("LC_ALL", "C")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let name = line.split_whitespace().next().unwrap_or("").to_string();
            (
                TransactionAction {
                    name,
                    source: PackageSource::Repo,
                    kind: ActionKind::Install,
                    origin: None,
                },
                line.to_string(),
            )
        })
        .collect()
}

fn collect_aur_updates(ctx: &AppContext) -> anyhow::Result<Vec<(TransactionAction, String)>> {
    let helper = ctx.settings.lock().map(|s| s.aur_helper).unwrap_or(AurHelperKind::Yay);
    let Some(helper) = helper.installed() else { return Ok(Vec::new()); };
    let updates = crate::core::status::pending_aur_updates(helper.as_str())?;
    Ok(updates.into_iter().map(|update| {
        let display = format!("{} {} → {} (AUR)", update.name, update.old, update.new);
        (TransactionAction { name: update.name, source: PackageSource::Aur,
            kind: ActionKind::Install, origin: None }, display)
    }).collect())
}

fn collect_flatpak_updates() -> Vec<(TransactionAction, String)> {
    let output = Command::new("flatpak")
        .args([
            "remote-ls",
            "--updates",
            "--columns=application,version,branch,remote",
        ])
        .env("LC_ALL", "C")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    let mut items = Vec::new();
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let app_id = cols.get(0).unwrap_or(&"").trim().to_string();
        if app_id.is_empty() {
            continue;
        }
        let version = cols.get(1).unwrap_or(&"").trim();
        let branch = cols.get(2).unwrap_or(&"").trim();
        let remote = cols.get(3).unwrap_or(&"").trim();
        let mut display = app_id.clone();
        if !version.is_empty() {
            display.push_str(&format!(" {version}"));
        } else if !branch.is_empty() {
            display.push_str(&format!(" {branch}"));
        }
        if !remote.is_empty() {
            display.push_str(&format!(" ({remote})"));
        }
        display.push_str(" [Flatpak]");

        items.push((
            TransactionAction {
                name: app_id,
                source: PackageSource::Flatpak,
                kind: ActionKind::Upgrade,
                origin: None,
            },
            display,
        ));
    }
    items
}
