mod app;
mod core;
mod desktop;
mod i18n;
mod ui;

fn main() {
    // GTK names windows without an application after the program; dialogs
    // would otherwise appear as a separate "windoesnot-store" app in the taskbar.
    gtk::glib::set_prgname(Some("me.windoesnot.Store"));
    let app = app::AuroraApp::new();
    app.run();
}
