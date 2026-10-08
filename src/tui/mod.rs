//! Terminal UI built on ratatui and crossterm.
//!
//! The UI is split three ways:
//!
//! - [`App`] holds the state and changes only through [`App::update`],
//!   which turns a [`Msg`] (key press, tick, finished load) into new state
//!   plus [`Command`]s to run. It never touches the terminal or the network.
//! - [`render`] draws an [`App`]: tab bar, current [`Screen`], status bar
//!   and the key-binding help overlay.
//! - [`run`] owns the terminal and the event loop. It runs commands as
//!   background tasks against any [`EventProvider`](crate::provider::EventProvider),
//!   so the UI keeps drawing while data loads.

mod app;
mod run;
mod ui;

pub use app::{App, Command, KEY_BINDINGS, Load, Msg, Screen};
pub use run::run;
pub use ui::render;
