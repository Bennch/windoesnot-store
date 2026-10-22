mod app;
mod core;
mod desktop;
mod i18n;
mod ui;

fn main() {
    let app = app::AuroraApp::new();
    app.run();
}
