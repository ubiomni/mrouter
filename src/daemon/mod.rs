// Daemon 模块

use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::process;
use tokio::signal;
use crate::cli::{DaemonCommand, DaemonAction};
use crate::database;
use crate::services::proxy::ProxyServer;
use crate::services::admin::AdminServer;
use crate::services::HealthCheckService;
use crate::database::dao::ProviderDao;

/// Daemon 服务
pub struct DaemonService {
    pid_file: PathBuf,
}

impl DaemonService {
    pub fn new() -> Result<Self> {
        let mrouter_dir = if let Ok(dir) = std::env::var("MROUTER_DATA_DIR") {
            PathBuf::from(dir)
        } else {
            let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
            home.join(".mrouter")
        };

        fs::create_dir_all(&mrouter_dir)?;

        Ok(Self {
            pid_file: mrouter_dir.join("daemon.pid"),
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
        let db = match database::init().await {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Database initialization failed: {}", e);
                let config = crate::config::AppConfig::load().ok();
                if let Some(cfg) = &config {
                    let driver = &cfg.database.driver;
                    let path = &cfg.database.path;
                    if driver == "mysql" {
                        eprintln!("Config: driver={}, url={}", driver, path.split('@').last().unwrap_or(path));
                        eprintln!("Please ensure MySQL is running and the connection URL in ~/.mrouter/config.toml is correct.");
                    } else {
                        eprintln!("Config: driver={}, path={}", driver, path);
                        eprintln!("Please ensure the database file path is writable.");
                    }
                }
                return Err(e);
            }
        };
        let config = crate::config::AppConfig::load()?;

        // Ensure super_admin has a secure password (replaces seed default on first run)
        ensure_super_admin(&db);

        // Migrate plaintext api_keys to encrypted format
        if let Err(e) = crate::database::dao::ProviderDao::migrate_encrypt_keys(&db) {
            tracing::warn!("Failed to migrate api_keys: {}", e);
        }

        // Remove duplicate provider_model entries
        if let Err(e) = crate::database::dao::ProviderModelDao::dedup(&db) {
            tracing::warn!("Failed to dedup provider_models: {}", e);
        }

        // NOTE: In endpoint-driven architecture, path templates and format headers
        // are stored per-provider in provider_endpoints table, not in global presets.
        // These caches are deprecated and will be removed in a future version.

        // DEPRECATED: 加载路径模板到进程内存缓存
        // if let Ok(templates) = crate::database::dao::ProviderPresetDao::load_path_templates(&db) {
        //     crate::services::proxy::path_cache::load_from_presets(&templates);
        // }

        // DEPRECATED: 加载协议级 headers 到进程内存缓存
        // if let Ok(rows) = crate::database::dao::ProviderPresetDao::load_format_headers(&db) {
        //     crate::services::proxy::format_header_cache::load_from_rows(&rows);
        // }

        let proxy_bind = config.proxy.bind.clone();
        let proxy_port = config.proxy.port;

        // 启动各个服务：先 bind 所有端口，写 ready 文件，再 serve
        let config = std::sync::Arc::new(config);
        let proxy = ProxyServer::new(db.clone(), proxy_bind.clone(), proxy_port);
        let proxy_state = proxy.state();

        let (proxy_listener, proxy_app) = proxy.bind().await?;
        tracing::info!("Proxy server bound on {}:{}", proxy_bind, proxy_port);

        // 启动 Admin API server
        let admin_bind = config.admin.bind.clone();
        let admin_port = config.admin.port;
        let admin_server = AdminServer::new(db.clone(), config.clone(), proxy_state);

        // 所有端口 bind 成功，写 ready 文件通知 TUI
        let ready_file = self.pid_file.parent().unwrap().join("daemon.ready");
        fs::write(&ready_file, pid.to_string())?;

        // 启动 serve（进入事件循环）
        let proxy_handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(proxy_listener, proxy_app).await {
                tracing::error!("Proxy server error: {}", e);
            }
        });

        let _admin_handle = tokio::spawn(async move {
            if let Err(e) = admin_server.start().await {
                tracing::error!("Admin API server error: {}", e);
            }
        });

        let health_check_handle = {
            let db_clone = db.clone();
            tokio::spawn(async move {
                let health_service = HealthCheckService::new(db_clone.clone());

                loop {
                    // 每 5 分钟执行一次健康检查
                    tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;

                    if let Ok(providers) = ProviderDao::get_all_active(&db_clone, 0) {
                        if let Err(e) = health_service.check_all_providers(&providers).await {
                            tracing::error!("Health check error: {}", e);
                        }
                    }
                }
            })
        };

        // 使用 eprintln! 输出到 stderr（从 TUI 启动时会被重定向到 null）
        // 使用 tracing::info! 输出到日志文件
        eprintln!("Daemon started successfully");
        eprintln!("  - Proxy server: http://{}:{}", proxy_bind, proxy_port);
        eprintln!("  - Admin API: http://{}:{}", admin_bind, admin_port);
        eprintln!("  - Health check: every 5 minutes");
        eprintln!("  - PID file: {:?}", self.pid_file);
        if let Some(ref log_file) = config.log.file {
            eprintln!("  - Log file: {:?}", log_file);
        }
        if config.log.stderr {
            eprintln!("  - Log stderr: enabled");
        }

        tracing::info!("Daemon started successfully (PID: {})", pid);
        tracing::info!("Proxy server listening on http://{}:{}", proxy_bind, proxy_port);
        tracing::info!("Admin API listening on http://{}:{}", admin_bind, admin_port);
        tracing::info!("Health check interval: 5 minutes");
        tracing::info!("PID file: {:?}", self.pid_file);
        if let Some(ref log_file) = config.log.file {
            tracing::info!("Log file: {:?}", log_file);
        }
        if config.log.stderr {
            tracing::info!("Log stderr: enabled");
        }

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

        // 删除 PID 文件和 ready 文件
        fs::remove_file(&self.pid_file)?;
        let ready_file = self.pid_file.parent().unwrap().join("daemon.ready");
        fs::remove_file(&ready_file).ok();

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
            if let Some(ref log_file) = crate::config::AppConfig::load().ok().and_then(|c| c.log.file) {
                eprintln!("  - Log file: {:?}", log_file);
            }
        } else {
            eprintln!("Daemon is not running");
        }
        Ok(())
    }

    /// 显示日志
    pub fn logs(&self, lines: usize) -> Result<()> {
        let log_file = crate::config::AppConfig::load()
            .ok()
            .and_then(|c| c.log.file)
            .ok_or_else(|| anyhow::anyhow!("No log file configured"))?;
        let log_path = PathBuf::from(&log_file);

        if !log_path.exists() {
            eprintln!("No logs available ({})", log_file);
            return Ok(());
        }

        let content = fs::read_to_string(&log_path)?;
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

        // 容器重启后 PID 可能复用（尤其是 PID 1），
        // 如果 PID 文件中的值等于当前进程，说明是自己刚启动，不算"已运行"
        if pid == process::id() {
            fs::remove_file(&self.pid_file).ok();
            return Ok(false);
        }

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
        let ready_file = self.pid_file.parent().unwrap().join("daemon.ready");
        if ready_file.exists() {
            fs::remove_file(&ready_file).ok();
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

/// Ensure the super_admin user exists with a secure password.
/// If not found, create one with a random password and print it.
fn ensure_super_admin(db: &crate::database::Database) {
    use crate::database::dao::{UserDao, OrganizationDao};
    use crate::models::{User, UserRole};

    // Check if super_admin already exists
    let existing = UserDao::get_by_username_any(db, "admin")
        .ok()
        .and_then(|users| users.into_iter().find(|u| u.role.to_string() == "super_admin"));

    if existing.is_some() {
        return; // Already exists, nothing to do
    }

    // Generate random password (format: xxxx-xxxx-xxxx-xxxx)
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars = b"abcdefghijkmnpqrstuvwxyz23456789";
    let password: String = (0..19)
        .map(|i| {
            if i == 4 || i == 9 || i == 14 {
                '-'
            } else {
                chars[rng.gen_range(0..chars.len())] as char
            }
        })
        .collect();

    // Hash password and create user
    match crate::services::admin::auth::hash_password(&password) {
        Ok(hash) => {
            let admin = User {
                id: 0,
                org_id: 0, // Super admin belongs to no org
                username: "admin".to_string(),
                password_hash: hash,
                display_name: Some("Administrator".to_string()),
                email: None,
                role: UserRole::SuperAdmin,
                provider_group: "default".to_string(),
                is_active: true,
                last_login_at: None,
                created_at: chrono::Utc::now().timestamp_millis(),
                updated_at: chrono::Utc::now().timestamp_millis(),
            };

            match UserDao::insert(db, &admin) {
                Ok(_) => {
                    tracing::warn!("╔═══════════════════════════════════════╗");
                    tracing::warn!("║  Super Admin Created                  ║");
                    tracing::warn!("║  Username: admin                      ║");
                    tracing::warn!("║  Password: {:28}║", password);
                    tracing::warn!("║  (Change after first login)           ║");
                    tracing::warn!("╚═══════════════════════════════════════╝");
                    eprintln!("\n  🔐 Super Admin Password: {}\n", password);
                }
                Err(e) => tracing::error!("Failed to create super_admin: {}", e),
            }
        }
        Err(e) => tracing::error!("Failed to hash password: {}", e),
    }
}
