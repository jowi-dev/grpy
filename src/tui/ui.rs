//! Drawing an [`App`] to a terminal frame.
//!
//! Rendering only reads the app; all state changes go through
//! [`App::update`].

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, Paragraph, Tabs, Wrap};

use super::app::{App, KEY_BINDINGS, Load, Screen};

/// Draws the whole screen: tab bar, the current screen, the status bar
/// and, when open, the help overlay.
pub fn render(app: &App, frame: &mut Frame) {
    let [tabs, body, status] = frame.area().layout(&Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ]));

    render_tabs(app, frame, tabs);
    match app.screen() {
        Screen::Venues => render_venues(app, frame, body),
        Screen::Concerts => render_concerts(frame, body),
    }
    render_status(app, frame, status);
    if app.help_open() {
        render_help(frame, body);
    }
}

fn render_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let selected = Screen::ALL.iter().position(|s| *s == app.screen());
    let tabs = Tabs::new(Screen::ALL.map(Screen::title))
        .select(selected)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_widget(tabs, area);
}

fn render_venues(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title(" Venues ");
    match app.venues() {
        Load::Loaded(venues) if !venues.is_empty() => {
            let list = List::new(venues.iter().map(|v| v.name.as_str())).block(block);
            frame.render_widget(list, area);
        }
        Load::Loaded(_) => message(
            frame,
            area,
            block,
            format!(
                "No venues within {} km of {}.",
                app.radius_km(),
                app.location().label
            ),
        ),
        Load::Loading => message(frame, area, block, "Loading venues…".into()),
        Load::Failed(_) => message(frame, area, block, "Press r to retry.".into()),
    }
}

fn render_concerts(frame: &mut Frame, area: Rect) {
    message(
        frame,
        area,
        Block::bordered().title(" Concerts "),
        "No concerts yet. Follow venues to see their upcoming shows here.".into(),
    );
}

fn message(frame: &mut Frame, area: Rect, block: Block, text: String) {
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: true }).block(block);
    frame.render_widget(paragraph, area);
}

fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let left = match app.venues() {
        Load::Loading => {
            let spinner = app.spinner().unwrap_or(' ');
            Span::raw(format!(" {spinner} Loading venues…"))
        }
        Load::Loaded(venues) => {
            let noun = if venues.len() == 1 { "venue" } else { "venues" };
            Span::raw(format!(
                " {} {noun} within {} km",
                venues.len(),
                app.radius_km()
            ))
        }
        Load::Failed(err) => Span::raw(format!(" Couldn't load venues: {err}")).fg(Color::Red),
    };
    let right = Line::from(format!("{} · ? help ", app.location().label));

    let bar = Style::new().add_modifier(Modifier::REVERSED);
    let [left_area, right_area] = area.layout(&Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(right.width() as u16),
    ]));
    frame.render_widget(Line::from(left).style(bar), left_area);
    frame.render_widget(right.style(bar), right_area);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let keys_width = KEY_BINDINGS.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let lines: Vec<Line> = KEY_BINDINGS
        .iter()
        .map(|(keys, action)| {
            Line::from(vec![
                Span::raw(format!("{keys:<keys_width$}  ")).bold(),
                Span::raw(*action),
            ])
        })
        .collect();
    let width = lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 4;
    let height = lines.len() as u16 + 2;

    let popup = area.centered(Constraint::Length(width), Constraint::Length(height));
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Keys ")),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::domain::{Location, Venue};
    use crate::provider;
    use crate::tui::app::Msg;

    fn here() -> Location {
        Location {
            lat: 26.1224,
            lon: -80.1373,
            label: "Fort Lauderdale, FL".into(),
        }
    }

    fn venue(name: &str) -> Venue {
        Venue {
            id: format!("fake:{}", name.to_lowercase().replace(' ', "-"))
                .parse()
                .unwrap(),
            name: name.into(),
            address: None,
            location: here(),
        }
    }

    fn started() -> App {
        let mut app = App::new(here(), 40);
        app.start();
        app
    }

    fn press(app: &mut App, code: KeyCode) {
        app.update(Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    /// Renders `app` on an 80x24 screen and returns its rows as text.
    fn draw(app: &App) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render(app, frame)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    fn screen_text(app: &App) -> String {
        draw(app).join("\n")
    }

    fn status_bar(app: &App) -> String {
        draw(app).pop().unwrap()
    }

    #[test]
    fn tab_bar_names_every_screen() {
        let top = draw(&started()).remove(0);

        assert!(top.contains("Venues"), "{top}");
        assert!(top.contains("Concerts"), "{top}");
    }

    #[test]
    fn status_bar_shows_spinner_while_loading() {
        let app = started();
        let status = status_bar(&app);

        assert!(status.contains(app.spinner().unwrap()), "{status}");
        assert!(status.contains("Loading venues"), "{status}");
    }

    #[test]
    fn status_bar_shows_location_and_help_hint() {
        let status = status_bar(&started());

        assert!(status.contains("Fort Lauderdale, FL"), "{status}");
        assert!(status.contains("? help"), "{status}");
    }

    #[test]
    fn loaded_venues_are_listed_and_counted() {
        let mut app = started();
        app.update(Msg::VenuesLoaded(Ok(vec![
            venue("Revolution Live"),
            venue("Culture Room"),
        ])));

        let text = screen_text(&app);
        assert!(text.contains("Revolution Live"), "{text}");
        assert!(text.contains("Culture Room"), "{text}");
        assert!(status_bar(&app).contains("2 venues within 40 km"));
    }

    #[test]
    fn no_venues_says_so() {
        let mut app = started();
        app.update(Msg::VenuesLoaded(Ok(vec![])));

        let text = screen_text(&app);
        assert!(text.contains("No venues within 40 km"), "{text}");
    }

    #[test]
    fn failed_load_shows_the_error() {
        let mut app = started();
        let err = provider::Error::UnknownVenue("fake:gone".parse().unwrap());
        app.update(Msg::VenuesLoaded(Err(err)));

        let status = status_bar(&app);
        assert!(
            status.contains("Couldn't load venues: unknown venue `fake:gone`"),
            "{status}"
        );
    }

    #[test]
    fn concerts_screen_is_shown_after_switching() {
        let mut app = started();
        press(&mut app, KeyCode::Tab);

        let text = screen_text(&app);
        assert!(text.contains("No concerts yet"), "{text}");
    }

    #[test]
    fn help_overlay_lists_every_key_binding() {
        let mut app = started();
        press(&mut app, KeyCode::Char('?'));

        let text = screen_text(&app);
        for (keys, action) in KEY_BINDINGS {
            assert!(text.contains(keys), "missing {keys}:\n{text}");
            assert!(text.contains(action), "missing {action}:\n{text}");
        }
    }

    #[test]
    fn help_overlay_is_hidden_by_default() {
        let text = screen_text(&started());

        assert!(!text.contains(KEY_BINDINGS[0].1), "{text}");
    }
}
