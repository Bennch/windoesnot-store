use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;

use gtk::prelude::*;
use libadwaita as adw;

use crate::core::models::{ActionKind, AurHelperKind, PackageSource, TransactionAction};
use crate::core::installation::Installation;
use crate::ui::AppContext;
use crate::i18n::{t, tf};

/// Queues the actions behind a group header's button.
type GroupCallback = Rc<RefCell<Option<Rc<dyn Fn(Vec<TransactionAction>)>>>>;

#[derive(Clone)]
pub struct UpdatesPage {
    pub root: gtk::Box,
    select_all_button: gtk::Button,
    clear_selection_button: gtk::Button,
    apply_button: gtk::Button,
    apply_selected_button: gtk::Button,
    list: gtk::ListBox,
    status: gtk::Label,
    spinner: gtk::Spinner,
    error_label: gtk::Label,
    generation: Rc<std::cell::Cell<u64>>,
    search: gtk::SearchEntry,
    source_filter: gtk::DropDown,
    topic_filter: gtk::DropDown,
    topics: Rc<RefCell<HashMap<(PackageSource, String), Info>>>,
    group_callback: GroupCallback,
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
        let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        status_row.append(&spinner);
        status_row.append(&status);
        root.append(&status_row);
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
            gtk::DropDown::from_strings(&[t("Everything"), t("System (Pacman)"), t("AUR"), t("Programs (Flatpak)")]);
        source_filter.set_selected(0);
        root.append(&source_filter);
        let topic_filter = gtk::DropDown::from_strings(&[
            t("All topics"), t("Programs"), t("System & drivers"), t("Tools"), t("Libraries"),
        ]);
        topic_filter.set_selected(0);
        root.append(&topic_filter);

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.add_css_class("page-controls");
        let select_all_button = gtk::Button::with_label(t("Select All"));
        let clear_selection_button = gtk::Button::with_label(t("Select None"));
        let apply_selected_button = gtk::Button::with_label(t("Update Selected"));
        let apply_button = gtk::Button::with_label(t("Update All"));
        // WinDoesNot: no per-package updates. Updating a subset (e.g. hyprutils alone)
        // is a partial upgrade and breaks plugins built against the exact Hyprland ABI.
        apply_button.add_css_class("suggested-action");
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
            select_all_button,
            clear_selection_button,
            apply_button,
            apply_selected_button,
            list,
            status,
            spinner,
            error_label,
            generation: Rc::new(std::cell::Cell::new(0)),
            search,
            source_filter,
            topic_filter,
            topics: Rc::new(RefCell::new(HashMap::new())),
            group_callback: Rc::new(RefCell::new(None)),
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
        let rows = self.rows.clone();
        let status = self.status.clone();
        let all_updates = self.all_updates.clone();
        let source_filter = self.source_filter.clone();
        let topic_filter = self.topic_filter.clone();
        let topics = self.topics.clone();
        let group_callback = self.group_callback.clone();
        self.search.connect_search_changed(move |entry| {
            let query = entry.text().to_string();
            let items = all_updates.borrow();
            render_updates(
                &list,
                &rows,
                &items,
                &query,
                source_filter.selected(),
                topic_filter.selected(),
                &topics.borrow(),
                &group_callback,
                &status,
            );
        });

        let list = self.list.clone();
        let rows = self.rows.clone();
        let status = self.status.clone();
        let all_updates = self.all_updates.clone();
        let search = self.search.clone();
        let apply_button = self.apply_button.clone();
        let topic_filter = self.topic_filter.clone();
        let topics = self.topics.clone();
        let group_callback = self.group_callback.clone();
        self.source_filter.connect_selected_notify(move |f| {
            apply_button.set_label(update_button_label(f.selected()));
            let query = search.text().to_string();
            let items = all_updates.borrow();
            render_updates(&list, &rows, &items, &query, f.selected(), topic_filter.selected(), &topics.borrow(), &group_callback, &status);
        });

        let list = self.list.clone();
        let rows = self.rows.clone();
        let status = self.status.clone();
        let all_updates = self.all_updates.clone();
        let search = self.search.clone();
        let source_filter = self.source_filter.clone();
        let topics = self.topics.clone();
        let group_callback = self.group_callback.clone();
        self.topic_filter.connect_selected_notify(move |f| {
            let query = search.text().to_string();
            let items = all_updates.borrow();
            render_updates(&list, &rows, &items, &query, source_filter.selected(), f.selected(), &topics.borrow(), &group_callback, &status);
        });
    }

    pub fn refresh(&self, ctx: AppContext, notify: Option<adw::ToastOverlay>) {
        self.spinner.set_visible(true);
        self.spinner.start();
        self.status.set_text(t("Checking for updates…"));
        let generation = self.generation.clone();
        generation.set(generation.get() + 1);
        let request = generation.get();
        let error_label = self.error_label.clone();
        let list = self.list.clone();
        let status = self.status.clone();
        let spinner = self.spinner.clone();
        let rows = self.rows.clone();
        let all_updates = self.all_updates.clone();
        let search = self.search.clone();
        let source_filter = self.source_filter.clone();
        let topic_filter = self.topic_filter.clone();
        let topics = self.topics.clone();
        let group_callback = self.group_callback.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Refresh the search copy regardless of its age: this runs every
            // time the Updates page is shown, so a stale cache would mean a
            // visibly wrong "no updates" right after one was just installed.
            if let Err(err) = crate::core::syncdb::refresh_now() {
                eprintln!("windoesnot-store: update check without fresh database: {err}");
            }
            let (items, error) = collect_updates(&ctx);
            let sorted = classify(&items);
            let _ = tx.send((items, error, sorted));
        });

        let notify = notify.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(40), move || match rx.try_recv() {
            Ok((items, error, sorted)) => {
                if generation.get() != request { return glib::ControlFlow::Break; }
                spinner.stop();
                spinner.set_visible(false);
                error_label.set_visible(error.is_some());
                error_label.set_text(error.as_deref().unwrap_or(""));
                *all_updates.borrow_mut() = items;
                *topics.borrow_mut() = sorted;
                render_updates(
                    &list,
                    &rows,
                    &all_updates.borrow(),
                    &search.text(),
                    source_filter.selected(),
                    topic_filter.selected(),
                    &topics.borrow(),
                    &group_callback,
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
            Err(mpsc::TryRecvError::Disconnected) => {
                if generation.get() == request {
                    spinner.stop();
                    spinner.set_visible(false);
                }
                glib::ControlFlow::Break
            },
        });
    }

    /// The button updates what the source filter shows: everything, or one source.
    /// Pacman packages are never split further; they only work as a set.
    pub fn connect_apply_all<F: Fn(Option<PackageSource>) + 'static>(&self, f: F) {
        let filter = self.source_filter.clone();
        self.apply_button.connect_clicked(move |_| {
            f(match filter.selected() {
                1 => Some(PackageSource::Repo),
                2 => Some(PackageSource::Aur),
                3 => Some(PackageSource::Flatpak),
                _ => None,
            })
        });
    }

    /// Called with the actions behind a group header's update button.
    pub fn connect_group_update<F: Fn(Vec<TransactionAction>) + 'static>(&self, f: F) {
        *self.group_callback.borrow_mut() = Some(Rc::new(f));
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

pub(crate) fn collect_updates(ctx: &AppContext) -> (Vec<(TransactionAction, String)>, Option<String>) {
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

/// Match the actual installed package or Flatpak ID, including alternate
/// sources for an app shown on a repository detail page.
pub(crate) fn update_for_installations(
    updates: &[(TransactionAction, String)],
    installed: &[Installation],
) -> Option<(TransactionAction, String)> {
    updates.iter().find_map(|(action, description)| {
        installed.iter().any(|item| item.name == action.name && item.source == action.source)
            .then(|| (action.clone(), description.clone()))
    })
}

fn render_updates(
    list: &gtk::ListBox,
    rows: &Rc<RefCell<Vec<(gtk::CheckButton, TransactionAction, String)>>>,
    items: &[(TransactionAction, String)],
    query: &str,
    source_filter_idx: u32,
    topic_idx: u32,
    topics: &HashMap<(PackageSource, String), Info>,
    group_callback: &GroupCallback,
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
        .filter(|(action, _)| {
            topic_idx == 0
                || topics.get(&(action.source, action.name.clone())).map_or(false, |info| info.topic as u32 + 1 == topic_idx)
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
    // Group under topic headers, alphabetical within a group.
    let topic_of = |action: &TransactionAction| topics.get(&(action.source, action.name.clone())).map_or(Topic::Tools, |info| info.topic);
    let mut filtered = filtered;
    filtered.sort_by_key(|(action, _)| (topic_of(action) as u32, action.name.to_lowercase()));
    let mut counts = [0usize; 4];
    let mut members: [Vec<TransactionAction>; 4] = Default::default();
    for (action, _) in &filtered {
        counts[topic_of(action) as usize] += 1;
        members[topic_of(action) as usize].push(action.clone());
    }
    let mut current: Option<Topic> = None;
    for (action, display) in filtered {
        let topic = topic_of(&action);
        if current != Some(topic) {
            current = Some(topic);
            let title = topic.title();
            let label = gtk::Label::new(Some(&format!("{title} ({})", counts[topic as usize])));
            label.add_css_class("title-4");
            label.set_xalign(0.0);
            label.set_hexpand(true);
            let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            header.set_margin_top(14);
            header.set_margin_bottom(4);
            header.set_margin_start(6);
            header.set_margin_end(6);
            header.append(&label);

            // Flatpaks update one by one. Pacman and AUR packages only work as a set,
            // so a group containing them queues the whole pacman/AUR update.
            let group = &members[topic as usize];
            let mut queue_actions: Vec<TransactionAction> = group
                .iter()
                .filter(|a| a.source == PackageSource::Flatpak)
                .map(|a| TransactionAction { kind: ActionKind::Upgrade, ..a.clone() })
                .collect();
            for (source, name) in [(PackageSource::Repo, "system"), (PackageSource::Aur, "aur")] {
                if group.iter().any(|a| a.source == source) {
                    queue_actions.push(TransactionAction { name: name.into(), source, kind: ActionKind::Upgrade, origin: None });
                }
            }
            let button = gtk::Button::with_label(&tf("Update {}", &[&title]));
            if group.iter().any(|a| a.source != PackageSource::Flatpak) {
                button.set_tooltip_text(Some(t("System packages can only be updated together, so this updates the whole system.")));
            }
            let callback = group_callback.clone();
            button.connect_clicked(move |_| {
                if let Some(f) = callback.borrow().as_ref() {
                    f(queue_actions.clone());
                }
            });
            header.append(&button);
            let header_row = gtk::ListBoxRow::new();
            header_row.set_selectable(false);
            header_row.set_activatable(false);
            header_row.set_child(Some(&header));
            list.append(&header_row);
        }
        let check = gtk::CheckButton::new();
        check.set_active(true);
        check.set_visible(false);
        check.set_margin_end(2);

        let icon = package_icon(topics.get(&(action.source, action.name.clone())).and_then(|i| i.icon.as_deref()), &action.name);

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
        row_box.append(&icon);
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

/// Topic and launcher icon (theme name or absolute file) of one pending update.
#[derive(Clone, Debug)]
pub struct Info {
    pub topic: Topic,
    pub icon: Option<String>,
}

/// What an update is for. Display-only: pacman updates still go in as one set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Topic {
    Programs = 0,
    System = 1,
    Tools = 2,
    Libraries = 3,
}

fn is_system_name(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "linux", "nvidia", "mesa", "lib32-mesa", "vulkan", "lib32-vulkan", "systemd", "mkinitcpio",
        "grub", "networkmanager", "pipewire", "wireplumber", "xdg-desktop-portal", "cachyos",
        "chwd", "glibc", "btrfs", "xfsprogs", "lvm2", "device-mapper", "udev", "intel-", "amd-",
    ];
    name.contains("firmware") || PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

/// Sort pending updates by topic from local data: launcher files, install reason, names.
fn classify(items: &[(TransactionAction, String)]) -> HashMap<(PackageSource, String), Info> {
    classify_packages(items.iter().map(|(action, _)| (action.source, action.name.as_str())))
}

impl Topic {
    pub fn title(self) -> &'static str {
        match self {
            Topic::Programs => t("Programs"),
            Topic::System => t("System & drivers"),
            Topic::Tools => t("Tools"),
            Topic::Libraries => t("Libraries"),
        }
    }
}

/// Same sorting for any list of packages (updates, installed).
pub(crate) fn classify_packages<'a>(
    items: impl Iterator<Item = (PackageSource, &'a str)>,
) -> HashMap<(PackageSource, String), Info> {
    let run = |args: &[&str]| {
        Command::new("pacman").args(args).env("LC_ALL", "C").output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default()
    };
    // One call for all packages: "pkg /usr/share/applications/x.desktop".
    let mut launcher: HashMap<String, String> = HashMap::new();
    for line in run(&["-Ql"]).lines() {
        if line.contains(" /usr/share/applications/") && line.ends_with(".desktop") {
            if let Some((pkg, path)) = line.split_once(' ') {
                launcher.entry(pkg.to_owned()).or_insert_with(|| path.to_owned());
            }
        }
    }
    let as_dependency: HashSet<String> = run(&["-Qdq"]).lines().map(str::to_owned).collect();

    items
        .map(|(source, name)| {
            let topic = match source {
                PackageSource::Flatpak => {
                    let runtime = ["Platform", "Sdk", ".Locale", ".Codecs", ".GL.", ".Extension", ".Gtk3theme"]
                        .iter().any(|part| name.contains(part));
                    if runtime { Topic::Libraries } else { Topic::Programs }
                }
                _ if launcher.contains_key(name) => Topic::Programs,
                _ if is_system_name(name) => Topic::System,
                _ if as_dependency.contains(name) => Topic::Libraries,
                _ => Topic::Tools,
            };
            let icon = match source {
                PackageSource::Flatpak => Some(name.to_owned()),
                _ => launcher
                    .get(name)
                    .and_then(|path| std::fs::read_to_string(path).ok())
                    .and_then(|text| {
                        text.lines().find_map(|l| l.strip_prefix("Icon=").map(|v| v.trim().to_owned()))
                    })
                    .or_else(|| Some(name.to_owned())),
            };
            ((source, name.to_owned()), Info { topic, icon })
        })
        .collect()
}

/// The app's icon if the theme (or a file path) has it, else a generic package.
fn package_icon(wanted: Option<&str>, name: &str) -> gtk::Image {
    let theme = gtk::gdk::Display::default().map(|d| gtk::IconTheme::for_display(&d));
    let image = match wanted {
        Some(path) if path.starts_with('/') && std::path::Path::new(path).is_file() => gtk::Image::from_file(path),
        _ => {
            let found = [wanted, Some(name)]
                .into_iter()
                .flatten()
                .find(|icon| !icon.starts_with('/') && theme.as_ref().map_or(false, |t| t.has_icon(icon)));
            gtk::Image::from_icon_name(found.unwrap_or("package-x-generic-symbolic"))
        }
    };
    image.set_pixel_size(32);
    image.set_margin_end(6);
    image
}

fn update_button_label(filter: u32) -> &'static str {
    match filter {
        1 => t("Update System"),
        2 => t("Update AUR"),
        3 => t("Update Programs"),
        _ => t("Update All"),
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

#[cfg(test)]
mod detail_tests {
    use super::*;

    #[test]
    fn detail_update_matches_installed_name_and_source() {
        let updates = vec![
            (TransactionAction { name: "gimp".into(), source: PackageSource::Repo,
                kind: ActionKind::Install, origin: None }, "gimp 3.0 → 3.1".into()),
            (TransactionAction { name: "org.gimp.GIMP".into(), source: PackageSource::Flatpak,
                kind: ActionKind::Upgrade, origin: None }, "org.gimp.GIMP 3.1".into()),
        ];
        let installed = [Installation { name: "org.gimp.GIMP".into(),
            source: PackageSource::Flatpak, version: "3.0".into() }];
        let (action, _) = update_for_installations(&updates, &installed).unwrap();
        assert_eq!(action.name, "org.gimp.GIMP");
        assert_eq!(action.source, PackageSource::Flatpak);
    }
}
