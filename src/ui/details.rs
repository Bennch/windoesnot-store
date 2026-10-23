use std::sync::Arc;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use gtk::gio;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

use crate::core::appstream::AppStreamClient;
use crate::core::models::{PackageDetails, PackageSource, PackageSummary};
use crate::ui::{AppContext, UiHandles};
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
    let icon = app_icon(&summary.name, flathub_id, 96);
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
    // Single-package updates are partial upgrades; updates run via "Update all".
    button_row.append(&action_btn);
    button_row.append(&open_home_btn);
    button_row.append(&logs_btn);

    let carousel = ScreenshotCarousel::new();

    let details = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let version = gtk::Label::new(Some(t("Version: -")));
    version.set_xalign(0.0);
    let installed = gtk::Label::new(Some(t("Installed: no")));
    installed.set_xalign(0.0);
    let size = gtk::Label::new(Some(t("Size: -")));
    size.set_xalign(0.0);
    let homepage = gtk::Label::new(Some(t("Homepage: -")));
    homepage.set_xalign(0.0);
    homepage.set_selectable(true);
    homepage.set_wrap(true);
    details.append(&version);
    details.append(&installed);
    details.append(&size);
    details.append(&homepage);

    let description = gtk::Label::new(Some(""));
    description.set_xalign(0.0);
    description.set_wrap(true);

    root.append(&back_btn);
    root.append(&header);
    root.append(&button_row);
    root.append(carousel.widget());
    root.append(&details);
    root.append(&description);

    page.set_child(Some(&root));
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
    let carousel_clone = carousel.clone();
    let action_btn_clone = action_btn.clone();
    let open_home_btn_clone = open_home_btn.clone();
    let homepage_clone = homepage.clone();
    let installations_clone = installations.clone();
    let home_url = Rc::new(RefCell::new(None::<String>));
    let home_url_clone = home_url.clone();
    let appstream = ctx.appstream.clone();

    let summary_source = summary.source;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let found = ctx_clone.installed_apps().find(&summary_clone.name, flatpak_id.as_deref());
        let details = load_details(ctx_clone, summary_clone, appstream);
        let _ = tx.send((details, found));
    });

    glib::idle_add_local(move || {
        match rx.try_recv() {
            Ok((details, found)) => {
                summary_label_clone.set_text(&details.summary);
                version_clone.set_text(&tf("Version: {}", &[&details.version]));
                installed_clone.set_text(&installed_text(&found));
                action_btn_clone.set_label(&match preferred(&found, summary_source) {
                    Some(installation) => tf("Remove ({})", &[&installation.source.label()]),
                    None => t("Install").to_string(),
                });
                action_btn_clone.set_sensitive(true);
                *installations_clone.borrow_mut() = found;
                if let Some(size) = &details.size {
                    size_clone.set_text(&tf("Size: {}", &[size]));
                }
                if let Some(home) = details.home.clone() {
                    homepage_clone.set_text(&tf("Homepage: {}", &[&home]));
                    *home_url_clone.borrow_mut() = Some(home);
                    open_home_btn_clone.set_visible(true);
                } else {
                    homepage_clone.set_text(t("Homepage: -"));
                    *home_url_clone.borrow_mut() = None;
                    open_home_btn_clone.set_visible(false);
                }
                description_clone.set_text(&details.description);
                carousel_clone.set_screenshots(details.screenshots.clone());
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });

    let queue = handles.queue.clone();
    let pkg_name = summary.name.clone();
    let pkg_source = summary.source;
    let pkg_origin = summary.origin.clone();
    action_btn.connect_clicked(move |_| {
        // Remove exactly the installation that was found, e.g. the Flatpak.
        match preferred(&installations.borrow(), pkg_source) {
            Some(installation) => queue.add_remove(installation.name.clone(), installation.source),
            None => queue.add_install(pkg_name.clone(), pkg_source, pkg_origin.clone()),
        }
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

fn load_details(ctx: AppContext, summary: PackageSummary, appstream: Arc<AppStreamClient>) -> PackageDetails {
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

    if let Some(component) = appstream.search_component(&summary.name) {
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

    details
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

/// "Installed: no" or "Installed: yes – Flatpak org.gimp.GIMP 3.0.4; …".
fn installed_text(found: &[Installation]) -> String {
    if found.is_empty() {
        return tf("Installed: {}", &[&t("no")]);
    }
    let places: Vec<String> = found
        .iter()
        .map(|installation| format!("{} {} {}", installation.source.label(), installation.name, installation.version).trim_end().to_string())
        .collect();
    tf("Installed: yes – {}", &[&places.join("; ")])
}
