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
