//! Lightweight persistent animation for the TUI shell.
//!
//! The renderer is deliberately pure: it derives every frame from a monotonically
//! increasing tick and never performs I/O. This keeps the visual layer independent
//! from device operations and lets the event loop continue drawing while workers run.

use std::time::Duration;

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub const TICK_INTERVAL_MS: u64 = 80;
pub const REDUCED_TICK_INTERVAL_MS: u64 = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionMode {
    Full,
    Reduced,
    Off,
}

impl MotionMode {
    pub fn from_env() -> Self {
        match std::env::var("EDPCLI_ANIMATION") {
            Ok(value) if value.eq_ignore_ascii_case("off") => Self::Off,
            Ok(value) if value.eq_ignore_ascii_case("reduced") => Self::Reduced,
            _ => Self::Full,
        }
    }

    pub const fn tick_interval(self) -> Option<Duration> {
        match self {
            Self::Full => Some(Duration::from_millis(TICK_INTERVAL_MS)),
            Self::Reduced => Some(Duration::from_millis(REDUCED_TICK_INTERVAL_MS)),
            Self::Off => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreMode {
    Stable,
    Busy,
    Guard,
}

impl CoreMode {
    fn label(self) -> &'static str {
        match self {
            Self::Stable => "STABLE",
            Self::Busy => "ACTIVE",
            Self::Guard => "GUARDED",
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Stable => Style::default().fg(Color::Green),
            Self::Busy => Style::default().fg(Color::Yellow),
            Self::Guard => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Dot {
    Empty,
    Dim,
    Accent,
    Hot,
    Core,
}

impl Dot {
    fn glyph(self, tick: u64, x: usize, y: usize) -> &'static str {
        match self {
            Self::Empty => " ",
            Self::Dim => {
                if (tick as usize + x + y).is_multiple_of(4) {
                    "⠂"
                } else {
                    "·"
                }
            }
            Self::Accent => {
                if (tick as usize / 2 + x * 3 + y).is_multiple_of(7) {
                    "○"
                } else {
                    "∙"
                }
            }
            Self::Hot => "◆",
            Self::Core => {
                if (tick / 3 + (x + y) as u64).is_multiple_of(2) {
                    "▓"
                } else {
                    "▒"
                }
            }
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Empty => Style::default(),
            Self::Dim => Style::default().fg(Color::DarkGray),
            Self::Accent => Style::default().fg(Color::Cyan),
            Self::Hot => Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            Self::Core => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        }
    }
}

fn plot(grid: &mut [Vec<Dot>], x: f64, y: f64, dot: Dot) {
    let x = x.round() as isize;
    let y = y.round() as isize;
    if x < 0 || y < 0 {
        return;
    }
    let (x, y) = (x as usize, y as usize);
    let Some(row) = grid.get_mut(y) else {
        return;
    };
    let Some(cell) = row.get_mut(x) else {
        return;
    };
    if dot > *cell {
        *cell = dot;
    }
}

fn draw_orbit(
    grid: &mut [Vec<Dot>],
    center: (f64, f64),
    radius: (f64, f64),
    phase: f64,
    points: usize,
    dot: Dot,
) {
    let (cx, cy) = center;
    let (rx, ry) = radius;
    for i in 0..points {
        let theta = std::f64::consts::TAU * i as f64 / points as f64 + phase;
        let wobble = (theta * 3.0 - phase * 0.7).sin() * 0.45;
        plot(
            grid,
            cx + theta.cos() * rx + wobble,
            cy + theta.sin() * ry,
            dot,
        );
    }
}

fn core_grid(tick: u64, width: usize, height: usize) -> Vec<Vec<Dot>> {
    let mut grid = vec![vec![Dot::Empty; width]; height];
    if width < 5 || height < 5 {
        return grid;
    }

    let cx = (width.saturating_sub(1)) as f64 / 2.0;
    let cy = (height.saturating_sub(1)) as f64 / 2.0;
    let rx = (width as f64 * 0.43).max(2.0);
    let ry = (height as f64 * 0.40).max(2.0);
    let phase = tick as f64 * 0.085;

    draw_orbit(&mut grid, (cx, cy), (rx, ry), phase, 64, Dot::Dim);
    draw_orbit(
        &mut grid,
        (cx, cy),
        (rx * 0.72, ry * 0.72),
        -phase * 1.35,
        48,
        Dot::Accent,
    );

    // Rotating spokes give the core a mechanical/HUD feel without requiring
    // image protocols or terminal-specific graphics extensions.
    for spoke in 0..6 {
        let theta = phase * 0.72 + std::f64::consts::TAU * spoke as f64 / 6.0;
        for step in 2..=8 {
            let t = step as f64 / 10.0;
            plot(
                &mut grid,
                cx + theta.cos() * rx * t,
                cy + theta.sin() * ry * t,
                if step == 8 { Dot::Hot } else { Dot::Accent },
            );
        }
    }

    let core_rx = (rx * 0.28).max(1.4);
    let core_ry = (ry * 0.34).max(1.2);
    for y in 0..height {
        for x in 0..width {
            let nx = (x as f64 - cx) / core_rx;
            let ny = (y as f64 - cy) / core_ry;
            if nx * nx + ny * ny <= 1.0 {
                plot(&mut grid, x as f64, y as f64, Dot::Core);
            }
        }
    }

    // One bright tracker circles independently so consecutive frames remain
    // visually obvious even when the terminal uses a low-refresh font renderer.
    let tracker = phase * 1.9;
    plot(
        &mut grid,
        cx + tracker.cos() * rx,
        cy + tracker.sin() * ry,
        Dot::Hot,
    );

    grid
}

fn centered_grid_lines(tick: u64, width: usize, height: usize) -> Vec<Line<'static>> {
    let canvas_width = width.clamp(5, 25);
    let canvas_height = height.clamp(5, 13);
    let grid = core_grid(tick, canvas_width, canvas_height);
    let left_pad = width.saturating_sub(canvas_width) / 2;

    grid.into_iter()
        .enumerate()
        .map(|(y, row)| {
            let mut spans = Vec::with_capacity(row.len() + 1);
            spans.push(Span::raw(" ".repeat(left_pad)));
            for (x, dot) in row.into_iter().enumerate() {
                spans.push(Span::styled(dot.glyph(tick, x, y), dot.style()));
            }
            Line::from(spans)
        })
        .collect()
}

pub fn compact_indicator(tick: u64, mode: CoreMode) -> Span<'static> {
    const FRAMES: [&str; 8] = ["◇", "◈", "◆", "◈", "◇", "◌", "○", "◌"];
    let glyph = FRAMES[(tick as usize) % FRAMES.len()];
    Span::styled(
        format!("  CORE {glyph} {}", mode.label()),
        mode.style().add_modifier(Modifier::BOLD),
    )
}

pub fn draw(frame: &mut Frame, area: Rect, tick: u64, mode: CoreMode, activity: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" EDP CORE · LIVE ")
        .title_style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let width = inner.width as usize;
    let height = inner.height as usize;
    let reserved = 4usize.min(height);
    let visual_height = height.saturating_sub(reserved).max(1);

    let mut lines = Vec::with_capacity(height);
    lines.push(Line::from(vec![
        Span::styled("STABILITY // ", Style::default().fg(Color::DarkGray)),
        Span::styled(mode.label(), mode.style()),
    ]));
    lines.push(Line::from(Span::styled(
        format!("ACTIVITY  // {activity}"),
        Style::default().fg(Color::DarkGray),
    )));
    lines.extend(centered_grid_lines(tick, width, visual_height));
    lines.push(Line::from(Span::styled(
        "SECTOR VIEW // LBA 00-12",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(vec![
        Span::styled("FRAME      // ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:06}", tick % 1_000_000),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Left), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_grid_changes_between_animation_phases() {
        assert_ne!(core_grid(0, 25, 13), core_grid(7, 25, 13));
    }

    #[test]
    fn compact_indicator_cycles_and_preserves_mode() {
        assert_ne!(
            compact_indicator(0, CoreMode::Stable).content,
            compact_indicator(2, CoreMode::Stable).content
        );
        assert_eq!(
            compact_indicator(0, CoreMode::Stable).content,
            compact_indicator(8, CoreMode::Stable).content
        );
        assert!(compact_indicator(0, CoreMode::Busy)
            .content
            .contains("ACTIVE"));
        assert!(compact_indicator(0, CoreMode::Guard)
            .content
            .contains("GUARDED"));
    }

    #[test]
    fn reduced_and_off_modes_change_animation_scheduling() {
        assert!(MotionMode::Reduced.tick_interval() > MotionMode::Full.tick_interval());
        assert_eq!(MotionMode::Off.tick_interval(), None);
    }
}
