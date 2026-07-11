use std::time::Duration;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Bar, BarChart, BarGroup, Block, BorderType, Borders, Gauge, List, ListItem, ListState,
    Paragraph, Sparkline,
};
use ratatui::Frame;

use crate::app::{App, PlayState, RandomMode, SettingsFocus, View};
use crate::visualizer::OscillatorState;

pub struct Theme {
    pub name: &'static str,
    pub primary: Color,
    pub fg: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub dir_fg: Color,
}

pub const THEMES: [Theme; 5] = [
    Theme {
        name: "Default",
        primary: Color::Cyan,
        fg: Color::White,
        highlight_bg: Color::Cyan,
        highlight_fg: Color::Black,
        dir_fg: Color::Blue,
    },
    Theme {
        name: "Dracula",
        primary: Color::Magenta,
        fg: Color::White,
        highlight_bg: Color::LightMagenta,
        highlight_fg: Color::Black,
        dir_fg: Color::LightMagenta,
    },
    Theme {
        name: "Gruvbox",
        primary: Color::Yellow,
        fg: Color::White,
        highlight_bg: Color::Yellow,
        highlight_fg: Color::Black,
        dir_fg: Color::LightYellow,
    },
    Theme {
        name: "Nord",
        primary: Color::LightBlue,
        fg: Color::White,
        highlight_bg: Color::LightBlue,
        highlight_fg: Color::Black,
        dir_fg: Color::Cyan,
    },
    Theme {
        name: "Neon",
        primary: Color::LightGreen,
        fg: Color::White,
        highlight_bg: Color::LightGreen,
        highlight_fg: Color::Black,
        dir_fg: Color::Green,
    },
];

pub fn draw(frame: &mut Frame, app: &mut App, osc: &OscillatorState) {
    let root = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab bar
            Constraint::Min(3),    // main content
            Constraint::Length(3), // now-playing mini bar
            Constraint::Length(1), // status/help line
        ])
        .split(root);

    draw_tab_bar(frame, chunks[0], app);

    match app.view {
        View::Browser => draw_browser(frame, chunks[1], app),
        View::NowPlaying => draw_now_playing(frame, chunks[1], app, osc),
        View::Settings => draw_settings(frame, chunks[1], app),
    }

    draw_mini_player(frame, chunks[2], app);
    draw_status_line(frame, chunks[3], app);
}

fn draw_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &THEMES[app.theme_idx];
    let make = |label: &str, active: bool| {
        let style = if active {
            Style::default()
                .fg(theme.highlight_fg)
                .bg(theme.highlight_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Span::styled(format!(" {label} "), style)
    };

    let tabs_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(35),
            Constraint::Min(0),
            Constraint::Length(35),
        ])
        .split(area);

    let left_span = Line::from(vec![
        make("[1] Browser", app.view == View::Browser),
        Span::raw(" "),
        make("[2] Now Playing", app.view == View::NowPlaying),
    ]);
    frame.render_widget(
        Paragraph::new(left_span).alignment(Alignment::Left),
        tabs_layout[0],
    );

    let center_span = Line::from(vec![Span::styled(
        "FUSKYOM — Terminal Music Player",
        Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(
        Paragraph::new(center_span).alignment(Alignment::Center),
        tabs_layout[1],
    );

    let right_span = Line::from(vec![make("[9] Settings", app.view == View::Settings)]);
    frame.render_widget(
        Paragraph::new(right_span).alignment(Alignment::Right),
        tabs_layout[2],
    );
}

fn draw_browser(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = &THEMES[app.theme_idx];
    let area = if app.search_active {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);
        draw_search_box(frame, split[0], app);
        split[1]
    } else {
        area
    };

    let visible = app.visible_entries();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|e| {
            let label = if e.is_dir {
                format!("📁 {}/", e.name)
            } else {
                format!("🎵 {}", e.name)
            };
            let style = if e.is_dir {
                Style::default().fg(theme.dir_fg)
            } else {
                Style::default().fg(theme.fg)
            };
            ListItem::new(Span::styled(label, style))
        })
        .collect();

    let title = if app.at_root_picker {
        " Pick a library ".to_string()
    } else if app.search_query.is_empty() {
        format!(" {} ", app.cwd.display())
    } else {
        format!(" {} ({} results) ", app.cwd.display(), items.len())
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_search_box(frame: &mut Frame, area: Rect, app: &App) {
    let text = format!("/{}", app.search_query);
    let box_widget = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Search (Esc or / to exit) ")
            .style(Style::default().fg(THEMES[app.theme_idx].primary)),
    );
    frame.render_widget(box_widget, area);
}

fn draw_now_playing(frame: &mut Frame, area: Rect, app: &mut App, osc: &OscillatorState) {
    let theme = &THEMES[app.theme_idx];

    // Terminal character cells are roughly twice as tall as they are wide, so
    // a square album cover needs about `height * 2` columns to fill its box
    // without chafa having to letterbox it (which is what left that big dead
    // gap before). We derive the art column's width from the pane's height
    // instead of a fixed percentage, so the reserved space actually matches
    // what a square cover needs -- clamped so it never eats more than 70% of
    // the width on a very short/wide terminal, nor collapses below something
    // usable on a very tall/narrow one.
    let width = area.width as u32;
    let height = area.height as u32;
    let max_allowed = (width * 7 / 10).max(15);
    let art_width = (height * 2)
        .clamp(15, max_allowed)
        .min(width.saturating_sub(15))
        .max(10) as u16;

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(art_width.max(16)), Constraint::Min(20)])
        .split(area);

    let art_block = Block::default().borders(Borders::ALL).title(" Album Art ");
    let inner = art_block.inner(cols[0]);
    frame.render_widget(art_block, cols[0]);

    let vis_height = if app.show_visualizer {
        if app.vis_style == 2 {
            8
        } else {
            1
        }
    } else {
        0
    };
    let art_constraints = if app.show_visualizer {
        vec![
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(vis_height),
        ]
    } else {
        vec![Constraint::Min(5)]
    };

    let art_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(art_constraints)
        .split(inner);

    if let Some(track) = app.current_track.clone() {
        let w = art_layout[0].width.saturating_sub(1);
        let h = art_layout[0].height.saturating_sub(1);
        if let Some(art) = app.art.render(&track, w, h) {
            frame.render_widget(Paragraph::new(art), art_layout[0]);
        } else if app.art.chafa_missing {
            frame.render_widget(
                Paragraph::new("chafa not installed.\nsudo apt install chafa"),
                art_layout[0],
            );
        } else {
            frame.render_widget(Paragraph::new("(no embedded cover art)"), art_layout[0]);
        }
    } else {
        frame.render_widget(Paragraph::new("Nothing playing"), art_layout[0]);
    }

    if app.show_visualizer {
        let sep = Block::default()
            .borders(Borders::TOP)
            .border_type(BorderType::Double);
        frame.render_widget(sep, art_layout[1]);

        if app.vis_style == 0 || app.vis_style == 1 {
            let mut data = Vec::new();
            let num_bars = (art_layout[2].width / 3).max(1) as usize;
            let num_spark = art_layout[2].width.max(1) as usize;
            
            let elements = if app.vis_style == 0 { num_bars } else { num_spark };
            let is_playing = app.play_state == PlayState::Playing;

            for i in 0..elements {
                let mut val = 0;
                if is_playing {
                    val = ((app.position.as_millis() / 40) as u64 + (i as u64 * 11)) % 100;
                    val = ((val as f32 / 100.0).powf(1.5) * 100.0) as u64;
                } else if app.current_track.is_some() {
                    val = 10;
                }
                data.push(val);
            }

            if app.vis_style == 0 {
                let bars: Vec<Bar> = data
                    .into_iter()
                    .map(|v| Bar::default().value(v).text_value(String::new()))
                    .collect();
                let bg = BarGroup::default().bars(&bars);
                let barchart = BarChart::default()
                    .data(bg)
                    .bar_width(2)
                    .bar_gap(1)
                    .max(100)
                    .bar_style(Style::default().fg(theme.primary));
                frame.render_widget(barchart, art_layout[2]);
            } else {
                let sparkline = Sparkline::default()
                    .data(&data)
                    .max(100)
                    .style(Style::default().fg(theme.primary));
                frame.render_widget(sparkline, art_layout[2]);
            }
        } else {
            osc.draw(frame, art_layout[2]);
        }
    }

    let top_h = 8u16; // track info fits inside
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top_h), // top row (track)
            Constraint::Min(3),        // queue
        ])
        .split(cols[1]);

    let info_lines = track_info_lines(app);
    frame.render_widget(
        Paragraph::new(info_lines).block(Block::default().borders(Borders::ALL).title(" Track ")),
        right[0],
    );

    let queue_items: Vec<ListItem> = app
        .queue
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let is_current = app.queue_pos == Some(i);
            let label = if is_current {
                format!("▶ {name}")
            } else {
                format!("  {name}")
            };
            let style = if is_current {
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Span::styled(label, style))
        })
        .collect();

    let mut queue_state = ListState::default();
    if let Some(pos) = app.queue_pos {
        queue_state.select(Some(pos));
    }

    frame.render_stateful_widget(
        List::new(queue_items).block(Block::default().borders(Borders::ALL).title(" Queue ")),
        right[1],
        &mut queue_state,
    );
}

fn track_info_lines(app: &App) -> Vec<Line<'static>> {
    let Some(track) = &app.current_track else {
        return vec![Line::from("No track playing")];
    };
    let name = track
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let state = match app.play_state {
        PlayState::Playing => "▶ playing",
        PlayState::Paused => "⏸ paused",
        PlayState::Stopped => "⏹ stopped",
    };

    // Aquí implementamos el texto visual del Randomizer
    let rand_str = match app.random_mode {
        RandomMode::Off => "off",
        RandomMode::Album => "album",
        RandomMode::Global => "global",
    };

    vec![
        Line::from(Span::styled(
            name,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "{state}    vol {:.0}%    repeat (r) {}    random (d) {}",
            app.volume * 100.0,
            if app.repeat { "on" } else { "off" },
            rand_str
        )),
    ]
}

fn draw_settings(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &THEMES[app.theme_idx];
    let area = if app.adding_path {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);
        draw_path_input(frame, split[0], app);
        split[1]
    } else {
        area
    };

    let splits = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let left_splits = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(8)])
        .split(splits[0]);

    let right_splits = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(7),
        ])
        .split(splits[1]);

    let dir_items: Vec<ListItem> = if app.library_roots.is_empty() {
        vec![ListItem::new(
            "(no directories configured -- press 'a' to add one)",
        )]
    } else {
        app.library_roots
            .iter()
            .map(|p| ListItem::new(p.display().to_string()))
            .collect()
    };

    let dir_border = if app.settings_focus == SettingsFocus::Directories {
        theme.primary
    } else {
        Color::DarkGray
    };
    let dir_list = List::new(dir_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Default music directories ")
                .style(Style::default().fg(dir_border)),
        )
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut dir_state = ListState::default();
    if !app.library_roots.is_empty() {
        dir_state.select(Some(
            app.settings_selected
                .min(app.library_roots.len().saturating_sub(1)),
        ));
    }
    frame.render_stateful_widget(dir_list, left_splits[0], &mut dir_state);

    let theme_items: Vec<ListItem> = THEMES
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut text = format!(" [ ] {} ", t.name);
            if i == app.theme_idx {
                text = format!(" [*] {} ", t.name);
            }
            ListItem::new(Span::styled(text, Style::default().fg(t.primary)))
        })
        .collect();

    let theme_border = if app.settings_focus == SettingsFocus::Themes {
        theme.primary
    } else {
        Color::DarkGray
    };
    let theme_list = List::new(theme_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Themes (Enter to Apply) ")
                .style(Style::default().fg(theme_border)),
        )
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut theme_state = ListState::default();
    theme_state.select(Some(app.theme_selected));
    frame.render_stateful_widget(theme_list, left_splits[1], &mut theme_state);

    let vis_text = if app.show_visualizer {
        "[*] Show Wave Visualizer"
    } else {
        "[ ] Show Wave Visualizer"
    };
    let creator_style = if app.vis_style == 1 {
        " [*] Style: Sparkline"
    } else if app.vis_style == 0 {
        " [*] Style: Bar Chart"
    } else {
        " [ ] Style: Bar Chart"
    };
    let style_osc = if app.vis_style == 2 {
        " [*] Style: Oscillator"
    } else {
        " [ ] Style: Oscillator"
    };

    let opt_items = vec![
        ListItem::new(vis_text),
        ListItem::new(creator_style),
        ListItem::new(style_osc),
    ];

    let opt_border = if app.settings_focus == SettingsFocus::Options {
        theme.primary
    } else {
        Color::DarkGray
    };
    let opt_list = List::new(opt_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Options (Enter to Toggle) ")
                .style(Style::default().fg(opt_border)),
        )
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut opt_state = ListState::default();
    opt_state.select(Some(app.options_selected));
    frame.render_stateful_widget(opt_list, right_splits[0], &mut opt_state);

    let donation_text = vec![
        Line::from("Hi, thank you for using this app!"),
        Line::from("if you are enjoying, you can invite me a"),
        Line::from("coffee to keep the hard work for you!"),
        Line::from(""),
        Line::from(vec![
            Span::raw("☕ "),
            Span::styled(
                "buymeacoffee.com/daveness",
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    let donation_block = Paragraph::new(donation_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Support ☕ ")
                .style(Style::default().fg(Color::DarkGray)),
        )
        .alignment(Alignment::Center);
    frame.render_widget(donation_block, right_splits[2]);
}

fn draw_path_input(frame: &mut Frame, area: Rect, app: &App) {
    let text = format!("Path: {}", app.new_path_input);
    let widget = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" New directory (Enter to confirm, Esc to cancel) ")
            .style(Style::default().fg(THEMES[app.theme_idx].primary)),
    );
    frame.render_widget(widget, area);
}

fn draw_mini_player(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &THEMES[app.theme_idx];
    let pos = fmt_duration(app.position);
    let dur = app
        .current_duration
        .map(fmt_duration)
        .unwrap_or_else(|| "--:--".to_string());

    let name = app
        .current_track
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "— no track playing —".to_string());

    let ratio = match (app.current_duration, app.current_track.is_some()) {
        (Some(total), true) if total.as_secs_f64() > 0.0 => {
            (app.position.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };

    let state_symbol = match app.play_state {
        PlayState::Playing => "▶",
        PlayState::Paused => "⏸",
        PlayState::Stopped => "⏹",
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {state_symbol} {name} ")),
        )
        .gauge_style(Style::default().fg(theme.primary).bg(Color::Black))
        .ratio(ratio)
        .label(format!("{pos} / {dur}"));

    frame.render_widget(gauge, area);
}

fn draw_status_line(frame: &mut Frame, area: Rect, app: &App) {
    let help = match app.view {
        View::Settings => {
            "(Tab/<-/->) panels | (j/k) move | (a) add path | (d) remove | (r) refresh | (1/2/9) view | (q) exit"
        }
        _ => {
            "(<-/->) move | (/) search | (Enter) play | (space) pause | (n/p) next/prev | (s) stop | (+/-) vol | (r) repeat | (1/2/9) view | (q) exit | (d) random"
        }
    };
    let line = if app.status.is_empty() {
        help.to_string()
    } else {
        format!("{}   |   {help}", app.status)
    };
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
