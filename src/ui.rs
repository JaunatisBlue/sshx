use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, BroadcastPhase, InputMode, ScpDirection};
use crate::server::Server;

/// 主题色
mod theme {
    use ratatui::style::Color;
    pub const ACCENT: Color = Color::Cyan;
    pub const ACCENT2: Color = Color::Magenta;
    pub const TEXT: Color = Color::Gray;
    pub const DIM: Color = Color::DarkGray;
    pub const WARN: Color = Color::Yellow;
    pub const DANGER: Color = Color::Red;
    pub const OK: Color = Color::Green;
    pub const HEADER_BG: Color = Color::Indexed(236);
    pub const FOOTER_BG: Color = Color::Indexed(235);
}

pub fn ui(f: &mut Frame, app: &mut App) {
    let size = f.size();

    let help_text = help_for(app, size.width);

    // 底部帮助栏按宽度自适应换行
    let help_lines = wrapped_lines(&help_text, size.width);
    let footer_height = help_lines.max(1) as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(footer_height),
        ])
        .split(size);

    render_header(f, chunks[0], app);
    render_server_list(f, chunks[1], app);
    render_footer(f, chunks[2], &help_text);

    // 传输弹窗需要知道当前选中的服务器，先取出（避免与 input_mode 的借用冲突）
    let selected_server = app.selected_server();

    // ---------- 弹窗 ----------
    match &mut app.input_mode {
        InputMode::Adding(state) => render_connection_form(
            f,
            " Add Connection ",
            theme::ACCENT,
            state.fields.field_idx,
            &[
                &state.fields.name,
                &state.fields.user,
                &state.fields.host,
                &state.fields.port,
                &state.fields.jump_host,
            ],
        ),
        InputMode::Editing(state) => render_connection_form(
            f,
            " Edit Connection ",
            theme::ACCENT,
            state.fields.field_idx,
            &[
                &state.fields.name,
                &state.fields.user,
                &state.fields.host,
                &state.fields.port,
                &state.fields.jump_host,
            ],
        ),
        InputMode::ConfirmDelete(idx) => {
            if *idx < app.servers.len() {
                let server_name = app.servers[*idx].name.clone();
                render_confirm_dialog(f, &server_name);
            }
        }
        InputMode::ShowMessage(msg) => render_message_dialog(f, msg),
        InputMode::BroadcastCommand(state) => match state.phase {
            BroadcastPhase::EnterCommand => render_broadcast_command_input(f, &state.command),
            BroadcastPhase::SelectServers => render_broadcast_server_select(f, &app.servers, state),
        },
        InputMode::SelectingProfile => render_profile_selection(
            f,
            &app.profiles,
            &mut app.profile_state,
            &app.current_profile,
        ),
        InputMode::CreatingProfile(name) => render_profile_creation(f, name),
        InputMode::Scp(state) => render_scp_dialog(f, state, selected_server.as_ref()),
        InputMode::Normal => {}
    }
}

fn help_for(app: &App, width: u16) -> String {
    // 终端较窄时使用精简提示，避免帮助栏占满屏幕
    let compact = width < 80;
    match &app.input_mode {
        InputMode::Normal => {
            if compact {
                "Enter SSH | s SFTP | f SCP | n New | i Edit | q Quit".to_string()
            } else {
                "Enter SSH | s SFTP | f SCP | m Mosh | p Broadcast | n New | i Edit | c CopyID | d Delete | e Profile | r Reconnect | q Quit".to_string()
            }
        }
        InputMode::Adding(_) => "Enter Save | Esc Cancel | Tab Next field".to_string(),
        InputMode::Editing(_) => "Enter Save | Esc Cancel | Tab Next field".to_string(),
        InputMode::ConfirmDelete(_) => "y Confirm | n/Esc Cancel".to_string(),
        InputMode::ShowMessage(_) => "Enter/Esc/Space Close".to_string(),
        InputMode::BroadcastCommand(s) => match s.phase {
            BroadcastPhase::EnterCommand => "Enter Next | Esc Cancel".to_string(),
            BroadcastPhase::SelectServers => {
                "Space Toggle | j/k Move | Enter Execute | Esc Cancel".to_string()
            }
        },
        InputMode::SelectingProfile => "Enter Load | n New | Esc Cancel".to_string(),
        InputMode::CreatingProfile(_) => "Enter Create | Esc Cancel".to_string(),
        InputMode::Scp(_) => {
            "Space Toggle | Tab Next field | Enter Transfer | Esc Cancel".to_string()
        }
    }
}

fn wrapped_lines(text: &str, width: u16) -> usize {
    if width == 0 {
        return 1;
    }
    let mut lines = 1;
    let mut current = 0usize;
    for ch in text.chars() {
        let w = ch.to_string().width();
        if current + w > width as usize {
            lines += 1;
            current = w;
        } else {
            current += w;
        }
    }
    lines
}

// ---------------------------------------------------------------- 头部

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let reconnect = if app.auto_reconnect {
        Span::styled(
            "⟳ auto-reconnect",
            Style::default().fg(theme::OK).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("⟳ auto-reconnect off", Style::default().fg(theme::DIM))
    };

    let right = Line::from(vec![
        Span::styled(
            format!("{} ", app.current_profile),
            Style::default()
                .fg(theme::ACCENT2)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("· {} servers  ", app.servers.len()),
            Style::default().fg(theme::TEXT),
        ),
        reconnect,
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ACCENT))
        .style(Style::default().bg(theme::HEADER_BG))
        .title(Span::styled(
            " SSHX ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let right_width = right.width() as u16;
    let left_text = "SSH connection manager";
    let left_width = left_text.width() as u16;

    // 终端太窄时隐藏左侧说明，避免和右侧信息重叠
    if inner.width > left_width + right_width + 2 {
        f.render_widget(
            Paragraph::new(Span::styled(left_text, Style::default().fg(theme::DIM))),
            inner,
        );
    }

    f.render_widget(
        Paragraph::new(right).alignment(Alignment::Right),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height,
        },
    );
}

// ---------------------------------------------------------------- 服务器列表

fn render_server_list(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Span::styled(
            " Connections ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);

    if app.servers.is_empty() {
        f.render_widget(block, area);
        let msg = Line::from(vec![
            Span::styled("No connections yet.", Style::default().fg(theme::WARN)),
            Span::styled("  Press ", Style::default().fg(theme::DIM)),
            Span::styled(
                "n",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to add your first server.",
                Style::default().fg(theme::DIM),
            ),
        ]);
        f.render_widget(Paragraph::new(msg).alignment(Alignment::Center), inner);
        return;
    }

    // 三列布局：名称 | 地址 | 跳板机
    let content_width = inner.width.saturating_sub(3) as usize; // 高亮符号 + 滚动条 + 间隔
    let addr_width = app
        .servers
        .iter()
        .map(|s| format!("{}@{}:{}", s.user, s.host, s.port).width())
        .max()
        .unwrap_or(0)
        .clamp(12, 42);
    let jump_width = app
        .servers
        .iter()
        .filter(|s| !s.jump_host.is_empty())
        .map(|s| s.jump_host.width())
        .max()
        .unwrap_or(0)
        .clamp(8, 30);
    let name_width = content_width
        .saturating_sub(addr_width + jump_width + 2)
        .clamp(8, 32);

    let items: Vec<ListItem> = app
        .servers
        .iter()
        .map(|s| {
            let name = column(&s.name, name_width);
            let addr = column(&format!("{}@{}:{}", s.user, s.host, s.port), addr_width);
            let jump = if s.jump_host.is_empty() {
                String::new()
            } else {
                format!("⇄ {}", truncate(&s.jump_host, jump_width))
            };

            let mut spans = vec![
                Span::styled(
                    name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(addr, Style::default().fg(theme::TEXT)),
            ];
            if !jump.is_empty() {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(jump, Style::default().fg(theme::DIM)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Indexed(24))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");

    f.render_stateful_widget(list, area, &mut app.state);

    // 列表超出可视区域时显示滚动条
    if app.servers.len() > inner.height as usize {
        let mut scrollbar_state =
            ScrollbarState::new(app.servers.len()).position(app.state.selected().unwrap_or(0));
        let scrollbar_area = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y + 1,
            width: 1,
            height: area.height.saturating_sub(2),
        };
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}

// ---------------------------------------------------------------- 底部帮助栏

fn render_footer(f: &mut Frame, area: Rect, help_text: &str) {
    let spans = help_spans(help_text);
    let paragraph = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(theme::FOOTER_BG))
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn help_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, part) in text.split(" | ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(theme::DIM)));
        }
        match part.split_once(' ') {
            Some((key, desc)) => {
                spans.push(Span::styled(
                    format!(" {} ", key),
                    Style::default()
                        .fg(Color::Black)
                        .bg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    format!(" {}", desc),
                    Style::default().fg(theme::TEXT),
                ));
            }
            None => spans.push(Span::styled(
                part.to_string(),
                Style::default().fg(theme::TEXT),
            )),
        }
    }
    spans
}

// ---------------------------------------------------------------- 弹窗：连接表单

fn render_connection_form(
    f: &mut Frame,
    title: &str,
    accent: Color,
    field_idx: usize,
    values: &[&String],
) {
    let size = f.size();
    let area = centered_fixed_rect(64, 21, size);

    // 屏幕不够高时压缩每个输入框的高度
    let available = area.height.saturating_sub(4);
    let field_height = (available.saturating_sub(5) / 5).clamp(1, 3);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));

    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let input_layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(field_height),
                Constraint::Length(field_height),
                Constraint::Length(field_height),
                Constraint::Length(field_height),
                Constraint::Length(field_height),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(area);

    let field_labels = [
        ("Name", "required"),
        ("User", "default: root"),
        ("Host / IP", "required"),
        ("Port", "default: 22"),
        ("Jump Host", "optional: user@host:port"),
    ];

    for (i, (label, hint)) in field_labels.iter().enumerate() {
        let value = values[i];
        let active = field_idx == i;
        let border_style = if active {
            Style::default()
                .fg(theme::WARN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::DIM)
        };
        let title_style = if active {
            Style::default()
                .fg(theme::WARN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(Line::from(vec![
                Span::styled(format!(" {} ", label), title_style),
                Span::styled(format!("· {}", hint), Style::default().fg(theme::DIM)),
            ]));

        let input = Paragraph::new(value.as_str())
            .style(if active {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(theme::TEXT)
            })
            .block(block);
        f.render_widget(input, input_layout[i]);

        if active {
            let inner = Rect {
                x: input_layout[i].x + 1,
                y: input_layout[i].y + 1,
                width: input_layout[i].width.saturating_sub(2),
                height: 1,
            };
            f.set_cursor(inner.x + value.width() as u16, inner.y);
        }
    }
}

// ---------------------------------------------------------------- 弹窗：SCP

fn render_scp_dialog(f: &mut Frame, state: &crate::app::ScpState, server: Option<&Server>) {
    let size = f.size();
    let area = centered_fixed_rect(68, 19, size);

    let tool = if state.scp_available {
        "scp  (falls back to SFTP if it fails)"
    } else {
        "SFTP  (scp not found)"
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::OK))
        .title(Line::from(vec![
            Span::styled(
                " File Transfer ",
                Style::default().fg(theme::OK).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("· {} ", tool), Style::default().fg(theme::DIM)),
        ]));

    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(2),
        ])
        .split(area);

    // 方向
    let dir_text = match state.direction {
        ScpDirection::Upload => "Upload  (local → remote)",
        ScpDirection::Download => "Download (remote → local)",
    };
    let dir_active = state.field_idx == 0;
    let dir_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(field_style(dir_active))
        .title(Span::styled(" Direction ", field_title_style(dir_active)));
    f.render_widget(
        Paragraph::new(Span::styled(dir_text, Style::default().fg(Color::White))).block(dir_block),
        inner[0],
    );

    // 本地路径
    let local_active = state.field_idx == 1;
    let local_label = match state.direction {
        ScpDirection::Upload => "Local source",
        ScpDirection::Download => "Local destination",
    };
    let local_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(field_style(local_active))
        .title(Span::styled(
            format!(" {} ", local_label),
            field_title_style(local_active),
        ));
    f.render_widget(
        Paragraph::new(Span::styled(
            state.local_path.as_str(),
            Style::default().fg(Color::White),
        ))
        .block(local_block),
        inner[1],
    );

    // 远端路径
    let remote_active = state.field_idx == 2;
    let remote_label = match state.direction {
        ScpDirection::Upload => "Remote destination",
        ScpDirection::Download => "Remote source",
    };
    let remote_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(field_style(remote_active))
        .title(Span::styled(
            format!(" {} (empty = ~) ", remote_label),
            field_title_style(remote_active),
        ));
    f.render_widget(
        Paragraph::new(Span::styled(
            state.remote_path.as_str(),
            Style::default().fg(Color::White),
        ))
        .block(remote_block),
        inner[2],
    );

    // 递归选项
    let rec_active = state.field_idx == 3;
    let rec_text = if state.recursive {
        "[x] recursive (copy directories)"
    } else {
        "[ ] recursive (copy directories)"
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            rec_text,
            Style::default()
                .fg(if state.recursive {
                    theme::OK
                } else {
                    theme::TEXT
                })
                .add_modifier(if rec_active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(field_style(rec_active))
                .title(Span::styled(" Options ", field_title_style(rec_active))),
        ),
        inner[3],
    );

    // 命令预览 + 操作提示
    let preview = Line::from(vec![
        Span::styled("$ ", Style::default().fg(theme::DIM)),
        Span::styled(
            command_preview(state, server),
            Style::default().fg(theme::ACCENT),
        ),
    ]);
    let hint = Line::from(vec![
        Span::styled(
            " Space ",
            Style::default()
                .fg(Color::Black)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" toggle   ", Style::default().fg(theme::TEXT)),
        Span::styled(
            " Enter ",
            Style::default()
                .fg(Color::Black)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" transfer   ", Style::default().fg(theme::TEXT)),
        Span::styled(
            " Esc ",
            Style::default()
                .fg(Color::Black)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel", Style::default().fg(theme::TEXT)),
    ]);
    f.render_widget(
        Paragraph::new(vec![preview, hint]).alignment(Alignment::Center),
        inner[4],
    );

    // 光标放在当前输入框里
    let cursor_target = match state.field_idx {
        1 => Some((
            inner[1].x + 1 + state.local_path.width() as u16,
            inner[1].y + 1,
        )),
        2 => Some((
            inner[2].x + 1 + state.remote_path.width() as u16,
            inner[2].y + 1,
        )),
        _ => None,
    };
    if let Some((x, y)) = cursor_target {
        f.set_cursor(x, y);
    }
}

/// 传输命令预览
fn command_preview(state: &crate::app::ScpState, server: Option<&Server>) -> String {
    let server = match server {
        Some(s) => s,
        None => return "—".to_string(),
    };
    let remote = server.remote_spec(&state.remote_path);
    let local = crate::server::normalize_local_path(&state.local_path);
    let local = if local.is_empty() {
        "<local path>".to_string()
    } else {
        local
    };
    let recursive = if state.recursive { "-r " } else { "" };
    match state.direction {
        ScpDirection::Upload => {
            format!(
                "scp {}{} {}",
                recursive,
                quote_if_needed(&local),
                quote_if_needed(&remote)
            )
        }
        ScpDirection::Download => {
            format!(
                "scp {}{} {}",
                recursive,
                quote_if_needed(&remote),
                quote_if_needed(&local)
            )
        }
    }
}

/// 路径里有空格时加上引号，和真实执行的参数保持一致
fn quote_if_needed(path: &str) -> String {
    if path.contains(' ') {
        format!("\"{}\"", path)
    } else {
        path.to_string()
    }
}

fn field_style(active: bool) -> Style {
    if active {
        Style::default()
            .fg(theme::WARN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::DIM)
    }
}

fn field_title_style(active: bool) -> Style {
    if active {
        Style::default()
            .fg(theme::WARN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT)
    }
}

// ---------------------------------------------------------------- 弹窗：其它

fn render_confirm_dialog(f: &mut Frame, server_name: &str) {
    let size = f.size();
    let area = centered_fixed_rect(50, 8, size);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::DANGER))
        .title(Span::styled(
            " Delete Connection ",
            Style::default()
                .fg(theme::DANGER)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(2), Constraint::Length(2)].as_ref())
        .split(area);

    let msg = Paragraph::new(Line::from(vec![
        Span::styled("Delete ", Style::default().fg(theme::TEXT)),
        Span::styled(
            format!("\"{}\"", server_name),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("?", Style::default().fg(theme::TEXT)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(msg, inner[0]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            " y ",
            Style::default()
                .fg(Color::Black)
                .bg(theme::DANGER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" confirm   ", Style::default().fg(theme::TEXT)),
        Span::styled(
            " n ",
            Style::default()
                .fg(Color::Black)
                .bg(theme::DIM)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel", Style::default().fg(theme::TEXT)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(hint, inner[1]);
}

fn render_message_dialog(f: &mut Frame, message: &str) {
    let size = f.size();
    let lines: Vec<&str> = message.lines().collect();
    let height = (lines.len() as u16 + 5).min(22);
    let area = centered_fixed_rect(60, height, size);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::WARN))
        .title(Span::styled(
            " Notice ",
            Style::default()
                .fg(theme::WARN)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
        .split(area);

    let text = Paragraph::new(message)
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: true });
    f.render_widget(text, inner[0]);

    let hint = Paragraph::new("Enter / Esc / Space to close")
        .style(Style::default().fg(theme::DIM))
        .alignment(Alignment::Center);
    f.render_widget(hint, inner[1]);
}

fn render_broadcast_command_input(f: &mut Frame, command: &str) {
    let size = f.size();
    let area = centered_fixed_rect(60, 8, size);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ACCENT2))
        .title(Span::styled(
            " Broadcast — Command ",
            Style::default()
                .fg(theme::ACCENT2)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Length(1)].as_ref())
        .split(area);

    let input = Paragraph::new(Span::styled(command, Style::default().fg(Color::White))).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme::DIM))
            .title(Span::styled(" Command ", Style::default().fg(theme::TEXT))),
    );
    f.render_widget(input, inner[0]);

    f.set_cursor(inner[0].x + command.width() as u16 + 1, inner[0].y + 1);

    let hint = Paragraph::new("Enter confirm │ Esc cancel")
        .style(Style::default().fg(theme::DIM))
        .alignment(Alignment::Center);
    f.render_widget(hint, inner[1]);
}

fn render_broadcast_server_select(
    f: &mut Frame,
    servers: &[crate::server::Server],
    state: &mut crate::app::BroadcastState,
) {
    let size = f.size();
    let height = (servers.len() as u16 + 6).min(size.height.saturating_sub(4));
    let area = centered_fixed_rect(64, height, size);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ACCENT2))
        .title(Span::styled(
            " Broadcast — Select Servers ",
            Style::default()
                .fg(theme::ACCENT2)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
        .split(area);

    let items: Vec<ListItem> = servers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let check = if state.selected.get(i).copied().unwrap_or(false) {
                Span::styled("[x]", Style::default().fg(theme::OK))
            } else {
                Span::styled("[ ]", Style::default().fg(theme::DIM))
            };
            let line = Line::from(vec![
                check,
                Span::raw(" "),
                Span::styled(
                    s.name.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{}@{}:{}", s.user, s.host, s.port),
                    Style::default().fg(theme::TEXT),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::Indexed(24))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");
    f.render_stateful_widget(list, inner[0], &mut state.list_state);

    let hint = Paragraph::new("Space toggle │ j/k move │ Enter execute │ Esc cancel")
        .style(Style::default().fg(theme::DIM))
        .alignment(Alignment::Center);
    f.render_widget(hint, inner[1]);
}

fn render_profile_selection(
    f: &mut Frame,
    profiles: &[String],
    state: &mut ListState,
    current: &str,
) {
    let size = f.size();
    let height = (profiles.len() as u16 + 5).min(size.height.saturating_sub(4));
    let area = centered_fixed_rect(44, height, size);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ACCENT2))
        .title(Span::styled(
            " Profiles ",
            Style::default()
                .fg(theme::ACCENT2)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
        .split(area);

    let items: Vec<ListItem> = profiles
        .iter()
        .map(|p| {
            let content = if p == current {
                Line::from(vec![
                    Span::styled("● ", Style::default().fg(theme::OK)),
                    Span::styled(
                        p.clone(),
                        Style::default()
                            .fg(theme::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(Span::styled(
                    format!("  {}", p),
                    Style::default().fg(theme::TEXT),
                ))
            };
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::Indexed(24))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");
    f.render_stateful_widget(list, inner[0], state);

    let hint = Paragraph::new("Enter load │ n new │ Esc cancel")
        .style(Style::default().fg(theme::DIM))
        .alignment(Alignment::Center);
    f.render_widget(hint, inner[1]);
}

fn render_profile_creation(f: &mut Frame, name: &str) {
    let size = f.size();
    let area = centered_fixed_rect(44, 8, size);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ACCENT2))
        .title(Span::styled(
            " New Profile ",
            Style::default()
                .fg(theme::ACCENT2)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Length(1)].as_ref())
        .split(area);

    let input = Paragraph::new(Span::styled(name, Style::default().fg(Color::White))).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme::DIM))
            .title(Span::styled(
                " Profile name ",
                Style::default().fg(theme::TEXT),
            )),
    );
    f.render_widget(input, inner[0]);

    f.set_cursor(inner[0].x + name.width() as u16 + 1, inner[0].y + 1);

    let hint = Paragraph::new("Enter create │ Esc cancel")
        .style(Style::default().fg(theme::DIM))
        .alignment(Alignment::Center);
    f.render_widget(hint, inner[1]);
}

// ---------------------------------------------------------------- 工具函数

fn centered_fixed_rect(width_percent: u16, height: u16, r: Rect) -> Rect {
    // 弹窗高度不能超过屏幕高度，否则小终端下会错位
    let height = height.min(r.height);

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Percentage(width_percent),
            Constraint::Fill(1),
        ])
        .split(popup_layout[1])[1]
}

/// 按显示宽度截断，超出部分用 … 代替
fn truncate(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = ch.to_string().width();
        if w + cw > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

/// 按显示宽度右侧补空格
fn pad_to(s: &str, width: usize) -> String {
    let w = s.width();
    if w >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - w))
    }
}

/// 截断并补齐到固定宽度（表格列）
fn column(s: &str, width: usize) -> String {
    let trimmed = truncate(s, width);
    pad_to(&trimmed, width)
}
