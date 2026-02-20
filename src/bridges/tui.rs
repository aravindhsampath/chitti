use async_trait::async_trait;
use std::sync::Mutex;
use tokio::sync::mpsc;
use anyhow::Result;
use std::io::{self, Write};
use crate::bridges::CommBridge;
use crate::conductor::events::{UserEvent, SystemEvent, SessionState};

pub struct TuiBridge {
    tx: mpsc::Sender<UserEvent>,
    state: Mutex<Option<SessionState>>,
}

impl TuiBridge {
    pub fn new() -> (Self, mpsc::Receiver<UserEvent>) {
        let (tx, rx) = mpsc::channel(100);
        (Self { tx, state: Mutex::new(None) }, rx)
    }

    fn print_status_bar(&self) {
        let state_lock = self.state.lock().unwrap();
        if let Some(ref state) = *state_lock {
            println!(
                "\x1b[1;30;47m Model: {} | Thinking: {} | Stream: {} | Memory: {} | PWD: {} | Branch: {} \x1b[0m",
                state.model,
                state.thinking_level,
                if state.streaming { "ON" } else { "OFF" },
                if state.memory_enabled { "ON" } else { "OFF" },
                state.pwd,
                state.git_branch
            );
        }
    }

    pub async fn run_input_loop(&self) -> Result<()> {
        let stdin = io::stdin();
        loop {
            self.print_status_bar();
            print!("chitti> ");
            io::stdout().flush()?;
            
            let mut user_input = String::new();
            stdin.read_line(&mut user_input)?;
            let prompt = user_input.trim();
            if prompt.is_empty() { continue; }

            self.tx.send(UserEvent::Input(prompt.to_string())).await?;
            
            // If it's an exit command, we stop the loop
            if prompt == "/exit" || prompt == "/quit" {
                break;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl CommBridge for TuiBridge {
    async fn send(&self, event: SystemEvent) -> Result<()> {
        let mut stdout = io::stdout();
        
        // Update local state cache from the event
        {
            let mut state_lock = self.state.lock().unwrap();
            match &event {
                SystemEvent::Text(_, state) => *state_lock = Some(state.clone()),
                SystemEvent::ToolCall { state, .. } => *state_lock = Some(state.clone()),
                SystemEvent::Error(_, state) => *state_lock = Some(state.clone()),
                SystemEvent::RequestApproval { state, .. } => *state_lock = Some(state.clone()),
            }
        }

        match event {
            SystemEvent::Text(text, _) => {
                print!("{}", text);
                stdout.flush()?;
            }
            SystemEvent::ToolCall { name, args, .. } => {
                println!("\x1b[34m\n[Chitti calling tool: {} with args: {}]\x1b[0m", name, args);
            }
            SystemEvent::Error(err, _) => {
                eprintln!("\x1b[31m\n[Error: {}]\x1b[0m", err);
            }
            SystemEvent::RequestApproval { description, .. } => {
                print!("\n\x1b[33m[Approval required: {}]\x1b[0m\nConfirm? (y/n): ", description);
                stdout.flush()?;
            }
        }
        Ok(())
    }
}
