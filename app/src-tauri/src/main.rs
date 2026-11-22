//! Binario de OpenLight.

// Sin consola extra en Windows para las builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    openlight_lib::run();
}
