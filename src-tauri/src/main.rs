// Release builds on Windows must not open a console window next to the tray app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ferry_lib::run();
}
