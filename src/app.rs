use anyhow::Result;
use ratatui::widgets::ListState;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

use crate::server::Server;

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct AppState {
    #[serde(default)]
    last_connected: Option<String>,
    #[serde(default)]
    last_profile: Option<String>,
    /// 断线自动重连，默认开启
    #[serde(default = "default_true")]
    auto_reconnect: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            last_connected: None,
            last_profile: None,
            auto_reconnect: true,
        }
    }
}

pub enum InputMode {
    Normal,
    Adding(AddingState),
    Editing(EditingState),
    ConfirmDelete(usize), // 存储要删除的服务器索引
    ShowMessage(String),  // 显示提示信息
    BroadcastCommand(BroadcastState),
    SelectingProfile,
    CreatingProfile(String),
    Scp(ScpState),
}

/// SCP 传输方向
#[derive(Clone, Copy, PartialEq)]
pub enum ScpDirection {
    /// 本地 -> 远端
    Upload,
    /// 远端 -> 本地
    Download,
}

pub struct ScpState {
    pub field_idx: usize,
    pub direction: ScpDirection,
    pub local_path: String,
    pub remote_path: String,
    pub recursive: bool,
    /// 系统里是否有 scp，没有的话直接走 sftp
    pub scp_available: bool,
}

impl ScpState {
    pub fn new(scp_available: bool) -> Self {
        Self {
            field_idx: 0,
            direction: ScpDirection::Upload,
            local_path: String::new(),
            remote_path: String::new(),
            recursive: false,
            scp_available,
        }
    }
}

pub struct BroadcastState {
    pub phase: BroadcastPhase,
    pub command: String,
    pub selected: Vec<bool>, // one bool per server
    pub list_state: ListState,
}

pub enum BroadcastPhase {
    EnterCommand,
    SelectServers,
}

impl BroadcastState {
    pub fn new(server_count: usize) -> Self {
        Self {
            phase: BroadcastPhase::EnterCommand,
            command: String::new(),
            selected: vec![false; server_count],
            list_state: {
                let mut s = ListState::default();
                s.select(if server_count > 0 { Some(0) } else { None });
                s
            },
        }
    }

    pub fn cursor(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    pub fn move_next(&mut self, len: usize) {
        let i = match self.list_state.selected() {
            Some(i) if i + 1 < len => i + 1,
            Some(i) => i,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn move_prev(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) if i > 0 => i - 1,
            Some(_) => 0,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn toggle(&mut self) {
        let idx = self.cursor();
        if idx < self.selected.len() {
            self.selected[idx] = !self.selected[idx];
        }
    }
}

/// 连接表单的字段集合，添加与编辑共用
pub struct FieldSet {
    pub field_idx: usize,
    pub name: String,
    pub user: String,
    pub host: String,
    pub port: String,
    pub jump_host: String,
}

pub const FIELD_COUNT: usize = 5;

impl FieldSet {
    pub fn new() -> Self {
        Self {
            field_idx: 0,
            name: String::new(),
            user: String::new(),
            host: String::new(),
            port: "22".to_string(),
            jump_host: String::new(),
        }
    }

    pub fn from_server(server: &Server) -> Self {
        Self {
            field_idx: 0,
            name: server.name.clone(),
            user: server.user.clone(),
            host: server.host.clone(),
            port: server.port.clone(),
            jump_host: server.jump_host.clone(),
        }
    }

    fn current_mut(&mut self) -> &mut String {
        match self.field_idx {
            0 => &mut self.name,
            1 => &mut self.user,
            2 => &mut self.host,
            3 => &mut self.port,
            _ => &mut self.jump_host,
        }
    }

    /// 处理表单里的按键，返回 true 表示按键已被消费
    pub fn handle_key(&mut self, key: crossterm::event::KeyCode) -> bool {
        use crossterm::event::KeyCode;
        match key {
            KeyCode::Char(c) => {
                self.current_mut().push(c);
                true
            }
            KeyCode::Backspace => {
                self.current_mut().pop();
                true
            }
            KeyCode::Tab | KeyCode::Down => {
                self.field_idx = (self.field_idx + 1) % FIELD_COUNT;
                true
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.field_idx = if self.field_idx == 0 {
                    FIELD_COUNT - 1
                } else {
                    self.field_idx - 1
                };
                true
            }
            _ => false,
        }
    }

    /// 校验必填字段，返回错误信息
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Name is required".to_string());
        }
        if self.host.trim().is_empty() {
            return Err("Host/IP is required".to_string());
        }
        if !self.port.trim().is_empty() && self.port.trim().parse::<u16>().is_err() {
            return Err("Port must be a number between 1 and 65535".to_string());
        }
        Ok(())
    }

    pub fn to_server(&self) -> Server {
        Server {
            name: self.name.trim().to_string(),
            user: if self.user.trim().is_empty() {
                "root".to_string()
            } else {
                self.user.trim().to_string()
            },
            host: self.host.trim().to_string(),
            port: if self.port.trim().is_empty() {
                "22".to_string()
            } else {
                self.port.trim().to_string()
            },
            jump_host: self.jump_host.trim().to_string(),
        }
    }
}

pub struct AddingState {
    pub fields: FieldSet,
}

impl AddingState {
    pub fn new() -> Self {
        Self {
            fields: FieldSet::new(),
        }
    }
}

pub struct EditingState {
    pub server_index: usize,
    pub fields: FieldSet,
}

impl EditingState {
    pub fn new(server: &Server, index: usize) -> Self {
        Self {
            server_index: index,
            fields: FieldSet::from_server(server),
        }
    }
}

/// 规范化 profile 名称：去掉路径分隔符等非法字符，避免写到配置目录之外
pub fn sanitize_profile_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .trim()
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return None;
    }
    let mut profile_name = cleaned;
    if !profile_name.ends_with(".json") {
        profile_name.push_str(".json");
    }
    Some(profile_name)
}

pub struct App {
    pub servers: Vec<Server>,
    pub state: ListState,
    pub input_mode: InputMode,
    pub pending_g: bool,
    pub profiles: Vec<String>,
    pub profile_state: ListState,
    pub current_profile: String,
    /// 断线后自动重连
    pub auto_reconnect: bool,
    /// 需要展示给用户的提示（例如配置文件损坏）
    pub pending_message: Option<String>,
    config_dir: PathBuf,
    state_path: PathBuf,
    last_connected: Option<String>,
}

impl App {
    pub fn new() -> Result<Self> {
        let config_dir_base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        let app_config_dir = config_dir_base.join("sshx");
        if !app_config_dir.exists() {
            fs::create_dir_all(&app_config_dir)?;
        }
        let state_path = app_config_dir.join("state.json");

        // Scan for profiles
        let mut profiles = Vec::new();
        if let Ok(entries) = fs::read_dir(&app_config_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if file_name != "state.json" {
                        profiles.push(file_name.to_string());
                    }
                }
            }
        }
        profiles.sort();
        if profiles.is_empty() {
            profiles.push("servers.json".to_string());
        }

        let app_state = Self::load_state(&state_path);
        let current_profile = app_state
            .last_profile
            .clone()
            .unwrap_or_else(|| "servers.json".to_string());

        // Ensure current profile is in the list
        if !profiles.contains(&current_profile) {
            profiles.push(current_profile.clone());
            profiles.sort();
        }

        let mut app = Self {
            servers: Vec::new(),
            state: ListState::default(),
            input_mode: InputMode::Normal,
            pending_g: false,
            profiles,
            profile_state: ListState::default(),
            current_profile: current_profile.clone(),
            auto_reconnect: app_state.auto_reconnect,
            pending_message: None,
            config_dir: app_config_dir,
            state_path,
            last_connected: app_state.last_connected,
        };

        app.load_profile(&current_profile)?;

        Ok(app)
    }

    pub fn load_profile(&mut self, profile_name: &str) -> Result<()> {
        let config_path = self.config_dir.join(profile_name);
        let mut servers: Vec<Server> = if config_path.exists() {
            let data = fs::read_to_string(&config_path)?;
            match serde_json::from_str::<Vec<Server>>(&data) {
                Ok(list) => list,
                Err(e) => {
                    // 配置文件损坏时先备份，避免直接覆盖导致数据丢失
                    let backup = config_path.with_extension("json.corrupt");
                    let _ = fs::copy(&config_path, &backup);
                    self.pending_message = Some(format!(
                        "Profile \"{}\" could not be parsed ({}).\n\nA backup was saved to:\n{}\n\nThe list is shown empty; fix or delete the file and restart.",
                        profile_name,
                        e,
                        backup.display()
                    ));
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Reorder: move last-connected server to the top
        if let Some(ref key) = self.last_connected {
            if let Some(pos) = servers.iter().position(|s| Self::server_key(s) == *key) {
                if pos > 0 {
                    let server = servers.remove(pos);
                    servers.insert(0, server);
                }
            }
        }

        self.servers = servers;
        self.current_profile = profile_name.to_string();
        self.state = ListState::default();
        if !self.servers.is_empty() {
            self.state.select(Some(0));
        }

        let _ = self.save_state();
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let config_path = self.config_dir.join(&self.current_profile);
        let data = serde_json::to_string_pretty(&self.servers)?;
        fs::write(config_path, data)?;
        Ok(())
    }

    pub fn next(&mut self) {
        if self.servers.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.servers.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.servers.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.servers.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn select_first(&mut self) {
        if !self.servers.is_empty() {
            self.state.select(Some(0));
        }
    }

    /// 当前选中的服务器（列表为空或选中项越界时返回 None）
    pub fn selected_server(&self) -> Option<Server> {
        self.state
            .selected()
            .and_then(|idx| self.servers.get(idx))
            .cloned()
    }

    pub fn select_last(&mut self) {
        if !self.servers.is_empty() {
            self.state.select(Some(self.servers.len() - 1));
        }
    }

    fn server_key(server: &Server) -> String {
        format!("{}@{}:{}", server.user, server.host, server.port)
    }

    pub fn next_profile(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        let i = match self.profile_state.selected() {
            Some(i) => {
                if i >= self.profiles.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.profile_state.select(Some(i));
    }

    pub fn previous_profile(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        let i = match self.profile_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.profiles.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.profile_state.select(Some(i));
    }

    fn load_state(state_path: &PathBuf) -> AppState {
        if state_path.exists() {
            fs::read_to_string(state_path)
                .ok()
                .and_then(|data| serde_json::from_str(&data).ok())
                .unwrap_or_default()
        } else {
            AppState::default()
        }
    }

    fn save_state(&self) -> Result<()> {
        let app_state = AppState {
            last_connected: self.last_connected.clone(),
            last_profile: Some(self.current_profile.clone()),
            auto_reconnect: self.auto_reconnect,
        };
        let data = serde_json::to_string_pretty(&app_state)?;
        fs::write(&self.state_path, data)?;
        Ok(())
    }

    /// 切换断线自动重连，并持久化设置
    pub fn toggle_auto_reconnect(&mut self) {
        self.auto_reconnect = !self.auto_reconnect;
        let _ = self.save_state();
    }

    pub fn set_last_connected(&mut self, server: &Server) {
        let key = Self::server_key(server);
        self.last_connected = Some(key.clone());
        let _ = self.save_state();

        // Reorder: move the connected server to the top
        if let Some(pos) = self.servers.iter().position(|s| Self::server_key(s) == key) {
            if pos > 0 {
                let s = self.servers.remove(pos);
                self.servers.insert(0, s);
                let _ = self.save();
            }
        }
        if !self.servers.is_empty() {
            self.state.select(Some(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_name_is_sanitized() {
        assert_eq!(sanitize_profile_name("work"), Some("work.json".to_string()));
        assert_eq!(
            sanitize_profile_name("work.json"),
            Some("work.json".to_string())
        );
        // 路径分隔符必须被去掉，避免写到配置目录之外
        assert_eq!(
            sanitize_profile_name("../../etc/passwd"),
            Some("....etcpasswd.json".to_string())
        );
        assert_eq!(sanitize_profile_name(".."), None);
        assert_eq!(sanitize_profile_name("   "), None);
        assert_eq!(sanitize_profile_name("a\\b"), Some("ab.json".to_string()));
    }
}
