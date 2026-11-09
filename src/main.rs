mod app;
mod command;
mod console;
mod server;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use std::{error::Error, io};

use app::{
    AddingState, App, BroadcastPhase, BroadcastState, EditingState, InputMode, ScpDirection,
    ScpState,
};
use command::{
    install_interrupt_handler, is_command_available, run_external_command, run_file_transfer,
    run_interactive_session, run_ssh_copy_id, Outcome,
};
use ui::ui;

fn main() -> Result<(), Box<dyn Error>> {
    // 在任何终端操作之前记录控制台输入模式（Windows 关键，见 console.rs）
    console::remember_original_mode();
    // 让 Ctrl+C 只中断外部会话，而不是把整个程序一起带走
    install_interrupt_handler();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;

    // 启动时若有提示（例如配置文件损坏）则直接展示
    if let Some(msg) = app.pending_message.take() {
        app.input_mode = InputMode::ShowMessage(msg);
    }

    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    // 把控制台输入模式还原成启动时的样子，退出后 shell 才有回显
    console::restore_original_mode();

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        // Hide cursor in normal mode, show in input/edit mode
        match app.input_mode {
            InputMode::Normal
            | InputMode::ConfirmDelete(_)
            | InputMode::ShowMessage(_)
            | InputMode::SelectingProfile => terminal.hide_cursor()?,
            InputMode::Adding(_) | InputMode::Editing(_) | InputMode::CreatingProfile(_) => {
                terminal.show_cursor()?
            }
            // 只有在输入路径的字段里才显示光标
            InputMode::Scp(ref s) => {
                if s.field_idx == 1 || s.field_idx == 2 {
                    terminal.show_cursor()?
                } else {
                    terminal.hide_cursor()?
                }
            }
            InputMode::BroadcastCommand(ref s) => match s.phase {
                BroadcastPhase::EnterCommand => terminal.show_cursor()?,
                BroadcastPhase::SelectServers => terminal.hide_cursor()?,
            },
        }

        terminal.draw(|f| ui(f, app))?;

        match event::read()? {
            Event::Key(key) => {
                // Windows 会同时发送 Press 和 Release 事件，只处理 Press
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if handle_key(terminal, app, key)? {
                    return Ok(());
                }
            }
            Event::Resize(_, _) => {
                // 终端尺寸变化后重新计算布局
                terminal.autoresize()?;
            }
            _ => {}
        }
    }
}

/// 处理一次按键。返回 true 表示要退出主循环（退出程序）。
fn handle_key<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    key: crossterm::event::KeyEvent,
) -> io::Result<bool> {
    match &mut app.input_mode {
        InputMode::Normal => {
            // Reset pending_g on any key that isn't 'g'
            if key.code != KeyCode::Char('g') {
                app.pending_g = false;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(true)
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(true)
                }
                KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => app.next(),
                KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => app.previous(),
                KeyCode::Char('G') => app.select_last(),
                KeyCode::Char('g') => {
                    if app.pending_g {
                        app.select_first();
                        app.pending_g = false;
                    } else {
                        app.pending_g = true;
                    }
                }
                KeyCode::Char('e') => {
                    // Enter profile selection mode
                    let current = app.current_profile.clone();
                    let idx = app.profiles.iter().position(|p| *p == current).unwrap_or(0);
                    app.profile_state.select(Some(idx));
                    app.input_mode = InputMode::SelectingProfile;
                }
                KeyCode::Char('n') => {
                    app.input_mode = InputMode::Adding(AddingState::new());
                }
                KeyCode::Char('c') => {
                    // SSH Copy ID
                    if let Some(server) = app.selected_server() {
                        let args = server.to_copy_id_args();
                        run_ssh_copy_id(terminal, &args)?;
                        app.set_last_connected(&server);
                    }
                }
                KeyCode::Char('d') => {
                    // 进入删除确认模式
                    if let Some(idx) = app.state.selected() {
                        app.input_mode = InputMode::ConfirmDelete(idx);
                    }
                }
                KeyCode::Char('i') => {
                    // Edit server
                    if let Some(idx) = app.state.selected() {
                        if let Some(server) = app.servers.get(idx).cloned() {
                            app.input_mode = InputMode::Editing(EditingState::new(&server, idx));
                        }
                    }
                }
                KeyCode::Char('m') => {
                    // Mosh
                    if let Some(server) = app.selected_server() {
                        // Check if mosh is installed
                        if !is_command_available("mosh") {
                            app.input_mode = InputMode::ShowMessage(
                                "Mosh is not installed on your system.\n\nPlease install mosh first:\n- Linux: sudo apt install mosh / sudo yum install mosh\n- macOS: brew install mosh\n- Windows: Install via package manager or from mosh.org".to_string()
                            );
                        } else if !server.jump_host.is_empty() {
                            app.input_mode = InputMode::ShowMessage(
                                "Mosh does not support jump host connections.\nPlease use SSH (Enter) instead.".to_string()
                            );
                        } else {
                            let args = server.to_mosh_args();
                            run_external_command(terminal, "mosh", &args)?;
                            app.set_last_connected(&server);
                        }
                    }
                }
                KeyCode::Char('s') => {
                    // SFTP
                    if let Some(server) = app.selected_server() {
                        let args = server.to_sftp_args();
                        let outcome =
                            run_interactive_session(terminal, "sftp", &args, app.auto_reconnect)?;
                        if outcome == Outcome::Success {
                            app.set_last_connected(&server);
                        }
                    }
                }
                KeyCode::Char('f') => {
                    // SCP / SFTP 文件传输
                    if app.selected_server().is_some() {
                        let has_scp = is_command_available("scp");
                        if !has_scp && !is_command_available("sftp") {
                            app.input_mode = InputMode::ShowMessage(
                                "Neither scp nor sftp is available on your system.\n\nOn Windows, make sure the OpenSSH Client is installed:\n  Settings > Optional features > OpenSSH Client".to_string(),
                            );
                        } else {
                            app.input_mode = InputMode::Scp(ScpState::new(has_scp));
                        }
                    }
                }
                KeyCode::Char('r') => {
                    // 切换断线自动重连
                    app.toggle_auto_reconnect();
                }
                KeyCode::Enter => {
                    // SSH
                    if let Some(server) = app.selected_server() {
                        let args = server.to_ssh_args();
                        let outcome =
                            run_interactive_session(terminal, "ssh", &args, app.auto_reconnect)?;
                        if outcome == Outcome::Success {
                            app.set_last_connected(&server);
                        }
                    }
                }
                KeyCode::Char('p') if !app.servers.is_empty() => {
                    app.input_mode =
                        InputMode::BroadcastCommand(BroadcastState::new(app.servers.len()));
                }
                _ => {}
            }
        }
        InputMode::Editing(state) => {
            if key.code == KeyCode::Esc {
                app.input_mode = InputMode::Normal;
                return Ok(false);
            }
            if key.code == KeyCode::Enter {
                match state.fields.validate() {
                    Ok(()) => {
                        let updated_server = state.fields.to_server();
                        if state.server_index < app.servers.len() {
                            app.servers[state.server_index] = updated_server;
                        }
                        let _ = app.save();
                        app.input_mode = InputMode::Normal;
                    }
                    Err(msg) => {
                        app.input_mode = InputMode::ShowMessage(msg);
                    }
                }
                return Ok(false);
            }
            state.fields.handle_key(key.code);
        }
        InputMode::Adding(state) => {
            if key.code == KeyCode::Esc {
                app.input_mode = InputMode::Normal;
                return Ok(false);
            }
            if key.code == KeyCode::Enter {
                match state.fields.validate() {
                    Ok(()) => {
                        let new_server = state.fields.to_server();
                        app.servers.push(new_server);
                        let _ = app.save();
                        app.state.select(Some(app.servers.len() - 1));
                        app.input_mode = InputMode::Normal;
                    }
                    Err(msg) => {
                        app.input_mode = InputMode::ShowMessage(msg);
                    }
                }
                return Ok(false);
            }
            state.fields.handle_key(key.code);
        }
        InputMode::ConfirmDelete(idx) => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let idx = *idx;
                app.input_mode = InputMode::Normal;
                if idx < app.servers.len() {
                    app.servers.remove(idx);
                    if app.servers.is_empty() {
                        app.state.select(None);
                    } else if idx >= app.servers.len() {
                        app.state.select(Some(app.servers.len() - 1));
                    }
                    let _ = app.save();
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.input_mode = InputMode::Normal;
            }
            _ => {}
        },
        InputMode::ShowMessage(_) => match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => {
                app.input_mode = InputMode::Normal;
            }
            _ => {}
        },
        InputMode::BroadcastCommand(state) => match state.phase {
            BroadcastPhase::EnterCommand => match key.code {
                KeyCode::Esc => app.input_mode = InputMode::Normal,
                KeyCode::Char(c) => state.command.push(c),
                KeyCode::Backspace => {
                    state.command.pop();
                }
                KeyCode::Enter if !state.command.is_empty() => {
                    state.phase = BroadcastPhase::SelectServers;
                }
                _ => {}
            },
            BroadcastPhase::SelectServers => match key.code {
                KeyCode::Esc => app.input_mode = InputMode::Normal,
                KeyCode::Char('j') | KeyCode::Down => {
                    state.move_next(app.servers.len());
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    state.move_prev();
                }
                KeyCode::Char(' ') => {
                    state.toggle();
                }
                KeyCode::Enter => {
                    let command = state.command.clone();
                    let targets: Vec<usize> = state
                        .selected
                        .iter()
                        .enumerate()
                        .filter(|(_, &sel)| sel)
                        .map(|(i, _)| i)
                        .collect();
                    app.input_mode = InputMode::Normal;
                    for idx in targets {
                        let server = app.servers[idx].clone();
                        let mut args = server.to_ssh_args();
                        // -t forces pseudo-terminal allocation for the remote command
                        args.insert(0, "-t".to_string());
                        args.push(command.clone());
                        run_external_command(terminal, "ssh", &args)?;
                    }
                }
                _ => {}
            },
        },
        InputMode::SelectingProfile => match key.code {
            KeyCode::Esc => app.input_mode = InputMode::Normal,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => app.next_profile(),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => app.previous_profile(),
            KeyCode::Char('n') => {
                app.input_mode = InputMode::CreatingProfile(String::new());
            }
            KeyCode::Enter => {
                if let Some(idx) = app.profile_state.selected() {
                    let profile = app.profiles[idx].clone();
                    let _ = app.load_profile(&profile);
                }
                app.input_mode = InputMode::Normal;
            }
            _ => {}
        },
        InputMode::CreatingProfile(name) => match key.code {
            KeyCode::Esc => app.input_mode = InputMode::SelectingProfile,
            KeyCode::Char(c) => name.push(c),
            KeyCode::Backspace => {
                name.pop();
            }
            KeyCode::Enter if !name.is_empty() => match app::sanitize_profile_name(name) {
                Some(profile_name) => {
                    if !app.profiles.contains(&profile_name) {
                        app.profiles.push(profile_name.clone());
                        app.profiles.sort();
                    }
                    let _ = app.load_profile(&profile_name);
                    app.input_mode = InputMode::Normal;
                }
                None => {
                    app.input_mode = InputMode::ShowMessage("Invalid profile name.".to_string());
                }
            },
            _ => {}
        },
        InputMode::Scp(state) => {
            let server = app
                .state
                .selected()
                .and_then(|idx| app.servers.get(idx))
                .cloned();

            match key.code {
                KeyCode::Esc => {
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Tab | KeyCode::Down => {
                    state.field_idx = (state.field_idx + 1) % 4;
                }
                KeyCode::BackTab | KeyCode::Up => {
                    state.field_idx = if state.field_idx == 0 {
                        3
                    } else {
                        state.field_idx - 1
                    };
                }
                KeyCode::Char(' ') if state.field_idx == 0 => {
                    state.direction = match state.direction {
                        ScpDirection::Upload => ScpDirection::Download,
                        ScpDirection::Download => ScpDirection::Upload,
                    };
                }
                KeyCode::Char(' ') if state.field_idx == 3 => {
                    state.recursive = !state.recursive;
                }
                KeyCode::Left | KeyCode::Right => match state.field_idx {
                    0 => {
                        state.direction = match state.direction {
                            ScpDirection::Upload => ScpDirection::Download,
                            ScpDirection::Download => ScpDirection::Upload,
                        }
                    }
                    3 => state.recursive = !state.recursive,
                    _ => {}
                },
                KeyCode::Backspace => match state.field_idx {
                    1 => {
                        state.local_path.pop();
                    }
                    2 => {
                        state.remote_path.pop();
                    }
                    _ => {}
                },
                KeyCode::Char(c) => match state.field_idx {
                    // 路径里的空格是正常字符，必须能输入
                    1 => state.local_path.push(c),
                    2 => state.remote_path.push(c),
                    _ => {}
                },
                KeyCode::Enter => {
                    let direction = state.direction;
                    let local_path = state.local_path.clone();
                    let remote_path = state.remote_path.clone();
                    let recursive = state.recursive;
                    app.input_mode = InputMode::Normal;

                    let server = match server {
                        Some(s) => s,
                        None => return Ok(false),
                    };

                    if local_path.trim().is_empty() {
                        app.input_mode =
                            InputMode::ShowMessage("Local path is required.".to_string());
                        return Ok(false);
                    }

                    let outcome = run_file_transfer(
                        terminal,
                        &server,
                        direction,
                        &local_path,
                        &remote_path,
                        recursive,
                        app.auto_reconnect,
                    )?;
                    if outcome == Outcome::Success {
                        app.set_last_connected(&server);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(false)
}
