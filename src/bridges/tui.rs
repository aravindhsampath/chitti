use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use anyhow::Result;
use crate::bridges::CommBridge;
use crate::conductor::events::{UserEvent, SystemEvent, SessionState};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;

pub struct TuiBridge {
    tx: mpsc::Sender<UserEvent>,
    shared_state: Arc<Mutex<TuiUiState>>,
}

struct TuiUiState {
    messages: Vec<ChatMessage>,
    input: String,
    session_state: Option<SessionState>,
    scroll_state: ListState,
    should_exit: bool,
}

#[derive(Clone)]
enum ChatMessage {
    User(String),
    Model(String),
    Thought(String),
    System(String),
    Error(String),
    Tool(String),
}

impl TuiBridge {
    pub fn new() -> (Self, mpsc::Receiver<UserEvent>) {
        let (tx, rx) = mpsc::channel(100);
        let shared_state = Arc::new(Mutex::new(TuiUiState {
            messages: Vec::new(),
            input: String::new(),
            session_state: None,
            scroll_state: ListState::default(),
            should_exit: false,
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
        B::Error: std::error::Error + Send + Sync + 'static 
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
                        let mut state = self.shared_state.lock().unwrap();
                        match key.code {
                            KeyCode::Enter => {
                                let input = state.input.drain(..).collect::<String>();
                                if !input.is_empty() {
                                    state.messages.push(ChatMessage::User(input.clone()));
                                    self.tx.send(UserEvent::Input(input)).await?;
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
                                self.tx.send(UserEvent::Input("/exit".to_string())).await?;
                            }
                            KeyCode::Up => {
                                let i = match state.scroll_state.selected() {
                                    Some(i) => if i == 0 { 0 } else { i - 1 },
                                    None => 0,
                                };
                                state.scroll_state.select(Some(i));
                            }
                            KeyCode::Down => {
                                let i = match state.scroll_state.selected() {
                                    Some(i) => i + 1,
                                    None => 0,
                                };
                                state.scroll_state.select(Some(i));
                            }
                            _ => {}
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
                Constraint::Min(3),    // Chat Area
                Constraint::Length(3), // Input Box
            ])
            .split(f.area());

        // 1. Status Bar
        let status_bar = if let Some(ref s) = state.session_state {
            format!(
                " Model: {} | Thinking: {} | Stream: {} | Memory: {} | PWD: {} | Branch: {} ",
                s.model,
                s.thinking_level,
                if s.streaming { "ON" } else { "OFF" },
                if s.memory_enabled { "ON" } else { "OFF" },
                s.pwd,
                s.git_branch
            )
        } else {
            " Initializing Chitti... ".to_string()
        };

        let status_widget = Paragraph::new(status_bar)
            .style(Style::default().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD));
        f.render_widget(status_widget, chunks[0]);

        // 2. Chat Area
        let messages: Vec<ListItem> = state.messages.iter().map(|m| {
            let (content, style) = match m {
                ChatMessage::User(t) => (format!("User: {}", t), Style::default().fg(Color::Cyan)),
                ChatMessage::Model(t) => (format!("Chitti: {}", t), Style::default().fg(Color::Green)),
                ChatMessage::Thought(t) => (format!("Thought: {}", t), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
                ChatMessage::System(t) => (format!("System: {}", t), Style::default().fg(Color::Yellow)),
                ChatMessage::Error(t) => (format!("Error: {}", t), Style::default().fg(Color::Red)),
                ChatMessage::Tool(t) => (format!("Tool: {}", t), Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC)),
            };
            // Use Paragraph inside ListItem is fine, but we need to return it as something that implements Into<Text>
            ListItem::new(content).style(style)
        }).collect();

        let list = List::new(messages)
            .block(Block::default().borders(Borders::ALL).title(" Conversation "))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");
        
        // Auto-scroll logic: if not manually scrolling, keep at bottom
        if state.scroll_state.selected().is_none() && !state.messages.is_empty() {
            let last_idx = state.messages.len().saturating_sub(1);
            state.scroll_state.select(Some(last_idx));
        }
        
        f.render_stateful_widget(list, chunks[1], &mut state.scroll_state);

        // 3. Input Box
        let input_widget = Paragraph::new(state.input.as_str())
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title(" Input (Esc to exit) "));
        f.render_widget(input_widget, chunks[2]);
        
        // Set cursor position for input
        f.set_cursor_position((
            chunks[2].x + state.input.len() as u16 + 1,
            chunks[2].y + 1,
        ));
    }
}

#[async_trait]
impl CommBridge for TuiBridge {
    async fn send(&self, event: SystemEvent) -> Result<()> {
        let mut state = self.shared_state.lock().unwrap();
        
        // Sync SessionState
        match &event {
            SystemEvent::Text(_, s) => state.session_state = Some(s.clone()),
            SystemEvent::Thought(_, s) => state.session_state = Some(s.clone()),
            SystemEvent::Info(_, s) => state.session_state = Some(s.clone()),
            SystemEvent::ToolCall { state: s, .. } => state.session_state = Some(s.clone()),
            SystemEvent::Error(_, s) => state.session_state = Some(s.clone()),
            SystemEvent::RequestApproval { state: s, .. } => state.session_state = Some(s.clone()),
            SystemEvent::Ready(s) => state.session_state = Some(s.clone()),
        }

        match event {
            SystemEvent::Text(text, _) => {
                let should_append = match state.messages.last() {
                    Some(ChatMessage::Model(_)) => true,
                    _ => false,
                };

                if should_append {
                    if let Some(ChatMessage::Model(ref mut last_text)) = state.messages.last_mut() {
                        last_text.push_str(&text);
                    }
                } else {
                    state.messages.push(ChatMessage::Model(text));
                }
            }
            SystemEvent::Thought(text, _) => {
                let should_append = match state.messages.last() {
                    Some(ChatMessage::Thought(_)) => true,
                    _ => false,
                };

                if should_append {
                    if let Some(ChatMessage::Thought(ref mut last_text)) = state.messages.last_mut() {
                        last_text.push_str(&text);
                    }
                } else {
                    state.messages.push(ChatMessage::Thought(text));
                }
            }
            SystemEvent::ToolCall { name, args, .. } => {
                state.messages.push(ChatMessage::Tool(format!("Calling {} with {}", name, args)));
            }
            SystemEvent::Error(err, _) => {
                state.messages.push(ChatMessage::Error(err));
            }
            SystemEvent::RequestApproval { description, .. } => {
                state.messages.push(ChatMessage::System(format!("APPROVAL REQUIRED: {}", description)));
                state.messages.push(ChatMessage::System("Type 'y' to approve, 'n' to reject, or any instruction to steer.".to_string()));
            }
            SystemEvent::Info(text, _) => {
                state.messages.push(ChatMessage::System(text));
            }
            SystemEvent::Ready(_) => {}
        }
        
        // Ensure scroll follows new content if we were at the bottom
        if !state.messages.is_empty() {
            let last_idx = state.messages.len().saturating_sub(1);
            state.scroll_state.select(Some(last_idx));
        }

        Ok(())
    }
}
