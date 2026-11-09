use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::Backend, Terminal};
use std::{
    io::{self, Write},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::app::ScpDirection;
use crate::console;
use crate::server::Server;

/// 断线后最多自动重连的次数
pub const RECONNECT_ATTEMPTS: u32 = 3;
/// 两次重连之间的固定间隔（秒）
pub const RECONNECT_DELAY_SECS: u64 = 10;

/// 外部命令的执行结果
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Outcome {
    /// 正常结束（远端主动 exit，或传输成功）
    Success,
    /// 失败，并且已经放弃重连
    Failed,
    /// 被用户中断
    Aborted,
}

/// Ctrl+C / SIGINT 到达时置位。
///
/// 外部会话（ssh / sftp / scp）期间控制台不在 raw mode，Ctrl+C 会变成真正的
/// 中断信号打到整个进程组——子进程会退出，父进程如果不做处理也会一起被杀，
/// 用户就再也回不到 TUI 了。这里用标志位把「中断」记录下来，由主循环决定
/// 是重连还是直接返回界面。
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// 注册 Ctrl+C 处理器（程序启动时调用一次）
pub fn install_interrupt_handler() {
    let _ = ctrlc::set_handler(|| {
        INTERRUPTED.store(true, Ordering::SeqCst);
    });
}

/// 读取并清除中断标志
fn take_interrupt() -> bool {
    INTERRUPTED.swap(false, Ordering::SeqCst)
}

/// 退出 TUI，把终端交还给外部命令
pub fn suspend_tui<B: Backend + std::io::Write>(terminal: &mut Terminal<B>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    // crossterm 的鼠标捕获会整体改写控制台输入模式，导致外部程序（如 sftp）
    // 读不到键盘输入，这里把输入模式还原成程序启动时的样子。
    console::restore_original_mode();
    Ok(())
}

/// 重新进入 TUI
pub fn restore_tui<B: Backend + std::io::Write>(terminal: &mut Terminal<B>) -> io::Result<()> {
    enable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    )?;
    terminal.clear()?;
    Ok(())
}

/// 运行一次外部命令（不做自动重连）
pub fn run_external_command<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    program: &str,
    args: &[String],
) -> io::Result<()> {
    suspend_tui(terminal)?;

    println!("Running {} {}...", program, args.join(" "));
    let status = Command::new(program).args(args).status();

    match status {
        Ok(s) => {
            if !s.success() {
                println!("Command exited with status: {}", s);
                println!("Press Enter to continue...");
                wait_for_enter()?;
            }
        }
        Err(e) => {
            println!("Failed to execute {}: {}", program, e);
            println!("Press Enter to continue...");
            wait_for_enter()?;
        }
    }

    restore_tui(terminal)
}

/// 运行交互式会话（如 ssh / sftp），连接断开后可以自动重连。
///
/// * 正常退出（状态码 0）时不再重连——通常是用户主动 `exit`。
/// * 非零退出码或进程启动失败时，等待固定间隔后重连，最多重连
///   [`RECONNECT_ATTEMPTS`] 次；仍然失败则放弃并返回 TUI。
/// * 重连等待期间按 Enter / Esc / q 或 Ctrl+C 可以立刻中止。
pub fn run_interactive_session<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    program: &str,
    args: &[String],
    auto_reconnect: bool,
) -> io::Result<Outcome> {
    suspend_tui(terminal)?;

    let label = format!("{} {}", program, args.join(" "));
    let outcome = reconnect_loop(&label, auto_reconnect, || {
        let status = Command::new(program).args(args).status()?;
        Ok(status.success())
    });

    restore_tui(terminal)?;
    Ok(outcome)
}

/// 文件传输：优先使用 `scp`，`scp` 不可用或失败时自动回退到 `sftp`。
///
/// Windows 自带的 OpenSSH 客户端版本较老，`scp` 走的是传统的 SCP 协议，
/// 遇到只支持 SFTP 的服务器会直接失败，因此这里保留一条 SFTP 兜底路径。
pub fn run_file_transfer<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    server: &Server,
    direction: ScpDirection,
    local_path: &str,
    remote_path: &str,
    recursive: bool,
    auto_reconnect: bool,
) -> io::Result<Outcome> {
    suspend_tui(terminal)?;

    let outcome = transfer(
        server,
        direction,
        local_path,
        remote_path,
        recursive,
        auto_reconnect,
    );

    restore_tui(terminal)?;
    Ok(outcome)
}

/// 具体的传输实现：先 scp，失败再 sftp
fn transfer(
    server: &Server,
    direction: ScpDirection,
    local_path: &str,
    remote_path: &str,
    recursive: bool,
    auto_reconnect: bool,
) -> Outcome {
    let scp_available = is_command_available("scp");
    if scp_available {
        let args = match direction {
            ScpDirection::Upload => server.to_scp_upload_args(local_path, remote_path, recursive),
            ScpDirection::Download => {
                server.to_scp_download_args(remote_path, local_path, recursive)
            }
        };
        let label = format!("scp {}", args.join(" "));
        let outcome = reconnect_loop(&label, auto_reconnect, || {
            let status = Command::new("scp").args(&args).status()?;
            Ok(status.success())
        });
        match outcome {
            Outcome::Success => return Outcome::Success,
            // 用户主动中断的，不要再换一种方式重试
            Outcome::Aborted => return Outcome::Aborted,
            Outcome::Failed => {}
        }
        println!();
        println!("scp did not succeed — retrying the transfer over SFTP...");
    } else {
        println!("scp was not found in PATH — using SFTP instead.");
    }

    sftp_transfer(
        server,
        direction,
        local_path,
        remote_path,
        recursive,
        auto_reconnect,
    )
}

/// SFTP 兜底传输：把 `get` / `put` 命令写进 sftp 的标准输入
fn sftp_transfer(
    server: &Server,
    direction: ScpDirection,
    local_path: &str,
    remote_path: &str,
    recursive: bool,
    auto_reconnect: bool,
) -> Outcome {
    if !is_command_available("sftp") {
        println!("\x1b[31msftp is not available either — install the OpenSSH client.\x1b[0m");
        println!("Press Enter to continue...");
        let _ = wait_for_enter();
        return Outcome::Failed;
    }

    let command = match direction {
        ScpDirection::Upload => sftp_put_command(local_path, remote_path, recursive),
        ScpDirection::Download => sftp_get_command(remote_path, local_path, recursive),
    };
    let args = server.to_sftp_args();
    let label = format!("sftp {} [{}]", args.join(" "), command);

    reconnect_loop(&label, auto_reconnect, || {
        run_sftp_with_command(&args, &command)
    })
}

/// 组装 sftp 的 `get` 命令
fn sftp_get_command(remote_path: &str, local_path: &str, recursive: bool) -> String {
    let flag = if recursive { "-R " } else { "" };
    format!(
        "get {}{} {}",
        flag,
        sftp_quote(&sftp_remote_path(remote_path)),
        sftp_quote(&sftp_local_path(local_path))
    )
}

/// 组装 sftp 的 `put` 命令
fn sftp_put_command(local_path: &str, remote_path: &str, recursive: bool) -> String {
    let flag = if recursive { "-R " } else { "" };
    format!(
        "put {}{} {}",
        flag,
        sftp_quote(&sftp_local_path(local_path)),
        sftp_quote(&sftp_remote_path(remote_path))
    )
}

/// 远端路径留空时表示家目录（sftp 里用 `.`）
fn sftp_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        ".".to_string()
    } else {
        trimmed.to_string()
    }
}

/// sftp 命令行里的路径需要用双引号包起来（空格是常见的坑）
fn sftp_quote(path: &str) -> String {
    let escaped = path.trim().replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

/// 本地路径统一成正斜杠：Windows 上 `\` 在 sftp 命令里是转义符
fn sftp_local_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

/// 启动 sftp，把命令写进 stdin，EOF 之后 sftp 自己退出
fn run_sftp_with_command(args: &[String], command: &str) -> io::Result<bool> {
    let mut child = Command::new("sftp")
        .args(args)
        .stdin(Stdio::piped())
        .spawn()?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| io::Error::other("failed to open sftp stdin"))?;
        writeln!(stdin, "{}", command)?;
        stdin.flush()?;
    }
    // 关闭 stdin，sftp 读到 EOF 后退出
    drop(child.stdin.take());

    let status = child.wait()?;
    Ok(status.success())
}

/// 统一的「执行 + 断线重连」循环
///
/// `run` 返回 `Ok(true)` 表示这次执行成功；失败后按固定间隔重连，
/// 最多 [`RECONNECT_ATTEMPTS`] 次。
fn reconnect_loop<F>(label: &str, auto_reconnect: bool, run: F) -> Outcome
where
    F: FnMut() -> io::Result<bool>,
{
    reconnect_loop_with_delay(label, auto_reconnect, RECONNECT_DELAY_SECS, run)
}

/// 与 [`reconnect_loop`] 相同，但重连间隔可以指定（便于测试）
fn reconnect_loop_with_delay<F>(
    label: &str,
    auto_reconnect: bool,
    delay_secs: u64,
    mut run: F,
) -> Outcome
where
    F: FnMut() -> io::Result<bool>,
{
    let mut attempt: u32 = 0;

    loop {
        // 清掉上一轮遗留的中断标志
        take_interrupt();

        println!("\x1b[2m── {} ──\x1b[0m", label);

        match run() {
            Ok(true) => return Outcome::Success,
            Ok(false) => println!("\x1b[31m✗ {} failed.\x1b[0m", label),
            Err(e) => println!("\x1b[31m✗ Failed to run {}: {}\x1b[0m", label, e),
        }

        // 运行期间用户按了 Ctrl+C：不要再重连，直接回界面
        if take_interrupt() {
            println!("\x1b[33mInterrupted — not reconnecting.\x1b[0m");
            return Outcome::Aborted;
        }

        if !auto_reconnect {
            println!("Press Enter to continue...");
            let _ = wait_for_enter();
            return Outcome::Failed;
        }

        if attempt >= RECONNECT_ATTEMPTS {
            println!(
                "\x1b[31m✗ Giving up after {} reconnect attempt(s).\x1b[0m",
                RECONNECT_ATTEMPTS
            );
            return Outcome::Failed;
        }

        attempt += 1;
        println!(
            "\x1b[33m↻ Reconnecting (attempt {}/{}) in {}s — press Enter to abort...\x1b[0m",
            attempt, RECONNECT_ATTEMPTS, delay_secs
        );
        if wait_for_abort(delay_secs) {
            println!("Reconnect aborted.");
            return Outcome::Aborted;
        }
    }
}

/// 等待指定秒数，期间用户按 Enter / Esc / q 或 Ctrl+C 可以立刻中止。返回 true 表示被中止。
fn wait_for_abort(seconds: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(seconds);

    while Instant::now() < deadline {
        if take_interrupt() {
            return true;
        }
        let ready = crossterm::event::poll(Duration::from_millis(200)).unwrap_or(false);
        if ready {
            if let Ok(Event::Key(key)) = crossterm::event::read() {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q')) {
                    return true;
                }
            }
        }
    }
    false
}

/// Wait for Enter key press in a cross-platform way
fn wait_for_enter() -> io::Result<()> {
    io::stdout().flush()?;

    loop {
        if let Event::Key(key) = crossterm::event::read()? {
            if key.code == KeyCode::Enter {
                break;
            }
        }
    }
    Ok(())
}

/// Check if a command is available in the system
pub fn is_command_available(command: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        Command::new("where")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new("which")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

/// Windows 上没有 ssh-copy-id，这里用自带的实现：
/// 直接把公钥内容通过 stdin 传给远端的 `cat >> ~/.ssh/authorized_keys`。
#[cfg(target_os = "windows")]
fn copy_id_windows(args: &[String]) -> io::Result<std::process::ExitStatus> {
    use std::env;
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::process::{Command as Cmd, Stdio};

    let userprofile = env::var("USERPROFILE")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "USERPROFILE not set"))?;
    let ssh_dir = PathBuf::from(userprofile).join(".ssh");

    // 依次尝试常见的公钥文件
    let candidates = ["id_ed25519.pub", "id_rsa.pub", "id_ecdsa.pub", "id_dsa.pub"];
    let public_key = candidates
        .iter()
        .map(|c| ssh_dir.join(c))
        .find(|p| p.exists())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "No public key found in {}. Generate one with: ssh-keygen -t ed25519",
                    ssh_dir.display()
                ),
            )
        })?;

    let key_content = std::fs::read_to_string(&public_key)?;
    // 保证公钥以换行结尾，否则远端追加时会和已有内容粘在一起
    let key_content = if key_content.ends_with('\n') {
        key_content
    } else {
        format!("{}\n", key_content)
    };

    // 解析出目标主机与选项
    let mut ssh_args: Vec<String> = Vec::new();
    let mut target = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-p" | "-o" => {
                if i + 1 < args.len() {
                    ssh_args.push(args[i].to_string());
                    ssh_args.push(args[i + 1].clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            arg => {
                target = arg.to_string();
                i += 1;
            }
        }
    }

    ssh_args.push("-i".to_string());
    ssh_args.push(public_key.with_extension("").to_string_lossy().to_string());
    ssh_args.push(target);
    ssh_args.push(
        "umask 077; mkdir -p ~/.ssh && cat >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"
            .to_string(),
    );

    let mut child = Cmd::new("ssh")
        .args(&ssh_args)
        .stdin(Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(key_content.as_bytes())?;
        stdin.flush()?;
    }
    // 关闭 stdin，让远端 cat 收到 EOF
    drop(child.stdin.take());

    child.wait()
}

/// Run ssh-copy-id with platform-specific implementation
pub fn run_ssh_copy_id<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    args: &[String],
) -> io::Result<()> {
    suspend_tui(terminal)?;

    #[cfg(target_os = "windows")]
    {
        println!("Copying SSH public key (built-in implementation)...");
        match copy_id_windows(args) {
            Ok(s) if s.success() => {
                println!("Key copied successfully.");
            }
            Ok(s) => {
                println!("Command exited with status: {}", s);
                println!("Press Enter to continue...");
                wait_for_enter()?;
            }
            Err(e) => {
                println!("Failed to copy key: {}", e);
                println!("Press Enter to continue...");
                wait_for_enter()?;
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        println!("Running ssh-copy-id {}...", args.join(" "));
        let status = Command::new("ssh-copy-id").args(args).status();

        match status {
            Ok(s) => {
                if !s.success() {
                    println!("Command exited with status: {}", s);
                    println!("Press Enter to continue...");
                    wait_for_enter()?;
                }
            }
            Err(e) => {
                println!("Failed to execute ssh-copy-id: {}", e);
                println!("Press Enter to continue...");
                wait_for_enter()?;
            }
        }
    }

    restore_tui(terminal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn sftp_commands_are_quoted() {
        assert_eq!(
            sftp_get_command("/var/log", "D:/my logs", false),
            "get \"/var/log\" \"D:/my logs\""
        );
        assert_eq!(
            sftp_put_command("C:\\tmp\\a b.txt", "/tmp", true),
            "put -R \"C:/tmp/a b.txt\" \"/tmp\""
        );
    }

    #[test]
    fn empty_remote_path_means_home() {
        assert_eq!(sftp_get_command("", "a.txt", false), "get \".\" \"a.txt\"");
    }

    #[test]
    fn reconnect_gives_up_after_three_attempts() {
        let calls = Cell::new(0u32);
        let outcome = reconnect_loop_with_delay("test", true, 0, || {
            calls.set(calls.get() + 1);
            Ok(false)
        });
        assert_eq!(outcome, Outcome::Failed);
        // 首次执行 + 3 次重连
        assert_eq!(calls.get(), RECONNECT_ATTEMPTS + 1);
    }

    #[test]
    fn reconnect_stops_on_success() {
        let calls = Cell::new(0u32);
        let outcome = reconnect_loop_with_delay("test", true, 0, || {
            calls.set(calls.get() + 1);
            Ok(true)
        });
        assert_eq!(outcome, Outcome::Success);
        assert_eq!(calls.get(), 1);
    }
}
