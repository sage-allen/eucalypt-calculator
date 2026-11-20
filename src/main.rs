mod calculator;

use std::collections::VecDeque;
use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

use calculator::{Calculator, format_number};
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Row, Table, Wrap};
use ratatui::{DefaultTerminal, Frame};

const HISTORY_LIMIT: usize = 200;
const INPUT_LIMIT: usize = 1024;
const WIND_STEP: Duration = Duration::from_millis(220);
/// Rows of glyphs over their paint keys (see `paint`). Lowercase keys are leaves, which sway.
type Art = [(&'static str, &'static str)];
#[rustfmt::skip]
const EUCALYPTUS: &Art = &[
    (r"                    .,;%&;,.  .,;%;,.",
     r"                    rreeeeee  eeaeerr"),
    (r"                 ,;%&@%*&%)&%;:;&%(%&);,",
     r"                 reebbafabeaeeaabbaaeeer"),
    (r"    .,;%&;,.    ;&%(*|%&@%\|)%&%/|%&\*%&%;",
     r"    rreeeeee    eaaafggbbccbaabbbcggdfaaer"),
    (r"  ,;%&(*&)%&;,  '%/|(\|%/|)\|(/|%&/|)\|(%'",
     r"  reaaafaaaaaa  acgbbcbbbcgggdccgccccgccad"),
    (r" ;&%/|%\|%(|\%;  '/|( '|\ |/ ')|\ |/ \|)'",
     r" raabgcgbcgcaaa  bccc bcc cg cccg gc bccc"),
    (r" '/|( \|/ |)\|'    |'  \|        /  '|  |'",
     r" bccc gcb cgcbd    cd  bT        T  ec  be"),
    (r"   |'  \|  '|  \\       |      // .,;%&;,.",
     r"   cd  gd  bd  WS       T      WS eeaceerr"),
    (r"                 \\    ||    // ,;%&(%*)%&;,",
     r"                 WS    WS    WS eebabafeager"),
    (r"                   \\  ||  //--;&%/*%\|%(|\%;",
     r"                   WS  WS  WTTSeaabfcbbcgbaaa"),
    (r"   .,;%;,.           \\||//    '/|( \|/ |)\|'",
     r"   rreeeea           WTTTTS    bcgb cgc ccgcd"),
    (r"  ;%&*/|)%;-._        ||:|       |'  \|  '|",
     r"  eaafaaaaaWTS        WTPS       bc  bc  bd"),
    (r"  '/| \|)\'   '-._    |!||",
     r"  bdc gdccc   WTTS    WSTS"),
    (r"    |' \|         '-._||;|",
     r"    ge bd         WTTTTTPS"),
    (r"                     |:||",
     r"                     WPTS"),
    (r"                     ||!|",
     r"                     WTSS"),
    (r"                    /|;||\",
     r"                    WTPTTS"),
    (r" .,_.-~'^`-.,_.,-~'/_||||_\^`-.,_.-~'^`-.,_.,.",
     r" HHEHEHHHHEHHEHHEHHWTTTTTTSHHEHHEHEHHHHEHHEHHH"),
];
#[rustfmt::skip]
const SAPLING: &Art = &[
    (r"       .,;%&;,. .,;%;,.",
     r"       rreeeeee eaeeerr"),
    (r"     ,;%&(%*)&%;&%(%&);,",
     r"     reaaaafaaaeaaaabbar"),
    (r"    ;&%/|%\|)%&/|%&\|%&;",
     r"    eaabggcbdcgacccccggb"),
    (r"    '/|( \|/ ')|\ |/ \|'",
     r"    bgcc ccg gccb gc cbd"),
    (r"      |' \\  |   //   '|",
     r"      bd WS  T   WS   ad"),
    (r"          \\ |  //",
     r"          WS T  WS"),
    (r"           \\||//",
     r"           WTTTTS"),
    (r"            |;||",
     r"            WPTS"),
    (r" .,_.-~'^`-/||||\-.,_.-~'^`",
     r" HHEHEHHHHEWTTTTSEHHEHEHHHH"),
];
#[rustfmt::skip]
const SPRIG: &Art = &[
    (r",;%&(*)%&;,_.--'",
     r"reabbfbbaerWTTTS"),
    (r"'/|( \|/ |)'",
     r"cgdb cgc cca"),
];

#[derive(Default)]
struct App {
    calculator: Calculator,
    history: VecDeque<(String, String)>,
    input: String,
    cursor: usize,
    recalled: Option<usize>,
    draft: String,
    error: Option<String>,
    show_help: bool,
    // Calculations scrolled back from the newest, and how many fit in the log.
    scroll: usize,
    page: usize,
}

impl App {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let started = Instant::now();
        loop {
            let elapsed = started.elapsed();
            let tick = (elapsed.as_millis() / WIND_STEP.as_millis()) as usize;
            terminal.draw(|frame| self.draw(frame, tick))?;

            if self.history.is_empty() && !self.show_help && !event::poll(WIND_STEP)? {
                continue;
            }
            if self.handle_event(event::read()?) {
                return Ok(());
            }
        }
    }

    fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => return self.handle_key(key),
            Event::Paste(text) if !self.show_help => self.insert(&text),
            _ => {}
        }
        false
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Windows reports AltGr as Ctrl+Alt, and AltGr types characters such as ^ on some layouts.
        let altgr = cfg!(windows)
            && key
                .modifiers
                .contains(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if key.modifiers.contains(KeyModifiers::CONTROL) && !altgr {
            return matches!(key.code, KeyCode::Char('c' | 'C' | 'q' | 'Q'));
        }
        let plain_or_shift = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
        match key.code {
            KeyCode::Char('?') if plain_or_shift => {
                if key.kind == KeyEventKind::Press {
                    self.show_help = !self.show_help;
                }
            }
            KeyCode::Esc if self.show_help => self.show_help = false,
            _ if self.show_help => {}
            KeyCode::Tab => {
                if key.kind == KeyEventKind::Press {
                    self.calculator.degrees = !self.calculator.degrees;
                    self.error = None;
                }
            }
            KeyCode::Char(ch) if altgr || !key.modifiers.contains(KeyModifiers::ALT) => {
                self.insert(ch.encode_utf8(&mut [0; 4]));
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.input.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.len(),
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.input.remove(self.cursor);
                self.edited();
            }
            KeyCode::Delete if self.cursor < self.input.len() => {
                self.input.remove(self.cursor);
                self.edited();
            }
            KeyCode::Esc if key.kind == KeyEventKind::Press => self.clear(),
            KeyCode::Enter => self.submit(),
            KeyCode::Up => self.recall(true),
            KeyCode::Down => self.recall(false),
            // Drawing the log clamps the scroll to its length.
            KeyCode::PageUp => self.scroll += self.page,
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(self.page),
            _ => {}
        }
        false
    }

    fn insert(&mut self, text: &str) {
        let mut batch = String::new();
        for ch in text.chars() {
            match ch {
                '\u{03c0}' => batch.push_str("pi"),
                '\u{00d7}' => batch.push('*'),
                '\u{00f7}' => batch.push('/'),
                '\u{2212}' => batch.push('-'),
                '\n' | '\r' | '\t' => batch.push(' '),
                ' '..='~' => batch.push(ch),
                _ => {
                    self.error = Some("Unsupported character in expression".into());
                    return;
                }
            }
            if batch.len() + self.input.len() > INPUT_LIMIT {
                self.error = Some(format!("Expression limit: {INPUT_LIMIT} characters"));
                return;
            }
        }
        self.input.insert_str(self.cursor, &batch);
        self.cursor += batch.len();
        self.edited();
    }

    fn edited(&mut self) {
        self.recalled = None;
        self.draft.clear();
        self.error = None;
    }

    fn clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.edited();
    }

    fn submit(&mut self) {
        let expression = self.input.trim();
        if expression.is_empty() {
            return;
        }
        match self.calculator.submit(expression) {
            Ok(value) => {
                if self.history.len() == HISTORY_LIMIT {
                    self.history.pop_front();
                }
                self.history
                    .push_back((expression.to_owned(), format_number(value)));
                self.scroll = 0;
                self.clear();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn recall(&mut self, up: bool) {
        let len = self.history.len();
        let current = self.recalled.unwrap_or(len);
        let next = if up {
            current.saturating_sub(1)
        } else {
            (current + 1).min(len)
        };
        if current == next {
            return;
        }
        if current == len {
            self.draft = std::mem::take(&mut self.input);
        }
        self.recalled = (next < len).then_some(next);
        self.input = match self.history.get(next) {
            Some((expression, _)) => expression.clone(),
            None => std::mem::take(&mut self.draft),
        };
        self.cursor = self.input.len();
        self.error = None;
    }

    fn draw(&mut self, frame: &mut Frame<'_>, tick: usize) {
        let area = frame.area();
        if area.width < 24 || area.height < 10 {
            frame.render_widget("Resize or Ctrl+C quit", area);
            return;
        }
        let [workspace, editor, footer] = Layout::vertical([
            Constraint::Min(4),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .areas(area);

        if area.width >= 72 {
            let [log, variables] =
                Layout::horizontal([Constraint::Min(44), Constraint::Length(28)]).areas(workspace);
            self.draw_log(frame, log, tick);
            let block = panel("Variables", false);
            let inner = block.inner(variables);
            frame.render_widget(block, variables);
            let lines: Vec<_> = self
                .calculator
                .values()
                .take(usize::from(inner.height))
                .map(|(name, value)| {
                    Line::from(vec![
                        Span::raw(name).light_green(),
                        Span::raw(format!(" = {}", format_number(value))),
                    ])
                })
                .collect();
            if lines.is_empty() {
                frame.render_widget(Line::from("Try x = 54").dim(), inner);
            } else {
                frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
            }
        } else {
            self.draw_log(frame, workspace, tick);
        }

        self.draw_editor(frame, editor);
        let hints = [
            ("PgUp/PgDn", "scroll"),
            ("Enter", "evaluate"),
            ("Up/Down", "history"),
            ("Esc", "clear"),
            ("Tab", "angle"),
            ("?", "help"),
            ("Ctrl+C", "quit"),
        ];
        // Narrow terminals drop hints from the left until the rest fit.
        let keys = (0..hints.len())
            .map(|first| {
                Line::from_iter(hints[first..].iter().flat_map(|(key, action)| {
                    [
                        Span::raw(format!(" {key} ")).light_cyan().bold(),
                        Span::raw(format!("{action} ")).light_blue(),
                    ]
                }))
            })
            // The trailing space may run off the edge.
            .find(|line| line.width() <= usize::from(area.width) + 1);
        frame.render_widget(keys.unwrap_or_default(), footer);
        if self.show_help {
            draw_help(frame);
        }
    }

    fn draw_log(&mut self, frame: &mut Frame<'_>, area: Rect, tick: usize) {
        // Inside the border, each calculation takes two rows.
        let len = self.history.len();
        self.page = usize::from(area.height.saturating_sub(2) / 2);
        self.scroll = self.scroll.min(len.saturating_sub(self.page));
        let first = len.saturating_sub(self.page + self.scroll);
        let title = if len > self.page {
            let last = first + self.page;
            format!("Calculations {}-{last} of {len}", first + 1)
        } else {
            "Calculations".into()
        };
        let block = panel(&title, false);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if self.history.is_empty() {
            let size = |art: &Art| {
                let width = art
                    .iter()
                    .map(|(glyphs, _)| glyphs.len())
                    .max()
                    .unwrap_or(0);
                (width as u16, art.len() as u16)
            };
            let art = [EUCALYPTUS, SAPLING]
                .into_iter()
                .find(|art| {
                    let (width, height) = size(art);
                    width <= inner.width && height <= inner.height
                })
                .unwrap_or(SPRIG);
            let (width, height) = size(art);
            let tree = inner.centered(Constraint::Length(width), Constraint::Length(height));
            let lines = art.iter().enumerate().map(|(y, (glyphs, keys))| {
                let cells = glyphs.chars().zip(keys.chars()).enumerate();
                Line::from_iter(cells.map(|(x, (glyph, key))| {
                    let glyph = if key.is_ascii_lowercase() {
                        sway(glyph, tick + x / 4 + y / 2)
                    } else {
                        glyph
                    };
                    Span::styled(glyph.to_string(), paint(key))
                }))
            });
            frame.render_widget(Paragraph::new(lines.collect::<Vec<_>>()), tree);
            return;
        }
        let lines = self
            .history
            .iter()
            .skip(first)
            .take(self.page)
            .flat_map(|(expression, result)| {
                [
                    Line::from(expression.as_str()).dim(),
                    Line::from(format!("= {result}")).light_green(),
                ]
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_editor(&self, frame: &mut Frame<'_>, area: Rect) {
        let title = if self.calculator.degrees {
            "Expression (degrees)"
        } else {
            "Expression (radians)"
        };
        let block = panel(title, true);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        // The editor stores only ASCII, so byte offsets are also terminal columns.
        let offset = (self.cursor + 1).saturating_sub(usize::from(inner.width) - 2);
        let expression = Line::from(vec![
            Span::raw("> ").light_green(),
            Span::raw(&self.input[offset..]),
        ]);
        let preview = if let Some(error) = &self.error {
            Line::from(format!("! {error}")).light_red()
        } else if self.input.trim().is_empty() {
            Line::from("  + - * / % ^   sqrt(81)   x = 54").dim()
        } else if let Ok(value) = self.calculator.preview(&self.input) {
            Line::from(format!("= {}", format_number(value))).light_green()
        } else {
            Line::default()
        };
        frame.render_widget(Paragraph::new(vec![expression, preview]), inner);
        if !self.show_help {
            frame.set_cursor_position((inner.x + 2 + (self.cursor - offset) as u16, inner.y));
        }
    }
}

fn paint(key: char) -> Color {
    Color::Indexed(match key {
        // Leaves from sunlit to shaded, then blue-grey, new growth and blossom.
        'e' => 150,
        'a' => 114,
        'b' => 71,
        'c' => 65,
        'd' => 29,
        'g' => 109,
        'r' => 173,
        'f' => 204,
        // Bark from sunlit to shaded, then a peeling patch.
        'W' => 187,
        'T' => 180,
        'S' => 137,
        'P' => 174,
        // Red earth and dry grass.
        'E' => 166,
        _ => 143,
    })
}

/// A gust leans a leaf one step along `/(|)\`, then it swings back the other way.
fn sway(leaf: char, phase: usize) -> char {
    const LEAN: &str = r"/(|)\";
    const GUST: [isize; 6] = [0, -1, -1, 0, 1, 0];
    match LEAN.find(leaf) {
        Some(lean) => {
            let gust = GUST[phase % GUST.len()];
            LEAN.as_bytes()[lean.saturating_add_signed(gust).min(LEAN.len() - 1)] as char
        }
        None => leaf,
    }
}

fn draw_help(frame: &mut Frame<'_>) {
    let bindings = [
        ("Enter", "Evaluate"),
        ("Up/Down", "History"),
        ("PgUp/PgDn", "Scroll log"),
        ("Left/Right", "Move cursor"),
        ("Home/End", "Start/end"),
        ("Esc", "Clear input"),
        ("Tab", "Deg / rad"),
        ("Ctrl+C", "Quit"),
    ];
    let popup = frame.area().centered(
        Constraint::Max(28),
        Constraint::Max(bindings.len() as u16 + 2),
    );
    let rows =
        bindings.map(|(key, action)| Row::new([Span::raw(key).light_green(), Span::raw(action)]));
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Table::new(rows, [Constraint::Length(10), Constraint::Fill(1)])
            .block(panel("Help - ? or Esc closes", true)),
        popup,
    );
}

fn panel(title: &str, active: bool) -> Block<'static> {
    let (border, text) = if active {
        let green = Style::new().green().bold();
        (green, green)
    } else {
        (Style::new().dark_gray(), Style::new().gray())
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Line::styled(format!(" {title} "), text))
}

fn main() -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other("eucalypt needs an interactive terminal"));
    }
    let result = ratatui::try_init().and_then(|mut terminal| {
        // Older Windows consoles may not support bracketed paste.
        let bracketed_paste = execute!(terminal.backend_mut(), EnableBracketedPaste).is_ok();
        let result = App::default().run(&mut terminal);
        if bracketed_paste {
            result.and(execute!(terminal.backend_mut(), DisableBracketedPaste))
        } else {
            result
        }
    });
    result.and(ratatui::try_restore())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn recalls_history_without_losing_the_draft() {
        let mut app = App::default();
        app.insert("x = 54");
        app.submit();
        app.insert("x \u{00d7} 2");
        app.recall(true);
        assert_eq!(app.input, "x = 54");
        app.recall(false);
        assert_eq!(app.input, "x * 2");
        app.handle_key(KeyEvent::from(KeyCode::Left));
        app.handle_key(KeyEvent::from(KeyCode::Delete));
        app.insert("3");
        app.submit();
        assert_eq!(app.history.back().unwrap().1, "162");
        assert!(app.input.is_empty());
    }

    #[test]
    fn tab_toggles_the_angle_unit_and_leaves_every_letter_typeable() {
        let mut app = App::default();
        app.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
        app.insert(" = acos(0)");
        app.handle_key(KeyCode::Tab.into());
        assert_eq!(app.calculator.preview(&app.input), Ok(90.0));
        app.submit();
        assert_eq!(app.history.back().unwrap().1, "90");
        app.handle_key(KeyCode::Tab.into());
        app.recall(true);
        assert_eq!(
            app.calculator.preview(&app.input),
            Ok(std::f64::consts::FRAC_PI_2)
        );
        assert_eq!(app.calculator.preview("A"), Ok(90.0));
    }

    #[test]
    fn page_keys_scroll_the_log_and_a_new_result_returns_to_the_end() {
        let mut app = App::default();
        let mut terminal = Terminal::new(TestBackend::new(30, 12)).unwrap();
        let mut screen = |app: &mut App| {
            terminal.draw(|frame| app.draw(frame, 0)).unwrap();
            terminal.backend().to_string()
        };
        for n in 1..=5 {
            app.insert(&n.to_string());
            app.submit();
        }
        assert!(screen(&mut app).contains("4-5 of 5"));
        for _ in 0..3 {
            app.handle_key(KeyCode::PageUp.into());
            screen(&mut app);
        }
        assert!(screen(&mut app).contains("1-2 of 5"));
        app.handle_key(KeyCode::PageDown.into());
        assert!(screen(&mut app).contains("3-4 of 5"));
        app.insert("6");
        app.submit();
        assert!(screen(&mut app).contains("5-6 of 6"));
    }

    #[test]
    fn help_captures_keys_and_paste_but_allows_quit() {
        let mut app = App::default();
        app.insert("1+2");
        app.handle_key(KeyCode::Left.into());
        let quit = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        for close in [KeyCode::Char('?'), KeyCode::Esc] {
            app.handle_key(KeyCode::Char('?').into());
            app.handle_key(KeyCode::Enter.into());
            app.handle_key(KeyCode::Tab.into());
            app.handle_event(Event::Paste("8".into()));
            assert!(app.show_help && app.handle_key(quit));
            app.handle_key(close.into());
            assert!(!app.show_help && !app.calculator.degrees);
            assert!(app.history.is_empty());
            assert_eq!((&*app.input, app.cursor), ("1+2", 2));
        }
        assert!(app.handle_key(quit));
    }

    #[test]
    fn altgr_types_a_character_where_windows_reports_it_as_ctrl_alt() {
        let mut app = App::default();
        let altgr = KeyModifiers::CONTROL | KeyModifiers::ALT;
        assert!(!app.handle_key(KeyEvent::new(KeyCode::Char('^'), altgr)));
        assert_eq!(app.input, if cfg!(windows) { "^" } else { "" });
    }

    #[test]
    fn tree_art_is_fully_painted_and_only_leaves_sway() {
        for (glyphs, keys) in [EUCALYPTUS, SAPLING, SPRIG].into_iter().flatten() {
            assert_eq!(glyphs.len(), keys.len(), "{glyphs}");
        }
        let frames = |leaf| (0..6).map(|phase| sway(leaf, phase)).collect::<String>();
        assert_eq!(frames('|'), "|((|)|");
        assert_eq!(frames('/'), "////(/");
        assert_eq!(frames('%'), "%%%%%%");
    }
}
