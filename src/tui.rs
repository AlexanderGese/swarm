use crate::download::State;
use crossterm::{event, execute, terminal};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Gauge, Paragraph, Wrap};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn run(title: String, state: Arc<Mutex<State>>) -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen, crossterm::cursor::Hide)?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout))?;

    let res = loop {
        if let Err(e) = term.draw(|f| render(f, &title, &state.lock().unwrap())) {
            break Err(e);
        }
        match event::poll(Duration::from_millis(120)) {
            Ok(true) => {
                if let Ok(event::Event::Key(k)) = event::read() {
                    if k.kind == event::KeyEventKind::Press {
                        match k.code {
                            event::KeyCode::Char('q') | event::KeyCode::Esc => break Ok(()),
                            _ => {}
                        }
                    }
                }
            }
            Ok(false) => {}
            Err(e) => break Err(e),
        }
    };

    terminal::disable_raw_mode()?;
    execute!(term.backend_mut(), terminal::LeaveAlternateScreen, crossterm::cursor::Show)?;
    res
}

fn render(f: &mut Frame, title: &str, s: &State) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(5), Constraint::Length(4)])
        .split(f.area());

    let head = Line::from(vec![
        Span::styled(" swarm ", Style::new().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(format!("  {title}")),
    ]);
    f.render_widget(Paragraph::new(head), rows[0]);

    let mid = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).split(rows[1]);
    f.render_widget(pieces(s), mid[0]);
    f.render_widget(peers(s), mid[1]);

    let foot = Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(2)])
        .split(rows[2]);
    let ratio = if s.total_bytes > 0 {
        (s.done_bytes as f64 / s.total_bytes as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    f.render_widget(
        Gauge::default()
            .gauge_style(Style::new().fg(Color::Green))
            .ratio(ratio)
            .label(format!("{:.1}%", ratio * 100.0)),
        foot[0],
    );
    let done = if s.finished { " · done" } else { "" };
    let stat = format!(
        " {}/{} pieces · {:.0} KiB/s · {} peers{done}",
        s.pieces_done(),
        s.have.len(),
        s.rate / 1024.0,
        s.peers_total
    );
    f.render_widget(Paragraph::new(Line::styled(stat, Style::new().fg(Color::Gray))), foot[1]);
    let last = s.log.last().cloned().unwrap_or_default();
    f.render_widget(
        Paragraph::new(Line::styled(format!(" {last}"), Style::new().fg(Color::DarkGray))),
        foot[2],
    );
}

fn pieces(s: &State) -> Paragraph<'static> {
    let mut spans = Vec::new();
    for (i, &h) in s.have.iter().enumerate() {
        let (ch, color) = if Some(i) == s.active_piece {
            ("▣", Color::Yellow)
        } else if h {
            ("■", Color::Green)
        } else {
            ("·", Color::DarkGray)
        };
        spans.push(Span::styled(ch, Style::new().fg(color)));
        spans.push(Span::raw(" "));
    }
    let title = format!(" pieces {}/{} ", s.pieces_done(), s.have.len());
    Paragraph::new(Line::from(spans))
        .wrap(Wrap { trim: false })
        .block(Block::bordered().title(title).border_style(Style::new().fg(Color::DarkGray)))
}

fn peers(s: &State) -> Paragraph<'static> {
    let mut lines = Vec::new();
    for p in &s.peers {
        let filled = ((p.have_frac * 10.0).round() as usize).min(10);
        let bar = "█".repeat(filled) + &"░".repeat(10 - filled);
        let style = if p.active {
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray)
        };
        let mark = if p.active { "▶" } else { " " };
        lines.push(Line::styled(
            format!("{mark} {:<17} {bar} {:3.0}%", p.label, p.have_frac * 100.0),
            style,
        ));
    }
    let title = format!(" peers ({}) ", s.peers.len());
    Paragraph::new(lines).block(Block::bordered().title(title).border_style(Style::new().fg(Color::DarkGray)))
}
