mod app;
mod core;
mod desktop;
mod i18n;
mod ui;

fn main() {
    // GTK names windows without an application after the program; dialogs
    // would otherwise appear as a separate "windoesnot-store" app in the taskbar.
    gtk::glib::set_prgname(Some("me.windoesnot.Store"));
    let args: Vec<String> = std::env::args().collect();
    // Desktop settings read the update status without opening a window.
    match core::cache::aur_cli_value(&args[1..]) {
        Ok(Some(enabled)) => {
            if let Err(error) = core::cache::set_aur_enabled(enabled) {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            println!("{}", serde_json::json!({"aur_enabled": enabled}));
            return;
        }
        Err(error) => { eprintln!("{error}"); std::process::exit(2); }
        Ok(None) => {}
    }
    if args.iter().any(|arg| arg == "--aur-status-json") {
        let settings = core::cache::load_settings();
        println!("{}", serde_json::json!({"aur_enabled": settings.aur_enabled,
            "aur_available": settings.aur_available()}));
        return;
    }
    if args.iter().any(|arg| arg == "--status-json") {
        let status = core::status::collect(args.iter().any(|arg| arg == "--refresh"));
        println!("{}", serde_json::to_string(&status).unwrap_or_else(|_| "{}".into()));
        return;
    }
    if args.iter().any(|arg| arg == "--updates") {
        ui::START_PAGE.set("updates").ok();
    }
    let app = app::AuroraApp::new();
    app.run();
}
