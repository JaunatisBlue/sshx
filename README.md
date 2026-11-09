# sshx

A terminal-based SSH connection manager written in Rust. `sshx` gives you a TUI to
store, organize and jump to your servers — built with [ratatui](https://ratatui.rs).

## Features

- **One-key connections** — `ssh`, `sftp`, `mosh` and file transfer (`scp`, with an automatic
  SFTP fallback) straight from the server list.
- **Auto-reconnect** — keepalive detects dead links; `sshx` retries 3 times, 10s apart, then
  returns to the TUI. `Ctrl+C` only ends the session, never the app.
- **Profiles** — keep servers in separate lists (lab / home / cloud), switch or create them
  on the fly.
- **Broadcast** — run one command on any number of selected servers.
- **Jump hosts** — connect through a bastion via SSH `-J`.
- **Key management** — `ssh-copy-id`, with a built-in fallback on Windows.
- **Vim-style navigation** — `j`/`k`, `gg`, `G`; the last connected server floats to the top.
- **Windows-first** — console handling that keeps `sftp`/`scp` input working and your shell's
  echo intact, plus path normalization for `scp`.

## Installation

### Windows

1. Install the **OpenSSH Client**: *Settings → System → Optional features → View features →
   OpenSSH Client* (Windows 10 1809+ can also install it from the Microsoft Store).
2. Go to the [Releases page](https://github.com/JaunatisBlue/sshx/releases) and download
   `sshx-windows-amd64-<version>.exe` — it runs as-is, no unzipping needed. The `.zip` bundle
   (binary + README + LICENSE) is there too if you prefer a folder.
3. Double-click the `.exe`.

No installation or admin rights needed — configuration lives in `%APPDATA%\sshx`.

### Linux / macOS

```bash
git clone https://github.com/JaunatisBlue/sshx.git
cd sshx
cargo build --release
```

The binary is at `target/release/sshx`.

### Prerequisites

`ssh` (Windows: OpenSSH Client, as above), `scp`/`sftp` for transfers, `mosh` and
`ssh-copy-id` are optional.

## Usage

Run `sshx` (from `PATH`) or `cargo run`.

### Key bindings

| Key | Action |
|-----|--------|
| `j` / `Down` / `Tab` | Select next server |
| `k` / `Up` / `Shift+Tab` | Select previous server |
| `gg` / `G` | Jump to first / last server |
| `q` / `Esc` / `Ctrl+C` / `Ctrl+D` | Quit |

| Key | Action |
|-----|--------|
| `Enter` | Connect via `ssh` |
| `s` | Connect via `sftp` |
| `f` | Transfer files (`scp`) |
| `m` | Connect via `mosh` |
| `p` | Broadcast a command to multiple servers |
| `e` | Switch / create profile |
| `n` / `i` / `d` | Add / edit / delete server |
| `c` | Copy your public key (`ssh-copy-id`) |
| `r` | Toggle auto-reconnect |

### Dialogs

- **Profiles (`e`)** — `j`/`k` to pick (the active one is marked `●`), `Enter` to load, `n` for a
  new profile, `Esc` to cancel.
- **Add / edit** — `Tab` / `Shift+Tab` between fields, `Enter` to save, `Esc` to cancel.
- **Delete (`d`)** — `y` to confirm, `n` / `Esc` to cancel.
- **Broadcast (`p`)** — type the command, `Space` to toggle servers, `Enter` to run it on all
  of them, `Esc` to cancel.
- **File transfer (`f`)** — `Space` switches upload/download, `Tab` moves between the local and
  remote path, `Space` on the options row toggles recursive copies, `Enter` starts the
  transfer. The dialog shows the exact command that will run.

### Auto-reconnect

Sessions are opened with keepalive options, so a dropped connection is noticed within ~45s
instead of hanging. `sshx` then waits 10s and reconnects, up to 3 attempts, before giving up and
returning to the TUI. Press `Enter` (or `Esc` / `q`) during the countdown to abort right away.
A clean `exit` never triggers a reconnect. Press `r` in the main list to disable the feature;
the setting is remembered in `state.json`.

## Configuration

Each profile is a `.json` file in your config directory:

- **Linux/macOS**: `~/.config/sshx/*.json`
- **Windows**: `%APPDATA%\sshx\*.json`

Servers live in `servers.json` by default; `state.json` keeps the last connected server, the
current profile and the auto-reconnect setting.

### Jump host (bastion)

The jump host field takes `user@host` or `user@host:port`; leave it empty for a direct
connection. Mosh does not support jump hosts — use `ssh` instead.

## License

`sshx` is released under the **GNU General Public License v3.0** — see [LICENSE](LICENSE).

This project is a fork of [CGH0S7/sshx](https://github.com/CGH0S7/sshx), also licensed under GPL-3.0.
