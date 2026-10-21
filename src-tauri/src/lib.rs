mod ai;
mod app;
mod commands;
mod db;
mod error;
mod fetch;
mod http;
mod ingest;
mod models;
mod opml;
mod prompts;
mod subscriptions;
mod tools;
mod worker;

pub use app::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    commands::run_tauri();
}

pub fn run_web() {
    http::run_web_blocking();
}
