pub mod calc;
pub mod config;
pub mod gui;
pub mod contacts;
pub mod engine;
pub mod settings;
pub mod features;
pub mod webassets;
pub mod largetype;
pub mod webbookmarks;
pub mod install;
pub mod icons;
pub mod onepassword;
pub mod quicklook;
pub mod itunes;
pub mod theme;
pub mod expand;
pub mod hotkey;
pub mod migration;
pub mod universal;
pub mod db;
pub mod index;
pub mod tray;
pub mod router;
pub mod search;
pub mod workflow;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_wf;
#[cfg(test)]
mod tests_graph;

pub use config::Config;
#[cfg(test)]
mod tests_mig;
