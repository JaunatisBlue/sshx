use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Server {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub jump_host: String, // 跳板机，格式: user@host:port 或 user@host
}

impl Server {
    pub fn address(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }

    /// 跳转主机参数（若配置了跳板机）
    fn jump_args(&self) -> Vec<String> {
        if self.jump_host.is_empty() {
            Vec::new()
        } else {
            vec!["-J".to_string(), self.jump_host.clone()]
        }
    }

    /// 保活参数：让 ssh 能及时发现已经断开的连接
    fn keepalive_args() -> Vec<String> {
        vec![
            "-o".to_string(),
            "ServerAliveInterval=15".to_string(),
            "-o".to_string(),
            "ServerAliveCountMax=3".to_string(),
            "-o".to_string(),
            "TCPKeepAlive=yes".to_string(),
        ]
    }

    /// ssh/scp/sftp 共用的选项部分。
    /// 注意：Windows 自带的 OpenSSH 使用 BSD 风格的参数解析，选项必须全部放在目标地址之前，
    /// 否则 `ssh user@host -p 2222` 里的 `-p 2222` 会被当成要执行的远程命令。
    fn option_args(&self, port_flag: &str) -> Vec<String> {
        let mut args = Vec::new();
        args.extend(self.jump_args());
        if !self.port.is_empty() {
            args.push(port_flag.to_string());
            args.push(self.port.clone());
        }
        args.extend(Self::keepalive_args());
        args
    }

    pub fn to_ssh_args(&self) -> Vec<String> {
        let mut args = self.option_args("-p");
        args.push(self.address());
        args
    }

    pub fn to_mosh_args(&self) -> Vec<String> {
        let mut args = vec![];
        let mut ssh_opts = String::new();
        if !self.port.is_empty() {
            ssh_opts.push_str(&format!("-p {}", self.port));
        }
        if !self.jump_host.is_empty() {
            if !ssh_opts.is_empty() {
                ssh_opts.push(' ');
            }
            ssh_opts.push_str(&format!("-J {}", self.jump_host));
        }
        if !ssh_opts.is_empty() {
            args.push("--ssh".to_string());
            args.push(format!("ssh {}", ssh_opts));
        }
        args.push(self.address());
        args
    }

    pub fn to_copy_id_args(&self) -> Vec<String> {
        let mut args = vec![];
        // 添加跳板机参数
        if !self.jump_host.is_empty() {
            args.push("-o".to_string());
            args.push(format!("ProxyJump={}", self.jump_host));
        }
        if !self.port.is_empty() {
            args.push("-p".to_string());
            args.push(self.port.clone());
        }
        args.push(self.address());
        args
    }

    pub fn to_sftp_args(&self) -> Vec<String> {
        let mut args = self.option_args("-P");
        args.push(self.address());
        args
    }

    /// scp 上传：本地 -> 远端
    pub fn to_scp_upload_args(
        &self,
        local_path: &str,
        remote_path: &str,
        recursive: bool,
    ) -> Vec<String> {
        let mut args = self.option_args("-P");
        if recursive {
            args.push("-r".to_string());
        }
        args.push(normalize_local_path(local_path));
        args.push(self.remote_spec(remote_path));
        args
    }

    /// scp 下载：远端 -> 本地
    pub fn to_scp_download_args(
        &self,
        remote_path: &str,
        local_path: &str,
        recursive: bool,
    ) -> Vec<String> {
        let mut args = self.option_args("-P");
        if recursive {
            args.push("-r".to_string());
        }
        args.push(self.remote_spec(remote_path));
        args.push(normalize_local_path(local_path));
        args
    }

    /// 生成 `user@host:path` 形式的远端路径。
    /// 用户若已经填写了完整形式（含冒号）则原样使用。
    pub fn remote_spec(&self, remote_path: &str) -> String {
        if remote_path.contains(':') {
            return remote_path.to_string();
        }
        let path = if remote_path.is_empty() {
            "~"
        } else {
            remote_path
        };
        format!("{}:{}", self.address(), path)
    }
}

/// 把 Windows 本地路径转换成 scp 能识别的形式。
///
/// Windows 上 `C:\Users\me\a.txt` 里的反斜杠会被 scp 当作转义字符，
/// 而 `C:/Users/...` 才是 Windows OpenSSH 能正确识别的写法。
pub fn normalize_local_path(path: &str) -> String {
    let trimmed = path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return String::new();
    }
    let replaced = trimmed.replace('\\', "/");
    // 盘符开头的路径保持 `C:/...`，scp 会识别为本地路径
    if replaced.len() >= 2
        && replaced.as_bytes()[1] == b':'
        && (replaced.as_bytes()[0] as char).is_ascii_alphabetic()
    {
        return replaced;
    }
    // 其它情况转成 ./ 前缀，避免被误认为 host:path
    if replaced.starts_with('/') {
        replaced
    } else {
        format!("./{}", replaced)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            name: "test".to_string(),
            user: "root".to_string(),
            host: "1.2.3.4".to_string(),
            port: "2222".to_string(),
            jump_host: String::new(),
        }
    }

    #[test]
    fn ssh_options_precede_destination() {
        let args = server().to_ssh_args();
        let dest = args.iter().position(|a| a == "root@1.2.3.4").unwrap();
        let port = args.iter().position(|a| a == "2222").unwrap();
        assert!(port < dest, "port option must come before destination");
    }

    #[test]
    fn sftp_options_precede_destination() {
        let args = server().to_sftp_args();
        let dest = args.iter().position(|a| a == "root@1.2.3.4").unwrap();
        let port = args.iter().position(|a| a == "2222").unwrap();
        assert!(port < dest, "port option must come before destination");
    }

    #[test]
    fn jump_host_precedes_destination() {
        let mut s = server();
        s.jump_host = "bastion@jump:2200".to_string();
        let args = s.to_ssh_args();
        let dest = args.iter().position(|a| a == "root@1.2.3.4").unwrap();
        let jump = args.iter().position(|a| a == "-J").unwrap();
        assert!(jump < dest);
    }

    #[test]
    fn windows_local_path_is_normalized() {
        assert_eq!(
            normalize_local_path("C:\\Users\\me\\a.txt"),
            "C:/Users/me/a.txt"
        );
        assert_eq!(normalize_local_path("a.txt"), "./a.txt");
        assert_eq!(normalize_local_path("/tmp/a.txt"), "/tmp/a.txt");
    }

    #[test]
    fn remote_spec_defaults_to_home() {
        assert_eq!(server().remote_spec(""), "root@1.2.3.4:~");
        assert_eq!(server().remote_spec("/var/log"), "root@1.2.3.4:/var/log");
        assert_eq!(server().remote_spec("root@other:/tmp"), "root@other:/tmp");
    }
}
