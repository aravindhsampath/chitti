use crate::bridges::CommBridge;
use crate::conductor::events::{SessionState, SystemEvent, UserEvent};
use anyhow::Result;
use async_trait::async_trait;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct TuiBridge {
    tx: mpsc::Sender<UserEvent>,
    shared_state: Arc<Mutex<TuiUiState>>,
}

struct TuiUiState {
    messages: Vec<ChatMessage>,
    input: String,
    session_state: Option<SessionState>,
    list_state: ListState,
    should_exit: bool,
    last_ctrl_c: Option<Instant>,
    is_thinking: bool,
    auto_scroll: bool,
}

#[derive(Clone)]
enum ChatMessage {
    User(String),
    Model(String),
    Thought(String),
    System(String),
    Error(String),
    Tool(String),
    Debug(String),
}

impl TuiBridge {
    pub fn new() -> (Self, mpsc::Receiver<UserEvent>) {
        let (tx, rx) = mpsc::channel(100);
        let shared_state = Arc::new(Mutex::new(TuiUiState {
            messages: Vec::new(),
            input: String::new(),
            session_state: None,
            list_state: ListState::default(),
            should_exit: false,
            last_ctrl_c: None,
            is_thinking: false,
            auto_scroll: true,
        }));
        (Self { tx, shared_state }, rx)
    }

    pub async fn run_ui_loop(&self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let res = self.main_loop(&mut terminal).await;

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        res
    }

    async fn main_loop<B: Backend>(&self, terminal: &mut Terminal<B>) -> Result<()>
    where
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        loop {
            {
                let mut state = self.shared_state.lock().unwrap();
                if state.should_exit {
                    return Ok(());
                }
                terminal.draw(|f| self.render(f, &mut state))?;
            }

            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        let mut event_to_send = None;
                        {
                            let mut state = self.shared_state.lock().unwrap();
                            match key.code {
                                KeyCode::Char('c')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    state.input.clear();
                                    if let Some(last) = state.last_ctrl_c {
                                        if last.elapsed() < Duration::from_millis(500) {
                                            state.should_exit = true;
                                            event_to_send =
                                                Some(UserEvent::Input("/exit".to_string()));
                                        }
                                    }
                                    state.last_ctrl_c = Some(Instant::now());
                                }
                                KeyCode::Enter => {
                                    let input = state.input.drain(..).collect::<String>();
                                    if !input.is_empty() {
                                        state.messages.push(ChatMessage::User(input.clone()));
                                        state.is_thinking = true;
                                        state.auto_scroll = true;
                                        // Reset scroll state
                                        state.list_state = ListState::default();
                                        event_to_send = Some(UserEvent::Input(input));
                                    }
                                }
                                KeyCode::Char(c) => {
                                    state.input.push(c);
                                }
                                KeyCode::Backspace => {
                                    state.input.pop();
                                }
                                KeyCode::Esc => {
                                    state.should_exit = true;
                                    event_to_send = Some(UserEvent::Input("/exit".to_string()));
                                }
                                KeyCode::Up => {
                                    state.auto_scroll = false;
                                    let i = match state.list_state.selected() {
                                        Some(i) => i.saturating_sub(1),
                                        None => 0,
                                    };
                                    state.list_state.select(Some(i));
                                }
                                KeyCode::Down => {
                                    state.auto_scroll = false;
                                    let i = match state.list_state.selected() {
                                        Some(i) => i + 1,
                                        None => 0,
                                    };
                                    state.list_state.select(Some(i));
                                }
                                _ => {}
                            }
                        }
                        if let Some(evt) = event_to_send {
                            self.tx.send(evt).await?;
                        }
                    }
                }
            }
        }
    }

    fn render(&self, f: &mut ratatui::Frame, state: &mut TuiUiState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(0)
            .constraints([
                Constraint::Length(1), // Status Bar
                Constraint::Fill(1),   // Chat Area (Flexible)
                Constraint::Length(3), // Input Box
            ])
            .split(f.area());

        // 1. Status Bar
        let status_bar = if let Some(ref s) = state.session_state {
            format!(
                " Model: {} | Thinking: {} | Stream: {} | Memory: {} | PWD: {} | Branch: {} | DEV: {} ",
                s.model,
                s.thinking_level,
                if s.streaming { "ON" } else { "OFF" },
                if s.memory_enabled { "ON" } else { "OFF" },
                s.pwd,
                s.git_branch,
                if s.dev_mode { "ON" } else { "OFF" }
            )
        } else {
            " Initializing Chitti... ".to_string()
        };

        let status_widget = Paragraph::new(status_bar).style(
            Style::default()
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
        f.render_widget(status_widget, chunks[0]);

        // 2. Chat Area (Line-buffered List)
        let chat_width = chunks[1].width.saturating_sub(4); // Borders + padding
        let mut display_lines = Vec::new();

        for m in &state.messages {
            match m {
                ChatMessage::User(t) => {
                    display_lines.push(Line::from(vec![
                        Span::styled(
                            "User: ",
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(t.clone()),
                    ]));
                }
                ChatMessage::Model(t) => {
                    Self::append_wrapped_lines(
                        &mut display_lines,
                        "Chitti: ",
                        t,
                        Style::default().fg(Color::Green),
                        chat_width,
                    );
                }
                ChatMessage::Thought(t) => {
                    Self::append_wrapped_lines(
                        &mut display_lines,
                        "Thought: ",
                        t,
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                        chat_width,
                    );
                }
                ChatMessage::System(t) => {
                    display_lines.push(Line::from(vec![
                        Span::styled("System: ", Style::default().fg(Color::Yellow)),
                        Span::raw(t.clone()),
                    ]));
                }
                ChatMessage::Error(t) => {
                    display_lines.push(Line::from(vec![
                        Span::styled(
                            "Error: ",
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(t.clone()),
                    ]));
                }
                ChatMessage::Tool(t) => {
                    display_lines.push(Line::from(vec![
                        Span::styled(
                            "Tool: ",
                            Style::default().fg(Color::Blue).add_modifier(Modifier::DIM),
                        ),
                        Span::raw(t.clone()),
                    ]));
                }
                ChatMessage::Debug(t) => {
                    display_lines.push(Line::from(vec![
                        Span::styled(
                            "DEBUG: ",
                            Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::DIM),
                        ),
                        Span::raw(t.clone()),
                    ]));
                }
            }
            display_lines.push(Line::raw("")); // Spacer
        }

        if state.is_thinking {
            display_lines.push(Line::from(vec![Span::styled(
                "Chitti is thinking...",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::ITALIC),
            )]));
        }

        let total_lines = display_lines.len();
        let list_items: Vec<ListItem> = display_lines.into_iter().map(ListItem::new).collect();

        let list = List::new(list_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Conversation "),
            )
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");

        // Auto-scroll logic: if state.auto_scroll is true, force select the last line
        if state.auto_scroll && total_lines > 0 {
            state.list_state.select(Some(total_lines.saturating_sub(1)));
        }

        f.render_stateful_widget(list, chunks[1], &mut state.list_state);

        // 3. Input Box
        let input_widget = Paragraph::new(state.input.as_str())
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Input (Esc to exit, Arrow keys to scroll) "),
            );
        f.render_widget(input_widget, chunks[2]);

        f.set_cursor_position((chunks[2].x + state.input.len() as u16 + 1, chunks[2].y + 1));
    }

    fn append_wrapped_lines(
        lines: &mut Vec<Line>,
        prefix: &str,
        content: &str,
        style: Style,
        width: u16,
    ) {
        let prefix_len = prefix.len();
        // Defensive check to prevent textwrap panic on narrow terminals
        let wrap_width = if width > prefix_len as u16 + 2 {
            width as usize - prefix_len
        } else {
            width.max(1) as usize
        };

        let mut is_first_line_of_message = true;

        if content.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                prefix.to_string(),
                style.add_modifier(Modifier::BOLD),
            )]));
            return;
        }

        for raw_line in content.lines() {
            if raw_line.is_empty() {
                lines.push(Line::raw(""));
                continue;
            }

            let wrapped = textwrap::fill(raw_line, wrap_width);
            for part in wrapped.lines() {
                if is_first_line_of_message {
                    lines.push(Line::from(vec![
                        Span::styled(prefix.to_string(), style.add_modifier(Modifier::BOLD)),
                        Span::styled(part.to_string(), style),
                    ]));
                    is_first_line_of_message = false;
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(prefix_len)),
                        Span::styled(part.to_string(), style),
                    ]));
                }
            }
        }
    }
}

#[async_trait]
impl CommBridge for TuiBridge {
    async fn send(&self, event: SystemEvent) -> Result<()> {
        let mut state = self.shared_state.lock().unwrap();

        match &event {
            SystemEvent::Text(_, s)
            | SystemEvent::Thought(_, s)
            | SystemEvent::Info(_, s)
            | SystemEvent::ToolCall { state: s, .. }
            | SystemEvent::Error(_, s)
            | SystemEvent::Debug(_, s)
            | SystemEvent::RequestApproval { state: s, .. }
            | SystemEvent::Ready(s) => state.session_state = Some(s.clone()),
        }

        match event {
            SystemEvent::Text(text, _) => {
                state.is_thinking = false;
                let should_append = matches!(state.messages.last(), Some(ChatMessage::Model(_)));
                if should_append {
                    if let Some(ChatMessage::Model(ref mut last_text)) = state.messages.last_mut() {
                        last_text.push_str(&text);
                    }
                } else {
                    state.messages.push(ChatMessage::Model(text));
                }
            }
            SystemEvent::Thought(text, _) => {
                state.is_thinking = false;
                let should_append = matches!(state.messages.last(), Some(ChatMessage::Thought(_)));
                if should_append {
                    if let Some(ChatMessage::Thought(ref mut last_text)) = state.messages.last_mut()
                    {
                        last_text.push_str(&text);
                    }
                } else {
                    state.messages.push(ChatMessage::Thought(text));
                }
            }
            SystemEvent::ToolCall { name, args, .. } => {
                state.is_thinking = false;
                state
                    .messages
                    .push(ChatMessage::Tool(format!("Calling {name} with {args}")));
            }
            SystemEvent::Error(err, _) => {
                state.is_thinking = false;
                state.messages.push(ChatMessage::Error(err));
            }
            SystemEvent::Debug(text, _) => {
                state.messages.push(ChatMessage::Debug(text));
            }
            SystemEvent::Info(text, _) => {
                if text == "Context cleared." {
                    state.messages.clear();
                    state.list_state = ListState::default();
                }
                state.messages.push(ChatMessage::System(text));
            }
            SystemEvent::RequestApproval { description, .. } => {
                state.is_thinking = false;
                state.messages.push(ChatMessage::System(format!(
                    "APPROVAL REQUIRED: {description}"
                )));
                state.messages.push(ChatMessage::System(
                    "Type 'y' to approve, 'n' to reject, or any instruction to steer.".to_string(),
                ));
            }
            SystemEvent::Ready(_) => {
                state.is_thinking = false;
            }
        }

        Ok(())
    }
}
