# sshx

`sshx` is a simple, terminal-based SSH connection manager written in Rust. It provides a TUI (Text User Interface) to manage your SSH connections, allowing you to add, edit, delete, and connect to servers quickly.

## Features

- **Multiple Profiles**: Organize servers into different scenarios (e.g., Lab, Home, Cloud). Switch between them easily or create new ones.
- **TUI Interface**: Built with `ratatui` for a smooth terminal experience.
- **Manage Servers**: Add, edit, and delete server configurations easily.
- **One-Key Connection**: Connect to your saved servers via `ssh`, `sftp`, or `mosh` with a single keypress.
- **Auto Reconnect**: If the connection drops, `sshx` waits 10s and reconnects (up to 3 attempts) before giving up and returning to the TUI (toggle with `r`).
- **Keepalive**: SSH sessions are opened with `ServerAliveInterval` so dead connections are detected quickly instead of hanging.
- **File Transfer**: Upload and download files straight from the interface, including recursive directory copies. `scp` is used first and falls back to SFTP automatically.
- **Broadcast Command**: Send the same command to multiple servers at once — pick your targets interactively, then execute.
- **Jump Host Support**: Connect through a bastion/proxy server using SSH `-J` option.
- **Mosh Support**: Launch `mosh` sessions directly from the interface (with availability check).
- **SFTP Support**: Open SFTP sessions for file transfers.
- **Key Management**: Quickly copy your public key to a server using `ssh-copy-id`.
- **Vim-style Navigation**: Use `j`/`k` to move, `gg` to jump to the top, `G` to jump to the bottom.
- **Last-Connected Sorting**: The most recently connected server is automatically moved to the top of the list.
- **Delete Confirmation**: Prevent accidental deletion with a confirmation dialog.
- **Persistent Storage**: Server configurations are saved in JSON format in your user configuration directory.
- **Windows Support**: Works on Windows — SSH options are emitted before the destination, local paths are normalized for `scp`, `ssh-copy-id` has a built-in fallback, and the console input mode is restored before launching external programs, so `sftp`/`scp` accept typed input and your shell keeps its echo after `sshx` exits.

## Installation

### Windows (recommended)

1. Install the **OpenSSH Client**: *Settings → System → Optional features → View features →
   OpenSSH Client*. (Windows 10 1809 and newer can also install it from the Microsoft Store.)
2. Download `sshx-windows-amd64-<version>.zip` from the
   [Releases page](https://github.com/JaunatisBlue/sshx/releases).
3. Unzip it anywhere and run `sshx.exe`.

No installation or admin rights are required — `sshx` keeps its configuration in
`%APPDATA%\sshx`.

### Linux / macOS

Build from source:

```bash
git clone https://github.com/JaunatisBlue/sshx.git
cd sshx
cargo build --release
```

The binary will be at `target/release/sshx`.

### Prerequisites

- `ssh` client (Windows: OpenSSH Client, as above)
- `scp` / `sftp` for file transfers (shipped with the OpenSSH client)
- `mosh` (optional, for mosh support)
- `ssh-copy-id` (optional, for key copying; built-in fallback on Windows)

## Usage

Run `sshx` (if installed to PATH) or `cargo run`.

### Key Bindings

**Navigation:**

| Key | Action |
|-----|--------|
| `j` / `Down` / `Tab` | Select next server |
| `k` / `Up` / `Shift+Tab` | Select previous server |
| `gg` | Jump to first server |
| `G` | Jump to last server |
| `q` / `Esc` / `Ctrl+C` / `Ctrl+D` | Quit |

**Actions:**

| Key | Action |
|-----|--------|
| `Enter` | Connect via `ssh` (auto-reconnects if the connection drops) |
| `s` | Connect via `sftp` |
| `f` | Transfer files via `scp` |
| `m` | Connect via `mosh` |
| `p` | Broadcast command to multiple servers |
| `e` | Open profile selection menu |
| `n` | Add a new server |
| `i` | Edit the selected server |
| `c` | Copy SSH public key (`ssh-copy-id`) |
| `d` | Delete the selected server (with confirmation) |
| `r` | Toggle auto-reconnect on / off |

**Profiles (`e`):**

1. Press `e` to open the profile selection popup.
2. Use `j`/`k` to select an existing profile (indicated by `*` if it is currently active).
3. Press `Enter` to load the selected profile and switch your server list.
4. Press `n` to create a new profile.
5. Press `Esc` to cancel and return to the main server list.

**Adding / Editing a Server:**

| Key | Action |
|-----|--------|
| `Tab` / `Down` | Move to next field |
| `Shift+Tab` / `Up` | Move to previous field |
| `Enter` | Save and close |
| `Esc` | Cancel |

**Broadcast Command (`p`):**

1. Type the command to run, then press `Enter`.
2. Use `j`/`k` to move, `Space` to toggle server selection (highlighted in green when selected).
3. Press `Enter` to execute the command on all selected servers sequentially.
4. Press `Esc` at any step to cancel.

**File Transfer (`f`):**

1. Press `f` to open the transfer dialog for the selected server.
2. Choose the direction with `Space` (`Upload`: local → remote, `Download`: remote → local).
3. Fill in the local path and the remote path (`Tab` to switch fields). An empty remote path means `~`.
4. Optionally enable `recursive` to copy whole directories (`Space` on the options row).
5. Press `Enter` to start the transfer, `Esc` to cancel.

The dialog shows the exact command that will be executed.

> `scp` is used first. If `scp` is missing from `PATH`, or the transfer fails (for example
> against a server that only accepts the SFTP protocol), `sshx` automatically repeats the
> transfer over SFTP.

> Note: on Windows, local paths such as `C:\Users\me\file.txt` are automatically rewritten to
> `C:/Users/me/file.txt` so that `scp` treats them as local paths instead of `host:path` specs.

**Auto Reconnect:**

- SSH, SFTP and file transfer sessions are started with keepalive options, so a dropped
  connection is detected within ~45 seconds instead of hanging forever.
- When the connection drops, `sshx` waits 10 seconds and reconnects, up to **3 attempts**.
  If all of them fail, `sshx` gives up and returns to the TUI.
- Press `Enter` (or `Esc` / `q`) during the countdown to abort the reconnect immediately.
- A clean exit (typing `exit` in the remote shell) never triggers a reconnect.
- `Ctrl+C` inside a session only ends that session — `sshx` stays alive and returns to the TUI.
- Press `r` in the main list to turn the feature off; the setting is remembered in `state.json`.

**Delete Confirmation:**

| Key | Action |
|-----|--------|
| `y` | Confirm deletion |
| `n` / `Esc` | Cancel |

## Configuration

Server configurations are stored in your user configuration directory. Each profile is a `.json` file:

- **Linux/macOS**: `~/.config/sshx/*.json`
- **Windows**: `%APPDATA%\sshx\*.json`

By default, servers are stored in `servers.json`. The application state (last connected server, last profile used) is stored in `state.json`.

### Jump Host (Bastion Server)

When adding or editing a server, you can specify a jump host to connect through a bastion server:

- `user@host` — jump host with default port (22)
- `user@host:port` — jump host with custom port

Leave the field empty for direct connections.

> Note: Mosh does not support jump host connections. Use SSH (`Enter`) instead.

## License

`sshx` is released under the **GNU General Public License v3.0** — see [LICENSE](LICENSE).

This project is a fork of [CGH0S7/sshx](https://github.com/CGH0S7/sshx), also licensed under GPL-3.0.
