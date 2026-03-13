// Daemon 模块

use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::process;
use tokio::signal;
use crate::cli::{DaemonCommand, DaemonAction};
use crate::database;
use crate::services::proxy::ProxyServer;
use crate::services::HealthCheckService;
use crate::database::dao::ProviderDao;
use crate::models::AppType;

/// Daemon 服务
pub struct DaemonService {
    pid_file: PathBuf,
    log_file: PathBuf,
}

impl DaemonService {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        let mrouter_dir = home.join(".mrouter");
        
        fs::create_dir_all(&mrouter_dir)?;
        
        Ok(Self {
            pid_file: mrouter_dir.join("daemon.pid"),
            log_file: mrouter_dir.join("daemon.log"),
        })
    }
    
    /// 启动 Daemon
    pub async fn start(&self, auto_start: bool) -> Result<()> {
        // 检查是否已经在运行
        if self.is_running()? {
            eprintln!("Daemon is already running");
            return Ok(());
        }

        // 写入 PID 文件
        let pid = process::id();
        fs::write(&self.pid_file, pid.to_string())?;

        eprintln!("Starting daemon (PID: {})...", pid);

        if auto_start {
            self.enable_auto_start()?;
            eprintln!("Auto-start enabled");
        }

        // 初始化数据库和配置
        let db = database::init().await?;
        let config = crate::config::AppConfig::load()?;
        let proxy_bind = config.proxy.bind.clone();
        let proxy_port = config.proxy.port;

        // 启动各个服务
        let proxy_handle = {
            let db_clone = db.clone();
            let bind = proxy_bind.clone();
            tokio::spawn(async move {
                let proxy = ProxyServer::new(db_clone, bind, proxy_port);
                if let Err(e) = proxy.start().await {
                    tracing::error!("Proxy server error: {}", e);
                }
            })
        };

        let health_check_handle = {
            let db_clone = db.clone();
            tokio::spawn(async move {
                let health_service = HealthCheckService::new(db_clone.clone());

                loop {
                    // 每 5 分钟执行一次健康检查
                    tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;

                    for app_type in AppType::all() {
                        if let Ok(providers) = ProviderDao::get_all(&db_clone, app_type) {
                            let active_providers: Vec<_> = providers.into_iter().filter(|p| p.is_active).collect();
                            if let Err(e) = health_service.check_all_providers(&active_providers).await {
                                tracing::error!("Health check error: {}", e);
                            }
                        }
                    }
                }
            })
        };

        // 使用 eprintln! 输出到 stderr（从 TUI 启动时会被重定向到 null）
        // 使用 tracing::info! 输出到日志文件
        eprintln!("Daemon started successfully");
        eprintln!("  - Proxy server: http://{}:{}", proxy_bind, proxy_port);
        eprintln!("  - Health check: every 5 minutes");
        eprintln!("  - PID file: {:?}", self.pid_file);
        eprintln!("  - Log file: {:?}", self.log_file);

        tracing::info!("Daemon started successfully (PID: {})", pid);
        tracing::info!("Proxy server listening on http://{}:{}", proxy_bind, proxy_port);
        tracing::info!("Health check interval: 5 minutes");
        tracing::info!("PID file: {:?}", self.pid_file);
        tracing::info!("Log file: {:?}", self.log_file);
        
        // 等待信号
        self.wait_for_signal().await?;

        // 清理
        proxy_handle.abort();
        health_check_handle.abort();
        self.cleanup()?;

        eprintln!("Daemon stopped");
        tracing::info!("Daemon stopped");

        Ok(())
    }

    /// 停止 Daemon
    pub fn stop(&self) -> Result<()> {
        if !self.is_running()? {
            eprintln!("Daemon is not running");
            return Ok(());
        }

        let pid_str = fs::read_to_string(&self.pid_file)?;
        let pid: u32 = pid_str.trim().parse()?;

        eprintln!("Stopping daemon (PID: {})...", pid);
        tracing::info!("Stopping daemon (PID: {})", pid);

        // 发送 SIGTERM 信号
        #[cfg(unix)]
        {
            use std::process::Command;
            Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .output()?;
        }

        #[cfg(windows)]
        {
            // 使用 taskkill 终止进程
            use std::process::Command;
            let result = Command::new("taskkill")
                .arg("/PID")
                .arg(pid.to_string())
                .arg("/F")
                .output();

            match result {
                Ok(o) if o.status.success() => {
                    eprintln!("Process {} terminated", pid);
                }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    eprintln!("Warning: taskkill may have failed: {}", stderr);
                }
                Err(e) => {
                    eprintln!("Failed to kill process {}: {}", pid, e);
                }
            }
        }

        // 删除 PID 文件
        fs::remove_file(&self.pid_file)?;

        eprintln!("Daemon stopped");
        tracing::info!("Daemon stopped successfully");

        Ok(())
    }

    /// 重启 Daemon
    pub async fn restart(&self) -> Result<()> {
        eprintln!("Restarting daemon...");
        tracing::info!("Restarting daemon");
        self.stop()?;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        self.start(false).await?;
        Ok(())
    }
    
    /// 显示 Daemon 状态
    pub fn status(&self) -> Result<()> {
        if self.is_running()? {
            let pid_str = fs::read_to_string(&self.pid_file)?;
            let pid = pid_str.trim();
            eprintln!("Daemon is running (PID: {})", pid);
            eprintln!("  - PID file: {:?}", self.pid_file);
            eprintln!("  - Log file: {:?}", self.log_file);
        } else {
            eprintln!("Daemon is not running");
        }
        Ok(())
    }

    /// 显示日志
    pub fn logs(&self, lines: usize) -> Result<()> {
        if !self.log_file.exists() {
            eprintln!("No logs available");
            return Ok(());
        }

        let content = fs::read_to_string(&self.log_file)?;
        let log_lines: Vec<&str> = content.lines().collect();

        let start = if log_lines.len() > lines {
            log_lines.len() - lines
        } else {
            0
        };

        for line in &log_lines[start..] {
            eprintln!("{}", line);
        }

        Ok(())
    }
    
    /// 检查 Daemon 是否在运行
    fn is_running(&self) -> Result<bool> {
        if !self.pid_file.exists() {
            return Ok(false);
        }

        let pid_str = fs::read_to_string(&self.pid_file)?;
        let pid: u32 = pid_str.trim().parse()?;

        // 检查进程是否存在
        #[cfg(unix)]
        {
            use std::process::Command;
            let output = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output();

            match output {
                Ok(o) if o.status.success() => return Ok(true),
                _ => {
                    // 进程不存在，删除 PID 文件
                    fs::remove_file(&self.pid_file).ok();
                    return Ok(false);
                }
            }
        }

        #[cfg(windows)]
        {
            // 使用 tasklist 检查进程是否存在
            use std::process::Command;
            let output = Command::new("tasklist")
                .arg("/FI")
                .arg(format!("PID eq {}", pid))
                .arg("/NH")
                .output();

            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    if stdout.contains(&pid.to_string()) {
                        return Ok(true);
                    } else {
                        // 进程不存在，删除 stale PID 文件
                        fs::remove_file(&self.pid_file).ok();
                        return Ok(false);
                    }
                }
                _ => {
                    // tasklist 失败，删除 PID 文件
                    fs::remove_file(&self.pid_file).ok();
                    return Ok(false);
                }
            }
        }
    }
    
    /// 等待信号
    async fn wait_for_signal(&self) -> Result<()> {
        #[cfg(unix)]
        {
            signal::ctrl_c().await?;
        }
        
        #[cfg(windows)]
        {
            signal::ctrl_c().await?;
        }
        
        Ok(())
    }
    
    /// 清理资源
    fn cleanup(&self) -> Result<()> {
        if self.pid_file.exists() {
            fs::remove_file(&self.pid_file)?;
        }
        Ok(())
    }
    
    /// 启用自动启动
    fn enable_auto_start(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            self.create_launchd_plist()?;
        }
        
        #[cfg(target_os = "linux")]
        {
            self.create_systemd_service()?;
        }
        
        #[cfg(windows)]
        {
            eprintln!("Auto-start on Windows not implemented yet");
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn create_launchd_plist(&self) -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        let plist_dir = home.join("Library").join("LaunchAgents");
        fs::create_dir_all(&plist_dir)?;

        let plist_path = plist_dir.join("com.mrouter.daemon.plist");
        let exe_path = std::env::current_exe()?;

        let plist_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.mrouter.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>daemon</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>"#, exe_path.display());

        fs::write(&plist_path, plist_content)?;
        eprintln!("Created launchd plist: {:?}", plist_path);
        tracing::info!("Created launchd plist: {:?}", plist_path);

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn create_systemd_service(&self) -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        let systemd_dir = home.join(".config").join("systemd").join("user");
        fs::create_dir_all(&systemd_dir)?;

        let service_path = systemd_dir.join("mrouter.service");
        let exe_path = std::env::current_exe()?;

        let service_content = format!(r#"[Unit]
Description=MRouter Daemon
After=network.target

[Service]
Type=simple
ExecStart={} daemon start
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target"#, exe_path.display());

        fs::write(&service_path, service_content)?;
        eprintln!("Created systemd service: {:?}", service_path);
        eprintln!("Run 'systemctl --user enable mrouter' to enable auto-start");
        tracing::info!("Created systemd service: {:?}", service_path);

        Ok(())
    }
}

/// 处理 Daemon 命令
pub async fn handle_command(cmd: DaemonCommand) -> Result<()> {
    let daemon = DaemonService::new()?;
    
    match cmd.action {
        DaemonAction::Start { auto_start } => {
            daemon.start(auto_start).await?;
        }
        DaemonAction::Stop => {
            daemon.stop()?;
        }
        DaemonAction::Restart => {
            daemon.restart().await?;
        }
        DaemonAction::Status => {
            daemon.status()?;
        }
        DaemonAction::Logs => {
            daemon.logs(50)?;
        }
    }
    
    Ok(())
}
