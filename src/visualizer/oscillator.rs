use std::sync::mpsc::Receiver;
use ratatui::style::Color;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{Block, Borders};
use ratatui::layout::Rect;
use ratatui::Frame;

const BUF_SIZE: usize = 2048;

pub struct OscillatorState {
    buf: Vec<f32>,
    pub rx: Receiver<Vec<f32>>,
}

impl OscillatorState {
    pub fn new(rx: Receiver<Vec<f32>>) -> Self {
        Self {
            buf: Vec::with_capacity(BUF_SIZE),
            rx,
        }
    }

    pub fn poll(&mut self) {
        while let Ok(chunk) = self.rx.try_recv() {
            self.buf.extend_from_slice(&chunk);
            if self.buf.len() > BUF_SIZE {
                let drain = self.buf.len() - BUF_SIZE;
                self.buf.drain(..drain);
            }
        }
    }

    pub fn tick(&mut self) {
        // There are no falling wave peaks
    }

    // Shrink the buffer to fit the screen width.
    pub fn waveform_samples(&self, n_points: usize) -> Vec<(f64, f64)> {
        if self.buf.is_empty() || n_points == 0 {
            return Vec::new();
        }
        let total = self.buf.len();
        let step = (total / n_points).max(1);
        self.buf
            .chunks(step)
            .take(n_points)
            .enumerate()
            .map(|(i, chunk)| {
                let avg = chunk.iter().sum::<f32>() / chunk.len() as f32;
                (i as f64, avg.clamp(-1.0, 1.0) as f64)
            })
            .collect()
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let n_pts = (area.width as usize * 2).max(1);
        let points = self.waveform_samples(n_pts);
        let x_max = n_pts as f64;

        // Gradient color helper: maps x position 0..1 to cyan->magenta.
        let wave_color = |x: f64| -> Color {
            let t = (x / x_max).clamp(0.0, 1.0) as f32;
            let r = (t * 220.0) as u8;
            let g = ((1.0 - t) * 200.0) as u8;
            Color::Rgb(r, g, 230)
        };

        let canvas = Canvas::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Visualizer "),
            )
            .x_bounds([0.0, x_max])
            .y_bounds([-1.05, 1.05])
            .paint(move |ctx| {
                // Zero-line (dim)
                ctx.draw(&CanvasLine {
                    x1: 0.0,
                    y1: 0.0,
                    x2: x_max,
                    y2: 0.0,
                    color: Color::DarkGray,
                });
                // Waveform segments
                for w in points.windows(2) {
                    ctx.draw(&CanvasLine {
                        x1: w[0].0,
                        y1: w[0].1,
                        x2: w[1].0,
                        y2: w[1].1,
                        color: wave_color(w[0].0),
                    });
                }
            });
        frame.render_widget(canvas, area);
    }
}
