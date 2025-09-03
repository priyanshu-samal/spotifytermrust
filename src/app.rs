use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Terminal,
    prelude::Stylize,
};
use rspotify::{AuthCodePkceSpotify, clients::{BaseClient, OAuthClient}, model::{PlaylistId, PlayableItem, PlayableId, TrackId}, prelude::Id};
use std::io;
use futures_util::TryStreamExt;

enum AppPanel {
    Playlists,
    Tracks,
}

struct App {
    spotify: AuthCodePkceSpotify,
    playlists: Vec<(String, String)>, // Store (name, id)
    tracks: Vec<(String, String)>, // Store (name, uri)
    selected_panel: AppPanel,
    playlists_state: ListState,
    tracks_state: ListState,
    selected_playlist_id: Option<String>, // To store the ID of the currently selected playlist
    devices: Vec<rspotify::model::Device>, // To store available devices
    selected_device_id: Option<String>, // To store the ID of the currently selected device
}

impl App {
    async fn new(spotify: AuthCodePkceSpotify) -> Result<App> {
        let mut playlists_state = ListState::default();
        playlists_state.select(Some(0)); // Select the first playlist by default

        let mut playlists: Vec<(String, String)> = Vec::new();
        let fetched_playlists = spotify.current_user_playlists().try_collect::<Vec<_>>().await?;
        for p in fetched_playlists {
            // Assuming p.id is already a PlaylistId object
            playlists.push((p.name.clone(), p.id.to_string()));
        }

        let selected_playlist_id = playlists.first().map(|(_, id)| id.clone());

        let mut app = App {
            spotify,
            playlists,
            tracks: vec![], // Initialize with empty tracks
            selected_panel: AppPanel::Playlists,
            playlists_state,
            tracks_state: ListState::default(),
            selected_playlist_id,
            devices: vec![], // Initialize empty
            selected_device_id: None, // Initialize empty
        };

        app.fetch_devices().await?;

        Ok(app)
    }

    async fn fetch_devices(&mut self) -> Result<()> {
        let devices_response = self.spotify.device().await?;
        self.devices = devices_response;
        if let Some(first_device) = self.devices.first() {
            self.selected_device_id = Some(first_device.id.clone().unwrap().to_string());
            eprintln!("Selected device: {}", first_device.name);
        } else {
            eprintln!("No active devices found.");
            self.selected_device_id = None;
        }
        Ok(())
    }

    fn next_playlist(&mut self) {
        let i = match self.playlists_state.selected() {
            Some(i) => {
                if i >= self.playlists.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.playlists_state.select(Some(i));
    }

    fn previous_playlist(&mut self) {
        let i = match self.playlists_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.playlists.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.playlists_state.select(Some(i));
    }

    fn next_track(&mut self) {
        let i = match self.tracks_state.selected() {
            Some(i) => {
                if i >= self.tracks.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.tracks_state.select(Some(i));
    }

    fn previous_track(&mut self) {
        let i = match self.tracks_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.tracks.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.tracks_state.select(Some(i));
    }

    async fn fetch_tracks_for_selected_playlist(&mut self) -> Result<()> {
        if let Some(playlist_id_str) = &self.selected_playlist_id {
            self.tracks.clear();
            self.tracks_state.select(None);

            let id_parts: Vec<&str> = playlist_id_str.split(':').collect();
            let actual_id_str = id_parts.last().unwrap_or(&"");

            let playlist_id_obj = match PlaylistId::from_id(*actual_id_str) {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("Error: Invalid playlist ID for fetching tracks: {} - {}", playlist_id_str, e);
                    return Ok(()); // Return gracefully if ID is invalid
                }
            };

            let fetched_tracks = self.spotify.playlist_items(playlist_id_obj, None, None).try_collect::<Vec<_>>().await?;
            for item in fetched_tracks {
                if let Some(playable_item) = item.track {
                    if let PlayableItem::Track(full_track) = playable_item {
                        self.tracks.push((format!("{} - {}", full_track.name, full_track.artists.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", ")), full_track.id.unwrap().uri().to_string()));
                    }
                }
            }
            if !self.tracks.is_empty() {
                self.tracks_state.select(Some(0));
            }
        }
        Ok(())
    }

    async fn play_track(&self, track_uri: String) -> Result<()> {
        let id_str = track_uri.split(':').last().ok_or_else(|| anyhow::anyhow!("Invalid track URI format"))?;
        let track_id = TrackId::from_id(id_str)?;
        let playable_id = PlayableId::from(track_id);

        // Use the selected_device_id if available
        let device_id_str = self.selected_device_id.as_deref(); // Get &str from Option<String>

        self.spotify.start_uris_playback(
            vec![playable_id], // This is the 'uris' argument
            device_id_str,        // This is the 'device_id' argument
            None,                 // This is the 'offset' argument
            None                  // This is the 'position' argument
        ).await?;
        Ok(())
    }

    
}

pub async fn run_app(spotify: AuthCodePkceSpotify) -> Result<()> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(spotify).await?;

    // Initial fetch of tracks for the initially selected playlist
    app.fetch_tracks_for_selected_playlist().await?;

    let mut should_quit = false;
    while !should_quit {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                .split(f.size());

            let playlists_items: Vec<ListItem> = app.playlists
                .iter()
                .map(|p| ListItem::new(p.0.as_str()))
                .collect();
            let playlists_list = List::new(playlists_items)
                .block(Block::default().borders(Borders::ALL).title("Playlists"))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD))
                .highlight_symbol("> ");

            let tracks_items: Vec<ListItem> = app.tracks
                .iter()
                .map(|t| ListItem::new(t.0.as_str())) // Updated here
                .collect();
            let tracks_list = List::new(tracks_items)
                .block(Block::default().borders(Borders::ALL).title("Tracks"))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD))
                .highlight_symbol("> ");

            match app.selected_panel {
                AppPanel::Playlists => {
                    f.render_stateful_widget(playlists_list.clone().block(Block::default().borders(Borders::ALL).title("Playlists").yellow()), chunks[0], &mut app.playlists_state);
                    f.render_stateful_widget(tracks_list, chunks[1], &mut app.tracks_state);
                }
                AppPanel::Tracks => {
                    f.render_stateful_widget(playlists_list, chunks[0], &mut app.playlists_state);
                    f.render_stateful_widget(tracks_list.clone().block(Block::default().borders(Borders::ALL).title("Tracks").yellow()), chunks[1], &mut app.tracks_state);
                }
            }
        })?;

        if event::poll(std::time::Duration::from_millis(250))? { // Poll for events with a timeout
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => {
                            should_quit = true;
                        }
                        KeyCode::Tab => {
                            app.selected_panel = match app.selected_panel {
                                AppPanel::Playlists => AppPanel::Tracks,
                                AppPanel::Tracks => AppPanel::Playlists,
                            };
                        }
                        KeyCode::Up => {
                            match app.selected_panel {
                                AppPanel::Playlists => app.previous_playlist(),
                                AppPanel::Tracks => app.previous_track(),
                            }
                        }
                        KeyCode::Down => {
                            match app.selected_panel {
                                AppPanel::Playlists => app.next_playlist(),
                                AppPanel::Tracks => app.next_track(),
                            }
                        }
                        KeyCode::Enter => {
                            match app.selected_panel {
                                AppPanel::Playlists => {
                                    if let Some(selected_index) = app.playlists_state.selected() {
                                        app.selected_playlist_id = Some(app.playlists[selected_index].1.clone());
                                        app.fetch_tracks_for_selected_playlist().await?;
                                    }
                                }
                                AppPanel::Tracks => {
                                    if let Some(selected_index) = app.tracks_state.selected() {
                                        let track_uri = app.tracks[selected_index].1.clone();
                                        app.play_track(track_uri).await?;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}