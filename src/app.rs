use libadwaita as adw;
use adw::prelude::*;

use crate::ui;

pub struct AuroraApp {
    app: adw::Application,
}

impl AuroraApp {
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id("me.windoesnot.Store")
            .build();

        app.connect_startup(|_| {
            adw::init();
        });

        app.connect_activate(|app| {
            ui::build_ui(app);
        });

        Self { app }
    }

    pub fn run(self) {
        self.app.run();
    }
}
