// Release builds are a GUI app: without this Windows opens a console window
// next to the app, and closing it kills FujiView.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    fujiview_lib::run()
}
