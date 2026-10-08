//! The event loop: terminal setup and teardown, input, timers and
//! background loads.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;

use super::app::{App, Command, Msg};
use super::ui::render;
use crate::domain::Location;
use crate::provider::EventProvider;

/// How often the spinner advances and the screen is redrawn when idle.
const TICK: Duration = Duration::from_millis(100);

/// Runs the TUI until the user quits, searching for venues within
/// `radius_km` of `location` with `provider`.
///
/// Takes over the terminal (raw mode, alternate screen) and puts it back
/// when it returns, when it panics, and on Ctrl-C or SIGINT. Must be
/// called from a tokio runtime with its timer and signal drivers enabled.
///
/// # Errors
///
/// Fails if the terminal can't be set up or drawn to, or if a background
/// load panics.
pub async fn run<P>(provider: P, location: Location, radius_km: u32) -> io::Result<()>
where
    P: EventProvider + Send + Sync + 'static,
{
    // Created first so a half-finished setup is still undone.
    let _restore = RestoreOnDrop;
    // Also installs a panic hook that restores the terminal.
    let mut terminal = ratatui::try_init()?;

    let (tx, rx) = mpsc::unbounded_channel();
    spawn_input(tx.clone());
    spawn_ticks(tx.clone());
    spawn_interrupt(tx);

    let app = App::new(location, radius_km);
    event_loop(&mut terminal, app, Arc::new(provider), rx).await?;
    Ok(())
}

/// Puts the terminal back to normal when dropped, including on early
/// returns and unwinding.
struct RestoreOnDrop;

impl Drop for RestoreOnDrop {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

/// Reads terminal input on its own thread, since crossterm's reads block.
/// Stops when the receiver is gone. A read error quits the app rather than
/// leaving it unable to take input.
fn spawn_input(tx: UnboundedSender<Msg>) {
    std::thread::spawn(move || {
        while !tx.is_closed() {
            let msg = match event::poll(TICK) {
                Ok(false) => continue,
                Ok(true) => match event::read() {
                    Ok(Event::Key(key)) => Msg::Key(key),
                    Ok(_) => continue,
                    Err(_) => Msg::Interrupt,
                },
                Err(_) => Msg::Interrupt,
            };
            if tx.send(msg).is_err() {
                break;
            }
        }
    });
}

/// Sends [`Msg::Tick`] every [`TICK`] until the receiver is gone.
fn spawn_ticks(tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(TICK);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticks.tick().await;
            if tx.send(Msg::Tick).is_err() {
                break;
            }
        }
    });
}

/// Turns SIGINT into [`Msg::Interrupt`]. In raw mode Ctrl-C arrives as a
/// key press instead, so this only catches signals sent from elsewhere.
fn spawn_interrupt(tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = tx.send(Msg::Interrupt);
        }
    });
}

/// Redraws until `app` quits, feeding it `events` and the results of the
/// commands it asks for. Commands run as tokio tasks, so a slow provider
/// never blocks drawing or input. Also stops once `events` is closed and
/// no commands are left running. Returns the final app state.
///
/// # Errors
///
/// Fails if drawing fails or a background task panics.
async fn event_loop<B, P>(
    terminal: &mut Terminal<B>,
    mut app: App,
    provider: Arc<P>,
    mut events: UnboundedReceiver<Msg>,
) -> io::Result<App>
where
    B: Backend,
    B::Error: Send + Sync + 'static,
    P: EventProvider + Send + Sync + 'static,
{
    let mut tasks = JoinSet::new();
    for command in app.start() {
        spawn_command(&mut tasks, &provider, command);
    }

    while app.is_running() {
        terminal
            .draw(|frame| render(&app, frame))
            .map_err(io::Error::other)?;

        let msg = tokio::select! {
            Some(msg) = events.recv() => msg,
            Some(done) = tasks.join_next() => done.map_err(io::Error::other)?,
            else => break,
        };
        for command in app.update(msg) {
            spawn_command(&mut tasks, &provider, command);
        }
    }
    Ok(app)
}

/// Runs `command` as a task whose output is the message to send back.
fn spawn_command<P>(tasks: &mut JoinSet<Msg>, provider: &Arc<P>, command: Command)
where
    P: EventProvider + Send + Sync + 'static,
{
    let provider = Arc::clone(provider);
    match command {
        Command::LoadVenues { near, radius_km } => {
            tasks.spawn(
                async move { Msg::VenuesLoaded(provider.venues_near(&near, radius_km).await) },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    use super::*;
    use crate::domain::{DateRange, Event, Location, Venue, VenueId};
    use crate::provider::{self, FakeProvider};
    use crate::tui::app::Load;

    fn here() -> Location {
        Location {
            lat: 26.1224,
            lon: -80.1373,
            label: "Fort Lauderdale, FL".into(),
        }
    }

    fn club() -> Venue {
        Venue {
            id: "fake:club".parse().unwrap(),
            name: "Revolution Live".into(),
            address: None,
            location: here(),
        }
    }

    fn quit() -> Msg {
        Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
    }

    fn terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(80, 24)).unwrap()
    }

    fn screen_text(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    async fn run_until_closed<P>(
        terminal: &mut Terminal<TestBackend>,
        provider: P,
        events: UnboundedReceiver<Msg>,
    ) -> io::Result<App>
    where
        P: EventProvider + Send + Sync + 'static,
    {
        let app = App::new(here(), 40);
        timeout(
            Duration::from_secs(5),
            event_loop(terminal, app, Arc::new(provider), events),
        )
        .await
        .expect("event loop did not finish")
    }

    #[tokio::test]
    async fn quitting_does_not_wait_for_a_slow_load() {
        let provider = FakeProvider::new()
            .with_venue(club())
            .with_latency(Duration::from_secs(60));
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(quit()).unwrap();

        let app = run_until_closed(&mut terminal(), provider, rx)
            .await
            .unwrap();

        assert!(!app.is_running());
        assert_eq!(app.venues(), &Load::Loading);
    }

    #[tokio::test]
    async fn the_loading_spinner_is_drawn_while_a_load_is_in_flight() {
        let provider = FakeProvider::new()
            .with_venue(club())
            .with_latency(Duration::from_secs(60));
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Msg::Tick).unwrap();
        tx.send(quit()).unwrap();
        let mut terminal = terminal();

        run_until_closed(&mut terminal, provider, rx).await.unwrap();

        assert!(screen_text(&terminal).contains("Loading venues"));
    }

    #[tokio::test]
    async fn loaded_venues_reach_the_app_and_the_screen() {
        let provider = FakeProvider::new().with_venue(club());
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tx.send(quit()).unwrap();
        });
        let mut terminal = terminal();

        let app = run_until_closed(&mut terminal, provider, rx).await.unwrap();

        assert_eq!(app.venues(), &Load::Loaded(vec![club()]));
        assert!(screen_text(&terminal).contains("Revolution Live"));
    }

    #[tokio::test]
    async fn the_loop_ends_when_input_closes_and_no_work_is_left() {
        let provider = FakeProvider::new();
        let (tx, rx) = mpsc::unbounded_channel();
        drop(tx);

        let app = run_until_closed(&mut terminal(), provider, rx)
            .await
            .unwrap();

        assert_eq!(app.venues(), &Load::Loaded(vec![]));
    }

    struct PanickingProvider;

    impl EventProvider for PanickingProvider {
        async fn venues_near(&self, _: &Location, _: u32) -> provider::Result<Vec<Venue>> {
            panic!("provider blew up");
        }

        async fn upcoming_events(&self, _: &VenueId, _: DateRange) -> provider::Result<Vec<Event>> {
            panic!("provider blew up");
        }
    }

    #[tokio::test]
    async fn a_panicking_load_ends_the_loop_with_an_error() {
        let (_tx, rx) = mpsc::unbounded_channel();

        let result = run_until_closed(&mut terminal(), PanickingProvider, rx).await;

        assert!(result.is_err());
    }
}
