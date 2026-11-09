//! 控制台输入模式管理（Windows 关键）。
//!
//! crossterm 的 `EnableMouseCapture` 会把控制台输入模式**整体覆盖**成
//! `ENABLE_MOUSE_MODE`（只保留鼠标相关位），`ENABLE_LINE_INPUT` / `ENABLE_ECHO_INPUT`
//! 会被清掉；而 `DisableMouseCapture` 恢复的是 crossterm 在启用鼠标捕获时保存的模式，
//! 那个模式本身就是 raw mode。
//!
//! 结果就是：TUI 退出后启动的外部程序（sftp / ssh）拿到的是一个「没有行输入、没有回显」
//! 的控制台，`sftp.exe` 用 `ReadConsole` 读命令时读不到任何东西，表现为
//! **打开 sftp 后无法输入**。
//!
//! 这里的做法是：程序启动时（任何 crossterm 操作之前）保存控制台输入模式，
//! 在启动外部程序之前、以及程序退出时把它原样还原回去。

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::OnceLock;
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, STD_INPUT_HANDLE,
    };

    static ORIGINAL_INPUT_MODE: OnceLock<u32> = OnceLock::new();

    /// 读取当前控制台输入模式，失败返回 None（例如没有控制台）
    fn current_input_mode() -> Option<u32> {
        unsafe {
            let handle = GetStdHandle(STD_INPUT_HANDLE);
            if handle == 0 {
                return None;
            }
            let mut mode: u32 = 0;
            if GetConsoleMode(handle, &mut mode) == 0 {
                return None;
            }
            Some(mode)
        }
    }

    fn apply_input_mode(mode: u32) -> bool {
        unsafe {
            let handle = GetStdHandle(STD_INPUT_HANDLE);
            if handle == 0 {
                return false;
            }
            SetConsoleMode(handle, mode) != 0
        }
    }

    /// 记录程序启动时的控制台输入模式（必须在任何终端操作之前调用）
    pub fn remember_original_mode() {
        if let Some(mode) = current_input_mode() {
            let _ = ORIGINAL_INPUT_MODE.set(mode);
        }
    }

    /// 把控制台输入模式恢复成程序启动时的样子
    pub fn restore_original_mode() {
        if let Some(mode) = ORIGINAL_INPUT_MODE.get() {
            apply_input_mode(*mode);
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn remember_original_mode() {}
    pub fn restore_original_mode() {}
}

pub use imp::{remember_original_mode, restore_original_mode};
