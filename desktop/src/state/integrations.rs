//! The settings rows and diagnostics the integrations add, and what their
//! rows do. Filled in as each integration arrives.

use super::App;

impl App {
    pub fn platform_settings(&self) -> Vec<(&'static str, &'static str, &'static str, String)> {
        Vec::new()
    }

    pub fn change_platform_setting(&mut self, _id: &str) {}

    pub fn platform_diagnostics(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}
