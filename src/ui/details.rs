use std::sync::Arc;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use gtk::gio;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

use crate::core::appstream::AppStreamClient;
use crate::core::models::{PackageDetails, PackageSource, PackageSummary, TransactionAction};
use crate::core::sources::{self, Lookups, SourceOption};
use crate::ui::{updates, AppContext, UiHandles};
use crate::core::installation::Installation;
use crate::ui::widgets::app_icon::app_icon;
use crate::ui::widgets::screenshot_carousel::ScreenshotCarousel;
use crate::i18n::{t, tf};

/// `flatpak_id` links a package to its Flatpak where the caller knows it
/// (curated home apps), so either installation is recognised.
pub fn show_details(ctx: &AppContext, handles: &UiHandles, summary: PackageSummary, flatpak_id: Option<String>) {
    let page = adw::NavigationPage::builder()
        .title(&summary.name)
        .build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.set_hexpand(true);
    root.set_vexpand(true);

    let back_btn = gtk::Button::from_icon_name("go-previous-symbolic");
    back_btn.add_css_class("flat");
    back_btn.set_halign(gtk::Align::Start);
    back_btn.set_tooltip_text(Some(t("Back")));

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    // Same icon as the tile that opened this page.
    let flathub_id = (summary.source == PackageSource::Flatpak).then_some(summary.name.as_str());
    // The official Flathub logo replaces this once it is known.
    let icon = gtk::Box::new(gtk::Orientation::Vertical, 0);
    icon.append(&app_icon(&summary.name, flathub_id, 96));
    icon.set_valign(gtk::Align::Start);

    let text_col = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let name = gtk::Label::new(Some(&summary.name));
    name.add_css_class("title-1");
    name.set_xalign(0.0);

    let summary_label = gtk::Label::new(Some(&summary.summary));
    summary_label.set_xalign(0.0);
    summary_label.set_wrap(true);

    let badges = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let source_badge = gtk::Label::new(Some(match summary.source {
        PackageSource::Repo => "Pacman",
        PackageSource::Aur => "AUR",
        PackageSource::Flatpak => "Flatpak",
    }));
    source_badge.add_css_class("pill");
    badges.append(&source_badge);

    text_col.append(&name);
    text_col.append(&summary_label);
    text_col.append(&badges);

    header.append(&icon);
    header.append(&text_col);

    let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    // Where the app is installed (any source); decides install vs. remove.
    let installations: Rc<RefCell<Vec<Installation>>> = Rc::new(RefCell::new(Vec::new()));
    let action_btn = gtk::Button::with_label(t("Install"));
    action_btn.add_css_class("suggested-action");
    // Enabled once the installed state is known, so nothing is installed twice.
    action_btn.set_sensitive(false);
    let open_home_btn = gtk::Button::with_label(t("Open Homepage"));
    open_home_btn.set_visible(false);
    let logs_btn = gtk::Button::with_label(t("View Logs"));
    let update_btn = gtk::Button::with_label(t("Update"));
    update_btn.add_css_class("suggested-action");
    update_btn.set_visible(false);
    let pending_update: Rc<RefCell<Option<TransactionAction>>> = Rc::new(RefCell::new(None));
    button_row.append(&action_btn);
    button_row.append(&update_btn);
    button_row.append(&open_home_btn);
    button_row.append(&logs_btn);

    let carousel = ScreenshotCarousel::new();

    // Facts as a tidy list instead of loose lines.
    let details = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let info_group = adw::PreferencesGroup::new();
    info_group.set_title(t("Information"));
    let info_row = |title: &str, hidden: bool| {
        let row = adw::ActionRow::builder().title(title).subtitle("–").build();
        row.add_css_class("property");
        row.set_subtitle_selectable(true);
        row.set_visible(!hidden);
        info_group.add(&row);
        row
    };
    let version = info_row(t("Version"), false);
    let installed = info_row(t("Installed"), false);
    let size = info_row(t("Size"), false);
    let homepage = info_row(t("Website"), false);
    let developer = info_row(t("Developer"), true);
    let license = info_row(t("License"), true);
    let categories = info_row(t("Categories"), true);
    details.append(&info_group);

    let update_status = gtk::Label::new(None);
    update_status.set_xalign(0.0);
    update_status.set_wrap(true);
    update_status.set_visible(false);
    details.append(&update_status);

    let update_loading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let update_spinner = gtk::Spinner::new();
    update_loading.append(&update_spinner);
    update_loading.append(&gtk::Label::new(Some(t("Checking for updates…"))));
    update_loading.set_visible(false);
    details.append(&update_loading);

    let loading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let spinner = gtk::Spinner::new();
    spinner.start();
    loading.append(&spinner);
    loading.append(&gtk::Label::new(Some(t("Loading package information…"))));

    let description_heading = gtk::Label::new(Some(t("Description")));
    description_heading.add_css_class("title-4");
    description_heading.set_xalign(0.0);
    description_heading.set_visible(false);
    let description = gtk::Label::new(Some(""));
    description.set_xalign(0.0);
    description.set_wrap(true);
    description.set_selectable(true);
    description.set_use_markup(true);
    description.set_visible(false);

    // Every source that offers this app; the install button uses the chosen one.
    let sources_heading = gtk::Label::new(Some(t("Install from")));
    sources_heading.add_css_class("heading");
    sources_heading.set_xalign(0.0);
    sources_heading.set_visible(false);
    let sources_list = gtk::ListBox::new();
    sources_list.add_css_class("boxed-list");
    sources_list.set_selection_mode(gtk::SelectionMode::None);
    sources_list.set_visible(false);
    let chosen: Rc<RefCell<Option<SourceOption>>> = Rc::new(RefCell::new(None));
    let available_sources: Rc<RefCell<Vec<SourceOption>>> = Rc::new(RefCell::new(Vec::new()));

    root.append(&back_btn);
    root.append(&header);
    root.append(&sources_heading);
    root.append(&sources_list);
    root.append(&button_row);
    // Installing a native package always updates the whole system: say so before the click.
    let system_note = gtk::Label::new(Some(t("Installing this also updates the whole system.")));
    system_note.add_css_class("warning");
    system_note.add_css_class("heading");
    system_note.set_xalign(0.0);
    system_note.set_wrap(true);
    system_note.set_visible(false);
    root.append(&system_note);
    // Shown when this app has an update; says plainly when a system update is needed.
    let update_banner = adw::Banner::new("");
    update_banner.set_revealed(false);
    root.append(&update_banner);
    root.append(&loading);
    root.append(carousel.widget());
    root.append(&description_heading);
    root.append(&description);
    root.append(&details);

    // Scrollable and capped in width so long texts stay readable.
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(960);
    clamp.set_child(Some(&root));
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_child(Some(&clamp));
    page.set_child(Some(&scroller));
    handles.nav_view.push(&page);

    let nav = handles.nav_view.clone();
    back_btn.connect_clicked(move |_| {
        nav.pop();
    });

    let ctx_clone = ctx.clone();
    let summary_clone = summary.clone();
    let summary_label_clone = summary_label.clone();
    let version_clone = version.clone();
    let installed_clone = installed.clone();
    let size_clone = size.clone();
    let description_clone = description.clone();
    let description_heading_clone = description_heading.clone();
    let carousel_clone = carousel.clone();
    let action_btn_clone = action_btn.clone();
    let update_loading_for_details = update_loading.clone();
    let update_spinner_for_details = update_spinner.clone();
    let update_btn_for_status = update_btn.clone();
    let update_banner_for_status = update_banner.clone();
    let pending_update_for_status = pending_update.clone();
    let loading_clone = loading.clone();
    let spinner_clone = spinner.clone();
    let open_home_btn_clone = open_home_btn.clone();
    let homepage_clone = homepage.clone();
    let installations_clone = installations.clone();
    let installations_clone_for_status = installations.clone();
    let home_url = Rc::new(RefCell::new(None::<String>));
    let home_url_clone = home_url.clone();
    let appstream = ctx.appstream.clone();
    let (name_clone, developer_clone, license_clone, categories_clone) = (name.clone(), developer.clone(), license.clone(), categories.clone());
    let icon_slot = icon.clone();

    let summary_source = summary.source;
    let flatpak_id_for_refresh = flatpak_id.clone();
    let sources_heading_clone = sources_heading.clone();
    let sources_list_clone = sources_list.clone();
    let chosen_clone = chosen.clone();
    let available_sources_for_load = available_sources.clone();
    let system_note_clone = system_note.clone();
    let (tx, rx) = mpsc::channel();
    let (update_tx, update_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let found = ctx_clone.installed_apps().find(&summary_clone.name, flatpak_id.as_deref());
        let options = source_options(&ctx_clone, &summary_clone, flatpak_id.as_deref());
        let (details, info) = load_details(ctx_clone.clone(), summary_clone, appstream, flatpak_id.as_deref());
        // Waiting system updates decide which source is worth recommending (see fill_sources).
        let pending = if found.is_empty() { updates::collect_pacman_updates().len() } else { 0 };
        let _ = tx.send((details, found.clone(), options, info, pending));
        let update = if found.is_empty() {
            None
        } else {
            let (available, _) = updates::collect_updates(&ctx_clone);
            updates::update_for_installations(&available, &found)
        };
        let _ = update_tx.send(update);
    });

    glib::timeout_add_local(std::time::Duration::from_millis(40), move || {
        match rx.try_recv() {
            Ok((details, found, options, info, pending)) => {
                let info = info.unwrap_or_default();
                spinner_clone.stop();
                loading_clone.set_visible(false);
                *available_sources_for_load.borrow_mut() = options.clone();
                fill_sources(&sources_list_clone, &options, &found, summary_source, &chosen_clone, &system_note_clone, pending);
                // Installed: the button removes, so choosing a source is moot.
                let pick_source = found.is_empty() && !options.is_empty();
                sources_heading_clone.set_visible(pick_source);
                sources_list_clone.set_visible(pick_source);
                summary_label_clone.set_text(&details.summary);
                version_clone.set_subtitle(&markup_escape(if details.version.is_empty() { "–" } else { &details.version }));
                installed_clone.set_subtitle(&markup_escape(&installed_value(&found)));
                action_btn_clone.set_label(&match preferred(&found, summary_source) {
                    Some(installation) => tf("Remove ({})", &[&installation.source.label()]),
                    None => t("Install").to_string(),
                });
                action_btn_clone.set_sensitive(true);
                if !found.is_empty() {
                    update_loading_for_details.set_visible(true);
                    update_spinner_for_details.start();
                }
                *installations_clone.borrow_mut() = found;
                if let Some(size) = &details.size {
                    size_clone.set_subtitle(&markup_escape(size));
                }
                if let Some(home) = details.home.clone().or_else(|| info.homepage.clone()) {
                    let shown = markup_escape(&home);
                    homepage_clone.set_subtitle(&format!("<a href=\"{shown}\">{shown}</a>"));
                    *home_url_clone.borrow_mut() = Some(home);
                    open_home_btn_clone.set_visible(true);
                } else {
                    homepage_clone.set_subtitle("–");
                    *home_url_clone.borrow_mut() = None;
                    open_home_btn_clone.set_visible(false);
                }
                if let Some(path) = &info.icon_path {
                    if let Ok(texture) = gtk::gdk::Texture::from_filename(path) {
                        let image = gtk::Image::from_paintable(Some(&texture));
                        image.set_pixel_size(96);
                        while let Some(child) = icon_slot.first_child() {
                            icon_slot.remove(&child);
                        }
                        icon_slot.append(&image);
                    }
                }
                if let Some(title) = &info.title {
                    name_clone.set_text(title);
                }
                if let Some(summary) = &info.summary {
                    summary_label_clone.set_text(summary);
                }
                match &info.description {
                    Some(markup) => description_clone.set_markup(markup),
                    None => description_clone.set_markup(&markup_escape(&details.description)),
                }
                let has_description = !description_clone.text().trim().is_empty();
                description_clone.set_visible(has_description);
                description_heading_clone.set_visible(has_description);
                for (row, value) in [
                    (&developer_clone, info.developer.clone()),
                    (&license_clone, info.license.clone()),
                    (&categories_clone, (!info.categories.is_empty()).then(|| info.categories.join(", "))),
                ] {
                    row.set_visible(value.is_some());
                    if let Some(value) = value {
                        row.set_subtitle(&markup_escape(&value));
                    }
                }
                carousel_clone.set_screenshots(details.screenshots.clone());
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                spinner_clone.stop();
                loading_clone.set_visible(false);
                glib::ControlFlow::Break
            }
        }
    });

    let update_status_for_initial = update_status.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(40), move || {
        match update_rx.try_recv() {
            Ok(update) => {
                update_spinner.stop();
                update_loading.set_visible(false);
                if let Some((action, description)) = update {
                    let message = tf("Update available: {}", &[&description]);
                    if action.source == PackageSource::Flatpak {
                        update_banner_for_status.set_title(&message);
                        *pending_update_for_status.borrow_mut() = Some(action);
                    } else {
                        // Pacman and AUR packages only update with the whole system.
                        update_banner_for_status.set_title(&format!(
                            "{}\n{}",
                            message,
                            t("A system update is required: this package can only be updated together with the whole system.")
                        ));
                        update_btn_for_status.set_label(t("Start system update"));
                        let (name, source) = if action.source == PackageSource::Aur { ("aur", PackageSource::Aur) } else { ("system", PackageSource::Repo) };
                        *pending_update_for_status.borrow_mut() = Some(TransactionAction {
                            name: name.into(),
                            source,
                            kind: crate::core::models::ActionKind::Upgrade,
                            origin: None,
                        });
                    }
                    update_banner_for_status.set_revealed(true);
                    update_btn_for_status.set_visible(true);
                } else if !installations_clone_for_status.borrow().is_empty() {
                    update_status_for_initial.set_text(t("No update found in current package data"));
                    update_status_for_initial.set_visible(true);
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                update_spinner.stop();
                update_loading.set_visible(false);
                glib::ControlFlow::Break
            }
        }
    });

    // The page can stay open while its queued install finishes. Re-read the
    // package database instead of keeping the state captured when it opened.
    let page_for_refresh = page.downgrade();
    let ctx_for_refresh = ctx.clone();
    let name_for_refresh = summary.name.clone();
    let installations_for_refresh = installations.clone();
    let action_for_refresh = action_btn.clone();
    let installed_for_refresh = installed.clone();
    let sources_heading_for_refresh = sources_heading.clone();
    let sources_list_for_refresh = sources_list.clone();
    let system_note_for_refresh = system_note.clone();
    let available_sources_for_refresh = available_sources.clone();
    let chosen_for_refresh = chosen.clone();
    let update_btn_for_refresh = update_btn.clone();
    let update_banner_for_refresh = update_banner.clone();
    let update_status_for_refresh = update_status.clone();
    let pending_update_for_refresh = pending_update.clone();
    handles.queue.connect_finished(move || {
        if page_for_refresh.upgrade().is_none() { return; }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx_for_refresh.clone();
        let name = name_for_refresh.clone();
        let flatpak_id = flatpak_id_for_refresh.clone();
        std::thread::spawn(move || {
            let found = ctx.installed_apps().find(&name, flatpak_id.as_deref());
            let pending = if found.is_empty() { updates::collect_pacman_updates().len() } else { 0 };
            let _ = tx.send((found, pending));
        });
        let installations = installations_for_refresh.clone();
        let action = action_for_refresh.clone();
        let installed = installed_for_refresh.clone();
        let sources_heading = sources_heading_for_refresh.clone();
        let sources_list = sources_list_for_refresh.clone();
        let system_note = system_note_for_refresh.clone();
        let available_sources = available_sources_for_refresh.clone();
        let chosen = chosen_for_refresh.clone();
        let update_btn = update_btn_for_refresh.clone();
        let update_banner = update_banner_for_refresh.clone();
        let update_status = update_status_for_refresh.clone();
        let pending_update = pending_update_for_refresh.clone();
        let page = page_for_refresh.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || match rx.try_recv() {
            Ok((found, pending)) => {
                if page.upgrade().is_some() {
                    installed.set_subtitle(&markup_escape(&installed_value(&found)));
                    action.set_label(&match preferred(&found, summary_source) {
                        Some(installation) => tf("Remove ({})", &[&installation.source.label()]),
                        None => t("Install").to_string(),
                    });
                    let options = available_sources.borrow();
                    fill_sources(&sources_list, &options, &found, summary_source, &chosen, &system_note, pending);
                    let choose_source = found.is_empty() && !options.is_empty();
                    sources_heading.set_visible(choose_source);
                    sources_list.set_visible(choose_source);
                    if found.is_empty() {
                        update_btn.set_visible(false);
                        update_banner.set_revealed(false);
                        update_status.set_visible(false);
                        *pending_update.borrow_mut() = None;
                    }
                    *installations.borrow_mut() = found;
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        });
    });

    let queue = handles.queue.clone();
    let pkg_name = summary.name.clone();
    let pkg_source = summary.source;
    let pkg_origin = summary.origin.clone();
    action_btn.connect_clicked(move |_| {
        // Remove exactly the installation that was found, e.g. the Flatpak.
        match preferred(&installations.borrow(), pkg_source) {
            Some(installation) => queue.add_remove(installation.name.clone(), installation.source),
            None => match chosen.borrow().clone() {
                Some(option) => queue.add_install(option.name, option.source, option.origin),
                None => queue.add_install(pkg_name.clone(), pkg_source, pkg_origin.clone()),
            },
        }
    });

    let update_queue = handles.queue.clone();
    let pending_update = pending_update.clone();
    update_btn.connect_clicked(move |_| {
        let Some(action) = pending_update.borrow().clone() else { return };
        update_queue.add_upgrade_packages(vec![action]);
        update_queue.show_review_dialog();
    });

    let home_url = home_url.clone();
    let toasts = handles.toasts.clone();
    open_home_btn.connect_clicked(move |_| {
        if let Some(url) = home_url.borrow().clone() {
            if gio::AppInfo::launch_default_for_uri(&url, None::<&gio::AppLaunchContext>).is_err() {
                toasts.add_toast(adw::Toast::new(t("Failed to open homepage")));
            }
        }
    });

    let drawer = handles.log_drawer.clone();
    logs_btn.connect_clicked(move |_| {
        let visible = drawer.is_visible();
        drawer.set_visible(!visible);
    });
}

/// Repositories, Flathub and (when switched on) the AUR; off the main thread.
fn source_options(ctx: &AppContext, summary: &PackageSummary, flatpak_id: Option<&str>) -> Vec<SourceOption> {
    let known = SourceOption {
        source: summary.source,
        name: summary.name.clone(),
        version: summary.version.clone(),
        size: None,
        origin: summary.origin.clone(),
    };
    let from = |source: PackageSource, details: PackageDetails| SourceOption {
        source,
        name: details.name,
        version: details.version,
        size: details.size,
        origin: None,
    };
    let repo = |name: &str| ctx.pacman.info_repo(name).ok().filter(|d| !d.version.is_empty()).map(|d| from(PackageSource::Repo, d));
    let flatpak_on = ctx.flatpak_available.lock().map(|on| *on).unwrap_or(false);
    let flathub = |query: &str| if flatpak_on { sources::flathub_search(query) } else { Vec::new() };
    let aur = |name: &str| ctx.aur.info(name).ok().filter(|d| !d.version.is_empty()).map(|d| from(PackageSource::Aur, d));
    let aur_on = ctx.settings.lock().map(|s| s.aur_available()).unwrap_or(false);
    // Search hits carry no size; ask the clicked source itself for the full entry.
    let known = match known.source {
        PackageSource::Repo => repo(&known.name),
        PackageSource::Aur if aur_on => aur(&known.name),
        _ => None,
    }
    .map(|full| SourceOption { origin: known.origin.clone(), ..full })
    .unwrap_or(known);
    let lookups = Lookups { repo: &repo, flathub: &flathub, aur: aur_on.then_some(&aur as &dyn Fn(&str) -> Option<SourceOption>) };
    sources::find(&summary.name, flatpak_id, known, &lookups)
}

/// One row per source with a radio button; prefer Flatpak for a new install.
fn fill_sources(
    list: &gtk::ListBox,
    options: &[SourceOption],
    found: &[Installation],
    preselect: PackageSource,
    chosen: &Rc<RefCell<Option<SourceOption>>>,
    system_note: &gtk::Label,
    pending_updates: usize,
) {
    // The note matters for a new native install only (not for Flatpak, not once installed).
    let not_installed = found.is_empty();
    // Native installs update the whole system even when no updates are currently
    // waiting. Recommend Flatpak whenever it is available for a new install.
    let flatpak_offered = options.iter().any(|option| option.source == PackageSource::Flatpak);
    let prefer_flatpak = not_installed && flatpak_offered;
    let preselect = if prefer_flatpak { PackageSource::Flatpak } else { preselect };
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }
    let mut group: Option<gtk::CheckButton> = None;
    for option in options {
        let waiting = not_installed && pending_updates > 0;
        let (title, hint) = match option.source {
            PackageSource::Repo if waiting => (
                t("System package sources"),
                tf("Updates the whole system first: {} updates are waiting, this can take a while.", &[&pending_updates]),
            ),
            PackageSource::Repo if prefer_flatpak => (t("System package sources"), t("Installing this also updates the whole system.").to_string()),
            PackageSource::Repo => (t("System package sources (recommended)"), t("Checked by the distribution and updated together with the system.").to_string()),
            PackageSource::Flatpak if prefer_flatpak => (
                t("Flathub (Flatpak) (recommended)"),
                t("No system update needed. Runs in its own sandbox and updates separately.").to_string(),
            ),
            PackageSource::Flatpak => (t("Flathub (Flatpak)"), t("Runs in its own sandbox and updates separately.").to_string()),
            PackageSource::Aur if waiting => (
                t("AUR (community)"),
                tf("Updates the whole system first ({} updates are waiting) and is built on this computer.", &[&pending_updates]),
            ),
            PackageSource::Aur => (t("AUR (community)"), t("Installing this also updates the whole system.").to_string()),
        };
        let mut facts = vec![tf("Version {}", &[&option.version])];
        if let Some(size) = &option.size {
            facts.push(size.clone());
        }
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(format!("{} · {}", facts.join(" · "), hint))
            .subtitle_lines(2)
            .build();
        let radio = gtk::CheckButton::new();
        radio.set_group(group.as_ref());
        group.get_or_insert_with(|| radio.clone());
        radio.set_valign(gtk::Align::Center);
        row.add_prefix(&radio);
        row.set_activatable_widget(Some(&radio));
        if found.iter().any(|installation| installation.source == option.source) {
            let badge = gtk::Label::new(Some(t("Installed")));
            badge.add_css_class("pill");
            badge.set_valign(gtk::Align::Center);
            row.add_suffix(&badge);
        }
        let (chosen_for_radio, option_for_radio, note_for_radio) = (chosen.clone(), option.clone(), system_note.clone());
        radio.connect_toggled(move |radio| {
            if radio.is_active() {
                note_for_radio.set_visible(not_installed && option_for_radio.source != PackageSource::Flatpak);
                *chosen_for_radio.borrow_mut() = Some(option_for_radio.clone());
            }
        });
        if option.source == preselect {
            radio.set_active(true);
            *chosen.borrow_mut() = Some(option.clone());
        }
        list.append(&row);
    }
}

fn load_details(ctx: AppContext, summary: PackageSummary, appstream: Arc<AppStreamClient>, flatpak_id: Option<&str>) -> (PackageDetails, Option<crate::core::appstream::FlathubInfo>) {
    // Ask the local database instead of trusting the caller: home tiles open
    // this page without knowing whether the package is installed.
    let mut details = match summary.source {
        PackageSource::Repo => ctx
            .pacman
            .info_installed(&summary.name)
            .or_else(|_| ctx.pacman.info_repo(&summary.name))
            .unwrap_or_else(|_| fallback_details(&summary)),
        PackageSource::Aur => ctx
            .pacman
            .info_installed(&summary.name)
            .map(|mut details| {
                details.source = PackageSource::Aur;
                details
            })
            .or_else(|_| ctx.aur.info(&summary.name))
            .unwrap_or_else(|_| fallback_details(&summary)),
        PackageSource::Flatpak => ctx
            .flatpak
            .info(&summary.name)
            .unwrap_or_else(|_| fallback_details(&summary)),
    };

    let mut component_id = None;
    if let Some(component) = appstream.search_component(&summary.name) {
        component_id = Some(component.id.clone());
        let comp = appstream
            .get_component(&component.id)
            .unwrap_or(component);
        if let Some(summary) = comp.summary {
            details.summary = summary;
        }
        if comp.icon_name.is_some() {
            details.icon_name = comp.icon_name;
        }
        if !comp.screenshots.is_empty() {
            details.screenshots = comp.screenshots.clone();
        }
    }

    // Flathub's own data: nicer name, formatted description, screenshots.
    let own_id = (summary.source == PackageSource::Flatpak).then(|| summary.name.clone());
    let mut info = [flatpak_id.map(str::to_owned), own_id, component_id]
        .into_iter()
        .flatten()
        .find_map(|id| AppStreamClient::flathub_info(&id));
    if info.is_none() && summary.source != PackageSource::Flatpak {
        info = sources::flathub_id_for(&summary.name).and_then(|id| AppStreamClient::flathub_info(&id));
    }
    if let Some(info) = &info {
        if details.screenshots.is_empty() {
            details.screenshots = info.screenshots.clone();
        }
    }

    (details, info)
}

fn fallback_details(summary: &PackageSummary) -> PackageDetails {
    PackageDetails {
        name: summary.name.clone(),
        summary: summary.summary.clone(),
        description: summary.summary.clone(),
        version: summary.version.clone(),
        source: summary.source,
        installed: summary.installed,
        size: None,
        home: None,
        screenshots: Vec::new(),
        icon_name: None,
    }
}

/// The installation a remove acts on: the one from this page's source if
/// present, otherwise the first one found.
fn preferred(found: &[Installation], source: PackageSource) -> Option<&Installation> {
    found.iter().find(|installation| installation.source == source).or_else(|| found.first())
}

fn markup_escape(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}

/// "Not installed" or "Flatpak org.gimp.GIMP 3.0.4; Pacman gimp 3.0.2".
fn installed_value(found: &[Installation]) -> String {
    if found.is_empty() {
        return t("Not installed").to_string();
    }
    found
        .iter()
        .map(|installation| format!("{} {} {}", installation.source.label(), installation.name, installation.version).trim_end().to_string())
        .collect::<Vec<_>>()
        .join("; ")
}
