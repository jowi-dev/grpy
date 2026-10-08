//! TUI state and the transitions between states.
//!
//! [`App`] holds everything the screen shows and changes only through
//! [`App::update`], which takes a [`Msg`] and returns the [`Command`]s the
//! event loop should run. Nothing here touches the terminal or a provider,
//! so every transition can be unit-tested.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::domain::{Location, Venue};
use crate::provider;

/// Key bindings shown in the help overlay, as `(keys, action)`.
pub const KEY_BINDINGS: &[(&str, &str)] = &[
    ("Tab / Shift-Tab", "Next / previous screen"),
    ("1, 2", "Go to Venues, Concerts"),
    ("r", "Reload venues"),
    ("?", "Toggle this help"),
    ("Esc", "Close help"),
    ("q, Ctrl-C", "Quit"),
];

/// Braille spinner frames shown while data is loading.
const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// A top-level screen, switched between with Tab or the number keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Venues near the search location.
    Venues,
    /// Upcoming shows at followed venues.
    Concerts,
}

impl Screen {
    /// Every screen, in tab order.
    pub const ALL: [Screen; 2] = [Screen::Venues, Screen::Concerts];

    /// Name shown in the tab bar.
    pub fn title(self) -> &'static str {
        match self {
            Screen::Venues => "Venues",
            Screen::Concerts => "Concerts",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    fn offset(self, by: isize) -> Screen {
        let len = Self::ALL.len() as isize;
        Self::ALL[(self.index() as isize + by).rem_euclid(len) as usize]
    }
}

/// Data fetched in the background.
#[derive(Debug, Clone, PartialEq)]
pub enum Load<T> {
    /// A fetch is in flight.
    Loading,
    /// The fetch finished.
    Loaded(T),
    /// The fetch failed, with a message for the user.
    Failed(String),
}

/// Something that happened, fed to [`App::update`].
#[derive(Debug)]
pub enum Msg {
    /// A key was pressed (or released; only presses do anything).
    Key(KeyEvent),
    /// Time passed; advances the loading spinner.
    Tick,
    /// SIGINT arrived from outside the terminal.
    Interrupt,
    /// A [`Command::LoadVenues`] finished.
    VenuesLoaded(provider::Result<Vec<Venue>>),
}

/// Work [`App::update`] asks the event loop to do in the background.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Fetch venues within `radius_km` of `near`, then send
    /// [`Msg::VenuesLoaded`].
    LoadVenues {
        /// Search origin.
        near: Location,
        /// Search radius in kilometres.
        radius_km: u32,
    },
}

/// Everything the TUI shows.
#[derive(Debug, Clone, PartialEq)]
pub struct App {
    location: Location,
    radius_km: u32,
    screen: Screen,
    venues: Load<Vec<Venue>>,
    help_open: bool,
    running: bool,
    ticks: usize,
}

impl App {
    /// An app searching within `radius_km` of `location`, with nothing
    /// loaded yet. Call [`start`](Self::start) to get the command that
    /// loads it.
    pub fn new(location: Location, radius_km: u32) -> Self {
        Self {
            location,
            radius_km,
            screen: Screen::Venues,
            venues: Load::Loading,
            help_open: false,
            running: true,
            ticks: 0,
        }
    }

    /// Marks venues as loading and returns the command that loads them.
    pub fn start(&mut self) -> Vec<Command> {
        self.venues = Load::Loading;
        vec![Command::LoadVenues {
            near: self.location.clone(),
            radius_km: self.radius_km,
        }]
    }

    /// Applies `msg` and returns any background work it calls for.
    pub fn update(&mut self, msg: Msg) -> Vec<Command> {
        match msg {
            Msg::Key(key) => return self.on_key(key),
            Msg::Tick => self.ticks = self.ticks.wrapping_add(1),
            Msg::Interrupt => self.running = false,
            Msg::VenuesLoaded(result) => {
                self.venues = match result {
                    Ok(venues) => Load::Loaded(venues),
                    Err(err) => Load::Failed(err.to_string()),
                }
            }
        }
        vec![]
    }

    fn on_key(&mut self, key: KeyEvent) -> Vec<Command> {
        if key.kind == KeyEventKind::Release {
            return vec![];
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if ctrl => self.running = false,
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('?') => self.help_open = !self.help_open,
            KeyCode::Esc => self.help_open = false,
            KeyCode::Tab => self.screen = self.screen.offset(1),
            KeyCode::BackTab => self.screen = self.screen.offset(-1),
            KeyCode::Char('1') => self.screen = Screen::Venues,
            KeyCode::Char('2') => self.screen = Screen::Concerts,
            KeyCode::Char('r') if self.venues != Load::Loading => return self.start(),
            _ => {}
        }
        vec![]
    }

    /// Where the search is centred.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Search radius in kilometres.
    pub fn radius_km(&self) -> u32 {
        self.radius_km
    }

    /// The screen currently shown.
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Venues near [`location`](Self::location).
    pub fn venues(&self) -> &Load<Vec<Venue>> {
        &self.venues
    }

    /// Whether the key-binding help overlay is open.
    pub fn help_open(&self) -> bool {
        self.help_open
    }

    /// `false` once the user has asked to quit.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// The spinner frame to draw, or `None` when nothing is loading.
    pub fn spinner(&self) -> Option<char> {
        (self.venues == Load::Loading).then(|| SPINNER[self.ticks % SPINNER.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn here() -> Location {
        Location {
            lat: 26.1224,
            lon: -80.1373,
            label: "Fort Lauderdale, FL".into(),
        }
    }

    fn venue(id: &str) -> Venue {
        Venue {
            id: id.parse().unwrap(),
            name: id.into(),
            address: None,
            location: here(),
        }
    }

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl(c: char) -> Msg {
        Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }

    fn load_venues() -> Command {
        Command::LoadVenues {
            near: here(),
            radius_km: 40,
        }
    }

    fn started() -> App {
        let mut app = App::new(here(), 40);
        app.start();
        app
    }

    fn loaded(venues: Vec<Venue>) -> App {
        let mut app = started();
        app.update(Msg::VenuesLoaded(Ok(venues)));
        app
    }

    #[test]
    fn new_app_is_running_on_the_venues_screen() {
        let app = App::new(here(), 40);

        assert!(app.is_running());
        assert_eq!(app.screen(), Screen::Venues);
        assert!(!app.help_open());
    }

    #[test]
    fn start_loads_venues_around_the_location() {
        let mut app = App::new(here(), 40);

        assert_eq!(app.start(), [load_venues()]);
        assert_eq!(app.venues(), &Load::Loading);
    }

    #[test]
    fn loaded_venues_are_kept() {
        let app = loaded(vec![venue("fake:club")]);

        assert_eq!(app.venues(), &Load::Loaded(vec![venue("fake:club")]));
    }

    #[test]
    fn a_failed_load_keeps_the_error_message() {
        let mut app = started();
        let err = provider::Error::UnknownVenue("fake:gone".parse().unwrap());

        app.update(Msg::VenuesLoaded(Err(err)));

        assert_eq!(
            app.venues(),
            &Load::Failed("unknown venue `fake:gone`".into())
        );
    }

    #[test]
    fn spinner_turns_on_ticks_only_while_loading() {
        let mut app = started();
        let first = app.spinner().unwrap();

        app.update(Msg::Tick);
        assert_ne!(app.spinner().unwrap(), first);

        app.update(Msg::VenuesLoaded(Ok(vec![])));
        assert_eq!(app.spinner(), None);
    }

    #[test]
    fn spinner_wraps_around() {
        let mut app = started();
        let first = app.spinner();

        for _ in 0..SPINNER.len() {
            app.update(Msg::Tick);
        }

        assert_eq!(app.spinner(), first);
    }

    #[test]
    fn r_reloads_venues() {
        let mut app = loaded(vec![]);

        assert_eq!(app.update(key(KeyCode::Char('r'))), [load_venues()]);
        assert_eq!(app.venues(), &Load::Loading);
    }

    #[test]
    fn r_while_loading_does_not_start_a_second_load() {
        let mut app = started();

        assert_eq!(app.update(key(KeyCode::Char('r'))), []);
    }

    #[test]
    fn q_quits() {
        let mut app = started();
        app.update(key(KeyCode::Char('q')));
        assert!(!app.is_running());
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = started();
        app.update(ctrl('c'));
        assert!(!app.is_running());
    }

    #[test]
    fn interrupt_quits() {
        let mut app = started();
        app.update(Msg::Interrupt);
        assert!(!app.is_running());
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut app = started();
        let mut release = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;

        app.update(Msg::Key(release));

        assert!(app.is_running());
    }

    #[test]
    fn question_mark_toggles_help() {
        let mut app = started();

        app.update(key(KeyCode::Char('?')));
        assert!(app.help_open());

        app.update(key(KeyCode::Char('?')));
        assert!(!app.help_open());
    }

    #[test]
    fn esc_closes_help_without_quitting() {
        let mut app = started();
        app.update(key(KeyCode::Char('?')));

        app.update(key(KeyCode::Esc));

        assert!(!app.help_open());
        assert!(app.is_running());
    }

    #[test]
    fn q_quits_even_with_help_open() {
        let mut app = started();
        app.update(key(KeyCode::Char('?')));

        app.update(key(KeyCode::Char('q')));

        assert!(!app.is_running());
    }

    #[test]
    fn tab_cycles_through_screens() {
        let mut app = started();

        app.update(key(KeyCode::Tab));
        assert_eq!(app.screen(), Screen::Concerts);

        app.update(key(KeyCode::Tab));
        assert_eq!(app.screen(), Screen::Venues);

        app.update(key(KeyCode::BackTab));
        assert_eq!(app.screen(), Screen::Concerts);
    }

    #[test]
    fn number_keys_jump_to_screens() {
        let mut app = started();

        app.update(key(KeyCode::Char('2')));
        assert_eq!(app.screen(), Screen::Concerts);

        app.update(key(KeyCode::Char('1')));
        assert_eq!(app.screen(), Screen::Venues);
    }
}
