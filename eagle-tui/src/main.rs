//! eagle-tui — Eagle Validation Suite TUI.
//!
//! Standalone launcher for LQS compliance, benchmarks, clinical validation.
//! Same layout as the hub's EaglePanel but runs independently.

use std::io;
use std::process::{Command, ExitCode};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use ratatui::widgets::*;

// ── Constants ─────────────────────────────────────────────────────────────────

const EAGLE_LOGO: &[&str] = &[
    "  ███████╗ █████╗  ██████╗ ██╗     ███████╗",
    "  ██╔════╝██╔══██╗██╔════╝ ██║     ██╔════╝",
    "  █████╗  ███████║██║  ███╗██║     █████╗  ",
    "  ██╔══╝  ██╔══██║██║   ██║██║     ██╔══╝  ",
    "  ███████╗██║  ██║╚██████╔╝███████╗███████╗",
    "  ╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚══════╝",
];

const LOGO_MIN_HEIGHT: u16 = 30;

// ── Launch targets ────────────────────────────────────────────────────────────

fn eagle_subcommand(args: &[&str]) {
    let mut cmd = Command::new("eagle");
    cmd.args(args);
    match cmd.status() {
        Ok(s) => {
            if !s.success() {
                eprintln!("eagle exited with {}", s.code().unwrap_or(1));
            }
        }
        Err(e) => eprintln!("failed to run eagle: {e}"),
    }
}

// ── App ───────────────────────────────────────────────────────────────────────

struct App {
    selected: usize,
    should_quit: bool,
    status: String,
}

impl App {
    fn new() -> Self {
        Self {
            selected: 0,
            should_quit: false,
            status: String::new(),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            // Compliance
            KeyCode::Char('1') => {
                self.status = "Running LQS compliance (full)...".into();
                eagle_subcommand(&["--full"]);
            }
            KeyCode::Char('2') => {
                self.status = "Running quick quality check...".into();
                eagle_subcommand(&["--quick"]);
            }
            KeyCode::Char('3') => {
                self.status = "Targeted level — not yet implemented.".into();
            }
            // Benchmarking
            KeyCode::Char('4') => {
                self.status = "Running performance suite...".into();
                eagle_subcommand(&["--perf"]);
            }
            KeyCode::Char('5') => {
                self.status = "Running rate-distortion sweep...".into();
                eagle_subcommand(&["--rd"]);
            }
            KeyCode::Char('6') => {
                self.status = "Running head-to-head...".into();
                eagle_subcommand(&["--h2h"]);
            }
            // Clinical
            KeyCode::Char('7') => {
                self.status = "Downstream tasks — coming in v1.1.".into();
            }
            KeyCode::Char('8') => {
                self.status = "Hallucination tests — coming in v1.1.".into();
            }
            // Exploration
            KeyCode::Char('9') => {
                self.status = "Metrics explorer — see eagle/runs/<latest>/metrics.json.".into();
            }
            // Registry
            KeyCode::Char('p') => {
                self.status = "Publish badge — coming in v1.1.".into();
            }
            KeyCode::Char('r') => {
                self.status = "Leaderboard — http://eagle.openhuman.tech/leaderboard.".into();
            }
            // Export
            KeyCode::Char('x') => {
                self.status = "Export — coming in v1.1.".into();
            }
            // Navigation
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
    }
}

// ── Render ────────────────────────────────────────────────────────────────────

fn ui(f: &mut Frame, app: &App) {
    let area = f.area();
    let show_logo = area.height >= LOGO_MIN_HEIGHT;
    let head_h: u16 = if show_logo { 8 } else { 2 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(head_h),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // Banner
    let mut head: Vec<Line> = Vec::new();
    if show_logo {
        for row in EAGLE_LOGO {
            head.push(Line::from(Span::styled(
                *row,
                Style::default().fg(Color::Cyan),
            )));
        }
        head.push(Line::from(""));
        head.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "OpenHuman Eagle  ·  Validation Suite for EEG Processing",
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    } else {
        head.push(Line::from(vec![
            Span::styled(
                "  EAGLE",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled(
                "Validation Suite for EEG Processing",
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        head.push(Line::from(""));
    }
    f.render_widget(Paragraph::new(head), chunks[0]);

    // Info box
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " Eagle Validation Suite ",
            Style::default().fg(Color::DarkGray),
        ));
    let inner = block.inner(chunks[1]);
    f.render_widget(block, chunks[1]);
    let info = vec![
        kv_line("Mode", "lossless", Style::default()),
        kv_line("Target", "LQS-L/C/M/A compliance", Style::default()),
        kv_line("Status", "ready to test", Style::default().fg(Color::Green)),
    ];
    f.render_widget(Paragraph::new(info), inner);

    // Options in two columns
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(2)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);

    let left = vec![
        section_header("COMPLIANCE", "verify"),
        option_row("1", "LQS compliance test", "all 4 levels"),
        option_row("2", "Quick quality check", "30s sanity"),
        option_row("3", "Targeted level", "one level"),
        Line::from(""),
        section_header("BENCHMARKING", "perform"),
        option_row("4", "Performance suite", "p50/95/99"),
        option_row("5", "Rate-distortion sweep", "vs CR"),
        option_row("6", "Head-to-head", "gzip, zstd"),
    ];
    let right = vec![
        section_header("CLINICAL VALIDATION", "safe?"),
        option_row("7", "Downstream tasks", "seizure"),
        option_row("8", "Hallucination tests", "fabrication"),
        Line::from(""),
        section_header("EXPLORATION", ""),
        option_row("9", "Metrics explorer", "last run"),
        Line::from(""),
        section_header("REGISTRY", ""),
        option_row("p", "Publish badge", "certificate"),
        option_row("r", "Leaderboard", "field state"),
    ];
    f.render_widget(Paragraph::new(left), cols[0]);
    f.render_widget(Paragraph::new(right), cols[1]);

    // Footer
    let status = if app.status.is_empty() {
        "  [1-9/p/r] select  [q] quit".to_string()
    } else {
        format!("  {}", app.status)
    };
    f.render_widget(Paragraph::new(status), chunks[3]);
}

fn kv_line<'a>(label: &str, value: &str, vstyle: Style) -> Line<'a> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("{:<8}", label), Style::default().fg(Color::DarkGray)),
        Span::styled(value.to_string(), vstyle),
    ])
}

fn section_header<'a>(title: &str, sub: &str) -> Line<'a> {
    let mut spans = vec![Span::styled(
        title.to_string(),
        Style::default().fg(Color::Cyan),
    )];
    if !sub.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            sub.to_string(),
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

fn option_row<'a>(key: &str, label: &str, desc: &str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("[{}]", key), Style::default().fg(Color::Yellow)),
        Span::raw(" "),
        Span::styled(format!("{:<22}", label), Style::default()),
        Span::styled(desc.to_string(), Style::default().fg(Color::DarkGray)),
    ])
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> ExitCode {
    enable_raw_mode().expect("raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("alt screen");
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("terminal");

    let mut app = App::new();

    loop {
        terminal.draw(|f| ui(f, &app)).expect("draw");
        if app.should_quit {
            break;
        }
        if event::poll(std::time::Duration::from_millis(100)).expect("poll") {
            if let Event::Key(key) = event::read().expect("read") {
                app.handle_key(key);
            }
        }
    }

    disable_raw_mode().expect("disable raw");
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    ExitCode::SUCCESS
}
