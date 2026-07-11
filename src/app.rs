use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::art::ArtRenderer;
use crate::config;
use crate::library::{self, Entry};
use crate::player::{PlayerCommand, PlayerEvent};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum View {
    Browser,
    NowPlaying,
    Settings,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum SettingsFocus {
    Directories,
    Themes,
    Options,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum RandomMode {
    Off,
    Album,
    Global,
}

/// Generador de números seudoaleatorios muy rápido y ligero basado en el reloj
/// del sistema, para evitar añadir la dependencia externa `rand` al proyecto.
fn random_usize(max: usize) -> usize {
    if max == 0 {
        return 0;
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize ^ d.as_secs() as usize)
        .unwrap_or(0);
    let mut x = nanos;
    if x == 0 {
        x = 1;
    }
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x % max
}

pub struct App {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub dir_history: HashMap<PathBuf, usize>,

    pub library_roots: Vec<PathBuf>,
    pub at_root_picker: bool,

    pub view: View,
    pub should_quit: bool,

    pub search_active: bool,
    pub search_query: String,

    pub settings_selected: usize,
    pub adding_path: bool,
    pub new_path_input: String,

    pub theme_idx: usize,
    pub theme_selected: usize,
    pub show_visualizer: bool,
    pub vis_style: usize,
    pub settings_focus: SettingsFocus,
    pub options_selected: usize,

    pub queue: Vec<PathBuf>,
    pub queue_pos: Option<usize>,

    // Lista caché de todas las canciones en tu disco para el modo Global
    pub global_queue: Vec<PathBuf>,

    pub play_state: PlayState,
    pub current_track: Option<PathBuf>,
    pub current_duration: Option<Duration>,
    pub position: Duration,
    pub volume: f32,
    pub repeat: bool,
    pub random_mode: RandomMode,

    pub status: String,
    pub art: ArtRenderer,

    cmd_tx: Sender<PlayerCommand>,
}

impl App {
    pub fn new(library_roots: Vec<PathBuf>, cmd_tx: Sender<PlayerCommand>) -> Self {
        let mut app = Self {
            cwd: PathBuf::new(),
            entries: Vec::new(),
            selected: 0,
            dir_history: HashMap::new(),
            library_roots,
            at_root_picker: false,
            view: View::Browser,
            should_quit: false,
            search_active: false,
            search_query: String::new(),
            settings_selected: 0,
            adding_path: false,
            new_path_input: String::new(),

            theme_idx: config::load_theme(),
            theme_selected: config::load_theme(),
            show_visualizer: config::load_visualizer(),
            vis_style: config::load_vis_style(),
            settings_focus: SettingsFocus::Directories,
            options_selected: 0,

            queue: Vec::new(),
            queue_pos: None,
            global_queue: Vec::new(),
            play_state: PlayState::Stopped,
            current_track: None,
            current_duration: None,
            position: Duration::ZERO,
            volume: 1.0,
            repeat: false,
            random_mode: RandomMode::Off,
            status: "Welcome to fuskyom -- h for keybinds below, 9 for Settings".to_string(),
            art: ArtRenderer::new(),
            cmd_tx,
        };

        if app.library_roots.len() > 1 {
            app.show_root_picker();
        } else {
            app.cwd = app
                .library_roots
                .first()
                .cloned()
                .unwrap_or_else(|| PathBuf::from("."));
            app.refresh_entries();
        }
        app
    }

    fn show_root_picker(&mut self) {
        self.at_root_picker = true;
        self.entries = self
            .library_roots
            .iter()
            .map(|p| Entry {
                path: p.clone(),
                name: p.display().to_string(),
                is_dir: true,
            })
            .collect();
        self.selected = self
            .dir_history
            .get(&PathBuf::from("ROOT"))
            .copied()
            .unwrap_or(0);
        self.search_active = false;
        self.search_query.clear();
    }

    fn refresh_entries(&mut self) {
        self.at_root_picker = false;
        self.entries = library::read_dir_sorted(&self.cwd).unwrap_or_default();
        self.selected = self.dir_history.get(&self.cwd).copied().unwrap_or(0);
        self.clamp_selection();
        self.search_active = false;
        self.search_query.clear();
    }

    pub fn visible_entries(&self) -> Vec<&Entry> {
        if self.search_query.is_empty() {
            self.entries.iter().collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.entries
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&q))
                .collect()
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_entries().len();
        self.selected = if len == 0 {
            0
        } else {
            self.selected.min(len - 1)
        };
    }

    pub fn selected_entry(&self) -> Option<Entry> {
        self.visible_entries()
            .get(self.selected)
            .map(|e| (*e).clone())
    }

    pub fn move_selection(&mut self, delta: i32) {
        let len = self.visible_entries().len();
        if len == 0 {
            return;
        }
        let len = len as i32;
        let mut idx = self.selected as i32 + delta;
        idx = idx.clamp(0, len - 1);
        self.selected = idx as usize;
    }

    pub fn toggle_search(&mut self) {
        if self.search_active {
            self.close_search();
        } else {
            self.search_active = true;
            self.search_query.clear();
            self.selected = 0;
            self.view = View::Browser;
        }
    }

    fn close_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.selected = self.dir_history.get(&self.cwd).copied().unwrap_or(0);
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('/') => self.close_search(),
            KeyCode::Enter => {
                self.search_active = false;
                self.enter_or_play();
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.clamp_selection();
            }
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.clamp_selection();
            }
            _ => {}
        }
    }

    pub fn enter_or_play(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        if entry.is_dir {
            let hist_key = if self.at_root_picker {
                PathBuf::from("ROOT")
            } else {
                self.cwd.clone()
            };
            self.dir_history.insert(hist_key, self.selected);
            self.cwd = entry.path;
            self.refresh_entries();
        } else {
            self.play_from_dir_at(&entry.path);
        }
    }

    pub fn go_up(&mut self) {
        if self.at_root_picker {
            return;
        }
        self.dir_history.insert(self.cwd.clone(), self.selected);
        if self.library_roots.len() > 1 && self.library_roots.iter().any(|r| r == &self.cwd) {
            self.show_root_picker();
            return;
        }
        if let Some(parent) = self.cwd.parent() {
            let parent = parent.to_path_buf();
            self.cwd = parent;
            self.refresh_entries();
        }
    }

    fn play_from_dir_at(&mut self, track: &PathBuf) {
        let dir = track.parent().unwrap_or(&self.cwd);
        let files = library::audio_files_in(dir).unwrap_or_default();
        let start = files.iter().position(|p| p == track).unwrap_or(0);
        self.queue = files;
        self.queue_pos = Some(start);
        self.play_current_queue_item();

        self.view = View::NowPlaying;
    }

    fn play_current_queue_item(&mut self) {
        if let Some(pos) = self.queue_pos {
            if let Some(path) = self.queue.get(pos).cloned() {
                let _ = self.cmd_tx.send(PlayerCommand::Play(path));
            }
        }
    }

    pub fn toggle_pause(&mut self) {
        if self.current_track.is_some() {
            let _ = self.cmd_tx.send(PlayerCommand::TogglePause);
        }
    }

    pub fn stop(&mut self) {
        let _ = self.cmd_tx.send(PlayerCommand::Stop);
        self.play_state = PlayState::Stopped;
        self.current_track = None;
        self.position = Duration::ZERO;
    }

    pub fn next_track(&mut self) {
        match self.random_mode {
            RandomMode::Global => {
                if !self.global_queue.is_empty() {
                    let idx = random_usize(self.global_queue.len());
                    let path = self.global_queue[idx].clone();

                    // Sincronización Total de Contexto
                    // Cuando saltamos a una nueva pista global, actualizamos todo:
                    // la cola de reproducción y el explorador de archivos.
                    if let Some(dir) = path.parent() {
                        self.cwd = dir.to_path_buf();
                        self.refresh_entries();

                        // Resaltamos visualmente la canción en el navegador
                        self.selected = self
                            .entries
                            .iter()
                            .position(|e| e.path == path)
                            .unwrap_or(0);

                        // Cargamos todo el nuevo álbum a la cola para poder seguir navegando en él
                        if let Ok(files) = library::audio_files_in(dir) {
                            self.queue = files;
                            self.queue_pos = self.queue.iter().position(|p| p == &path);
                        }
                    }

                    let _ = self.cmd_tx.send(PlayerCommand::Play(path));
                } else {
                    self.status = "No songs found in global library".to_string();
                    self.stop();
                }
            }
            RandomMode::Album => {
                if !self.queue.is_empty() {
                    let idx = random_usize(self.queue.len());
                    self.queue_pos = Some(idx);
                    self.play_current_queue_item();
                }
            }
            RandomMode::Off => {
                if let Some(pos) = self.queue_pos {
                    if pos + 1 < self.queue.len() {
                        self.queue_pos = Some(pos + 1);
                        self.play_current_queue_item();
                    } else if self.repeat && !self.queue.is_empty() {
                        self.queue_pos = Some(0);
                        self.play_current_queue_item();
                    } else {
                        self.stop();
                        self.status = "End of queue".to_string();
                    }
                }
            }
        }
    }

    pub fn prev_track(&mut self) {
        match self.random_mode {
            RandomMode::Global | RandomMode::Album => self.next_track(),
            RandomMode::Off => {
                if let Some(pos) = self.queue_pos {
                    if pos > 0 {
                        self.queue_pos = Some(pos - 1);
                        self.play_current_queue_item();
                    }
                }
            }
        }
    }

    pub fn adjust_volume(&mut self, delta: f32) {
        self.volume = (self.volume + delta).clamp(0.0, 2.0);
        let _ = self.cmd_tx.send(PlayerCommand::SetVolume(self.volume));
        self.status = format!("Volume: {:.0}%", self.volume * 100.0);
    }

    pub fn seek_by(&mut self, secs: f32) {
        if self.current_track.is_some() {
            let _ = self.cmd_tx.send(PlayerCommand::SeekBy(secs));
        }
    }

    pub fn on_player_event(&mut self, event: PlayerEvent) {
        match event {
            PlayerEvent::Started { path, duration } => {
                self.current_track = Some(path);
                self.current_duration = duration;
                self.position = Duration::ZERO;
                self.play_state = PlayState::Playing;
            }
            PlayerEvent::Position(pos) => {
                self.position = pos;
            }
            PlayerEvent::Paused(paused) => {
                self.play_state = if paused {
                    PlayState::Paused
                } else {
                    PlayState::Playing
                };
            }
            PlayerEvent::Finished => {
                self.next_track();
            }
            PlayerEvent::Error(msg) => {
                self.status = format!("Error: {msg}");
            }
        }
    }

    fn start_add_path(&mut self) {
        self.adding_path = true;
        self.new_path_input.clear();
    }

    fn confirm_add_path(&mut self) {
        let raw = self.new_path_input.trim();
        if raw.is_empty() {
            self.adding_path = false;
            return;
        }
        let path = config::expand_tilde(raw);
        if !path.is_dir() {
            self.status = format!("Not a valid directory: {}", path.display());
            return;
        }
        if !self.library_roots.iter().any(|p| p == &path) {
            self.library_roots.push(path);
            self.library_roots.sort();
            self.library_roots.dedup();
            match config::save_library_paths(&self.library_roots) {
                Ok(()) => self.status = "Directory added and saved".to_string(),
                Err(e) => self.status = format!("Could not save config: {e}"),
            }
        }
        self.adding_path = false;
        self.new_path_input.clear();
        self.settings_selected = self
            .settings_selected
            .min(self.library_roots.len().saturating_sub(1));
    }

    fn remove_selected_path(&mut self) {
        if self.library_roots.is_empty() || self.settings_selected >= self.library_roots.len() {
            return;
        }
        let removed = self.library_roots.remove(self.settings_selected);
        self.settings_selected = self
            .settings_selected
            .min(self.library_roots.len().saturating_sub(1));

        match config::save_library_paths(&self.library_roots) {
            Ok(()) => self.status = "Directory removed".to_string(),
            Err(e) => self.status = format!("Could not save config: {e}"),
        }

        if self.cwd == removed || !self.cwd.exists() {
            self.refresh_from_disk();
        }
    }

    fn refresh_from_disk(&mut self) {
        let prev_cwd = self.cwd.clone();
        self.library_roots = config::load_library_paths()
            .into_iter()
            .filter(|p| p.is_dir())
            .collect();
        self.settings_selected = self
            .settings_selected
            .min(self.library_roots.len().saturating_sub(1));

        self.status = if self.library_roots.is_empty() {
            "No directories configured".to_string()
        } else {
            "Configuration reloaded".to_string()
        };

        if self.at_root_picker {
            self.show_root_picker();
        } else if !prev_cwd.exists()
            || (self.library_roots.len() > 1
                && prev_cwd.parent().is_none()
                && !self.library_roots.contains(&prev_cwd))
        {
            if self.library_roots.len() > 1 {
                self.show_root_picker();
            } else if let Some(first) = self.library_roots.first() {
                self.cwd = first.clone();
                self.refresh_entries();
            } else {
                self.cwd = PathBuf::from(".");
                self.refresh_entries();
            }
        } else {
            self.refresh_entries();
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        if self.adding_path {
            match key.code {
                KeyCode::Esc => {
                    self.adding_path = false;
                    self.new_path_input.clear();
                }
                KeyCode::Enter => self.confirm_add_path(),
                KeyCode::Backspace => {
                    self.new_path_input.pop();
                }
                KeyCode::Char(c) => self.new_path_input.push(c),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab => {
                self.settings_focus = match self.settings_focus {
                    SettingsFocus::Directories => SettingsFocus::Themes,
                    SettingsFocus::Themes => SettingsFocus::Options,
                    SettingsFocus::Options => SettingsFocus::Directories,
                };
            }
            KeyCode::Left => {
                self.settings_focus = match self.settings_focus {
                    SettingsFocus::Directories => SettingsFocus::Options,
                    SettingsFocus::Themes => SettingsFocus::Directories,
                    SettingsFocus::Options => SettingsFocus::Themes,
                };
            }
            KeyCode::Right => {
                self.settings_focus = match self.settings_focus {
                    SettingsFocus::Directories => SettingsFocus::Themes,
                    SettingsFocus::Themes => SettingsFocus::Options,
                    SettingsFocus::Options => SettingsFocus::Directories,
                };
            }
            KeyCode::Down | KeyCode::Char('j') => match self.settings_focus {
                SettingsFocus::Directories => {
                    if !self.library_roots.is_empty() {
                        self.settings_selected =
                            (self.settings_selected + 1).min(self.library_roots.len() - 1);
                    }
                }
                SettingsFocus::Themes => {
                    self.theme_selected = (self.theme_selected + 1).min(4);
                }
                SettingsFocus::Options => {
                    self.options_selected = (self.options_selected + 1).min(1);
                }
            },
            KeyCode::Up | KeyCode::Char('k') => match self.settings_focus {
                SettingsFocus::Directories => {
                    self.settings_selected = self.settings_selected.saturating_sub(1);
                }
                SettingsFocus::Themes => {
                    self.theme_selected = self.theme_selected.saturating_sub(1);
                }
                SettingsFocus::Options => {
                    self.options_selected = self.options_selected.saturating_sub(1);
                }
            },
            KeyCode::Enter => match self.settings_focus {
                SettingsFocus::Directories => {}
                SettingsFocus::Themes => {
                    self.theme_idx = self.theme_selected;
                    let _ = config::save_theme(self.theme_idx);
                    self.status = "Tema aplicado y guardado.".to_string();
                }
                SettingsFocus::Options => {
                    if self.options_selected == 0 {
                        self.show_visualizer = !self.show_visualizer;
                        let _ = config::save_visualizer(self.show_visualizer);
                        self.status = format!(
                            "Visualizador {}",
                            if self.show_visualizer {
                                "activado"
                            } else {
                                "desactivado"
                            }
                        );
                    } else if self.options_selected == 1 {
                        self.vis_style = if self.vis_style == 0 { 1 } else { 0 };
                        let _ = config::save_vis_style(self.vis_style);
                        self.status = format!(
                            "Estilo visualizador: {}",
                            if self.vis_style == 0 {
                                "Barras"
                            } else {
                                "Sparkline"
                            }
                        );
                    }
                }
            },
            KeyCode::Char('a') => {
                if self.settings_focus == SettingsFocus::Directories {
                    self.start_add_path();
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if self.settings_focus == SettingsFocus::Directories {
                    self.remove_selected_path();
                }
            }
            KeyCode::Char('r') => self.refresh_from_disk(),
            KeyCode::Char('1') => {
                self.view = View::Browser;
                if self.at_root_picker {
                    self.show_root_picker();
                }
            }
            KeyCode::Char('2') => self.view = View::NowPlaying,
            KeyCode::Char('9') => {
                self.view = View::Browser;
                if self.at_root_picker {
                    self.show_root_picker();
                }
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.search_active {
            self.handle_search_key(key);
            return;
        }
        if self.view == View::Settings {
            self.handle_settings_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }
            KeyCode::Char('/') => self.toggle_search(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Enter => self.enter_or_play(),
            KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                if self.view == View::Browser {
                    self.go_up();
                } else {
                    self.prev_track();
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.view == View::NowPlaying {
                    self.next_track();
                } else {
                    self.enter_or_play();
                }
            }
            KeyCode::Char(' ') => self.toggle_pause(),
            KeyCode::Char('s') => self.stop(),
            KeyCode::Char('n') => self.next_track(),
            KeyCode::Char('p') => self.prev_track(),
            KeyCode::Char('+') | KeyCode::Char('=') => self.adjust_volume(0.05),
            KeyCode::Char('-') => self.adjust_volume(-0.05),
            KeyCode::Char('.') => self.seek_by(5.0),
            KeyCode::Char(',') => self.seek_by(-5.0),
            KeyCode::Char('r') => {
                self.repeat = !self.repeat;
                self.status = format!("Repeat: {}", if self.repeat { "on" } else { "off" });
            }
            KeyCode::Char('d') => {
                self.random_mode = match self.random_mode {
                    RandomMode::Off => RandomMode::Album,
                    RandomMode::Album => {
                        if self.global_queue.is_empty() {
                            self.status = "Escaneando biblioteca global...".to_string();
                            self.global_queue =
                                library::all_audio_files_global(&self.library_roots);
                        }
                        RandomMode::Global
                    }
                    RandomMode::Global => RandomMode::Off,
                };

                let mode_str = match self.random_mode {
                    RandomMode::Off => "off",
                    RandomMode::Album => "album",
                    RandomMode::Global => "global",
                };
                self.status = format!("Random: {}", mode_str);
            }
            KeyCode::Char('1') => self.view = View::Browser,
            KeyCode::Char('2') => self.view = View::NowPlaying,
            KeyCode::Char('9') => self.view = View::Settings,
            KeyCode::Tab => {
                self.view = match self.view {
                    View::Browser => View::NowPlaying,
                    View::NowPlaying => View::Settings,
                    View::Settings => View::Browser,
                };
            }
            _ => {}
        }
    }
}
