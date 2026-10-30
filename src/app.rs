use libadwaita as adw;
use adw::prelude::*;
use gtk::gio;
use gtk::glib::variant::ToVariant;

use crate::ui;

pub struct AuroraApp {
    app: adw::Application,
}

impl AuroraApp {
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id("me.windoesnot.Store")
            .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
            .build();

        app.connect_startup(|_| {
            adw::init();
        });

        app.connect_activate(|app| {
            if let Some(window) = app.active_window() {
                window.present();
            } else {
                ui::build_ui(app);
            }
        });

        app.connect_command_line(|app, command_line| {
            let args = command_line.arguments();
            if args.len() == 3 && args[1] == "--open-desktop" {
                let path = args[2].to_string_lossy();
                app.activate();
                app.activate_action("open-desktop", Some(&path.to_string().to_variant()));
                0
            } else if args.len() == 1 {
                app.activate();
                0
            } else {
                eprintln!("Usage: windoesnot-store [--open-desktop /path/to/app.desktop]");
                2
            }
        });

        Self { app }
    }

    pub fn run(self, forward_desktop: bool) {
        // Own options (e.g. --updates) are handled in main; GApplication would
        // otherwise treat them as files to open.
        let args: Vec<_> = std::env::args().collect();
        self.app.run_with_args(if forward_desktop { &args } else { &args[..1] });
    }
}
