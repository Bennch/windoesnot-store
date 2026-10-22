use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

use crate::core::cache::{clear_screenshots_cache, save_settings};
use crate::core::models::{AurHelperKind, TerminalEmulator, TerminalMode, ThemeMode};
use crate::ui::{apply_theme, AppContext};
use crate::i18n::{t, tf};

#[derive(Clone)]
pub struct SettingsPage {
    pub root: adw::PreferencesPage,
    theme_row: adw::ComboRow,
    terminal_mode_row: adw::ComboRow,
    terminal_emulator_row: adw::ComboRow,
    helper_row: adw::ComboRow,
    noconfirm_row: adw::SwitchRow,
    clear_cache: gtk::Button,
    about_btn: gtk::Button,
}

impl SettingsPage {
    pub fn new() -> Self {
        let root = adw::PreferencesPage::new();
        root.set_hexpand(true);
        root.set_vexpand(true);

        let appearance_group = adw::PreferencesGroup::new();
        appearance_group.set_title(t("Appearance"));
        let theme_labels = ThemeMode::all()
            .iter()
            .map(|theme| t(theme.label()))
            .collect::<Vec<_>>();
        let theme_list = gtk::StringList::new(&theme_labels);
        let theme_row = adw::ComboRow::new();
        theme_row.set_title(t("Theme"));
        theme_row.set_model(Some(&theme_list));
        appearance_group.add(&theme_row);

        let terminal_mode_labels = TerminalMode::all()
            .iter()
            .map(|mode| t(mode.label()))
            .collect::<Vec<_>>();
        let terminal_mode_list = gtk::StringList::new(&terminal_mode_labels);
        let terminal_mode_row = adw::ComboRow::new();
        terminal_mode_row.set_title(t("Command Output"));
        terminal_mode_row.set_subtitle(t("Integrated logs or external terminal window"));
        terminal_mode_row.set_model(Some(&terminal_mode_list));
        appearance_group.add(&terminal_mode_row);

        let terminal_emulator_labels = TerminalEmulator::all()
            .iter()
            .map(|terminal| terminal.label())
            .collect::<Vec<_>>();
        let terminal_emulator_list = gtk::StringList::new(&terminal_emulator_labels);
        let terminal_emulator_row = adw::ComboRow::new();
        terminal_emulator_row.set_title(t("Terminal Emulator"));
        terminal_emulator_row.set_subtitle(t("Used when Command Output is External Terminal"));
        terminal_emulator_row.set_model(Some(&terminal_emulator_list));
        appearance_group.add(&terminal_emulator_row);

        let group = adw::PreferencesGroup::new();
        group.set_title(t("General"));

        let list = gtk::StringList::new(&["yay", "paru"]);
        let helper_row = adw::ComboRow::new();
        helper_row.set_title(t("AUR Helper"));
        helper_row.set_model(Some(&list));

        let noconfirm_row = adw::SwitchRow::new();
        noconfirm_row.set_title(t("Allow --noconfirm"));
        noconfirm_row.set_subtitle(t("Applies to external terminal mode. Integrated logs are always non-interactive."));

        let cache_group = adw::PreferencesGroup::new();
        cache_group.set_title(t("Cache"));
        let clear_cache = gtk::Button::with_label(t("Clear screenshots cache"));
        let cache_row = adw::ActionRow::new();
        cache_row.set_title(t("Screenshots"));
        cache_row.add_suffix(&clear_cache);
        cache_row.set_activatable(false);

        let about_group = adw::PreferencesGroup::new();
        about_group.set_title(t("About"));
        let about_btn = gtk::Button::with_label(t("About WinDoesNot Store"));
        let about_row = adw::ActionRow::new();
        about_row.set_title(t("About"));
        about_row.add_suffix(&about_btn);
        about_row.set_activatable(false);
        about_group.add(&about_row);

        group.add(&helper_row);
        group.add(&noconfirm_row);
        cache_group.add(&cache_row);

        root.add(&appearance_group);
        root.add(&group);
        root.add(&cache_group);
        root.add(&about_group);

        Self {
            root,
            theme_row,
            terminal_mode_row,
            terminal_emulator_row,
            helper_row,
            noconfirm_row,
            clear_cache,
            about_btn,
        }
    }

    pub fn bind(&self, ctx: AppContext) {
        let settings = ctx.settings.lock().unwrap().clone();
        self.theme_row.set_selected(settings.theme.to_index());
        self.terminal_mode_row
            .set_selected(settings.terminal_mode.to_index());
        self.terminal_emulator_row
            .set_selected(settings.terminal_emulator.to_index());
        self.terminal_emulator_row
            .set_sensitive(settings.terminal_mode == TerminalMode::External);
        match settings.aur_helper {
            AurHelperKind::Yay => self.helper_row.set_selected(0),
            AurHelperKind::Paru => self.helper_row.set_selected(1),
        }
        self.noconfirm_row.set_active(settings.allow_noconfirm);

        let ctx_clone = ctx.clone();
        self.theme_row
            .connect_selected_notify(move |row: &adw::ComboRow| {
                let selected = row.selected();
                let mut settings = ctx_clone.settings.lock().unwrap();
                settings.theme = ThemeMode::from_index(selected);
                apply_theme(settings.theme);
                let _ = save_settings(&settings);
            });

        let ctx_clone = ctx.clone();
        let terminal_emulator_row = self.terminal_emulator_row.clone();
        self.terminal_mode_row
            .connect_selected_notify(move |row: &adw::ComboRow| {
                let selected = row.selected();
                let mode = TerminalMode::from_index(selected);
                terminal_emulator_row.set_sensitive(mode == TerminalMode::External);
                let mut settings = ctx_clone.settings.lock().unwrap();
                settings.terminal_mode = mode;
                let _ = save_settings(&settings);
            });

        let ctx_clone = ctx.clone();
        self.terminal_emulator_row
            .connect_selected_notify(move |row: &adw::ComboRow| {
                let selected = row.selected();
                let mut settings = ctx_clone.settings.lock().unwrap();
                settings.terminal_emulator = TerminalEmulator::from_index(selected);
                let _ = save_settings(&settings);
            });

        let ctx_clone = ctx.clone();
        self.helper_row
            .connect_selected_notify(move |row: &adw::ComboRow| {
            let selected = row.selected();
            let mut settings = ctx_clone.settings.lock().unwrap();
            settings.aur_helper = if selected == 0 {
                AurHelperKind::Yay
            } else {
                AurHelperKind::Paru
            };
            let _ = save_settings(&settings);
        });

        let ctx_clone = ctx.clone();
        self.noconfirm_row.connect_active_notify(move |row| {
            let mut settings = ctx_clone.settings.lock().unwrap();
            settings.allow_noconfirm = row.is_active();
            let _ = save_settings(&settings);
        });

        self.clear_cache.connect_clicked(move |_| {
            let _ = clear_screenshots_cache();
        });

        self.about_btn.connect_clicked(move |_| {
            let about = adw::AboutWindow::new();
            about.set_application_name("WinDoesNot Store");
            about.set_application_icon("system-software-install");
            about.set_developer_name("ahmoodio");
            about.set_version(env!("CARGO_PKG_VERSION"));
            about.set_website("https://github.com/ahmoodio/yay-gui-manager");
            about.set_issue_url("https://github.com/ahmoodio/yay-gui-manager/issues");
            about.present();
        });
    }
}
