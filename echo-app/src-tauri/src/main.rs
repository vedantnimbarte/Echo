// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // First statement in the process, so "startup time" measures everything
    // Echo controls rather than everything after some later checkpoint.
    echo_lib::mark_start();
    echo_lib::run()
}
