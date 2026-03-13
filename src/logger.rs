// 日志管理模块 - 使用 tracing + 本地时间日志滚动

use anyhow::Result;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 初始化日志系统
pub fn init_logger(
    log_file: Option<&str>,
    log_level: &str,
    _max_size_mb: u64,
    _max_backups: usize,
) -> Result<()> {
    if let Some(log_file) = log_file {
        // 解析日志文件路径
        let log_path = if log_file.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&log_file[2..])
            } else {
                PathBuf::from(log_file)
            }
        } else {
            PathBuf::from(log_file)
        };

        // 确保日志目录存在
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // 获取日志目录和文件名（不带扩展名）
        let log_dir = log_path.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf();
        let log_filename = log_path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("mrouter")
            .to_string();

        // 使用本地时间生成日志文件名，例如 mrouter.2026-03-13
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let daily_file = log_dir.join(format!("{}.{}", log_filename, today));

        // 打开（或创建）今天的日志文件，追加模式
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&daily_file)?;

        // 创建符号链接 mrouter.log -> mrouter.{today}（相对路径）
        let symlink_path = log_dir.join(format!("{}.log", log_filename));
        let _ = fs::remove_file(&symlink_path);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let _ = symlink(
                format!("{}.{}", log_filename, today),
                &symlink_path,
            );
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            let _ = symlink_file(&daily_file, &symlink_path);
        }

        // 使用 LocalTimeWriter 包装，实现跨日自动切换文件
        let writer = LocalTimeWriter {
            log_dir,
            log_filename,
            current_date: std::sync::Mutex::new(today),
            current_file: std::sync::Mutex::new(file),
        };

        // 配置日志格式
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(log_level));

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().with_writer(writer).with_ansi(false))
            .init();

        tracing::info!(
            "Logger initialized: file={}, level={}",
            log_file,
            log_level,
        );
    } else {
        // 如果没有配置日志文件，禁用日志输出（避免破坏 TUI）
        tracing_subscriber::registry()
            .with(EnvFilter::new("off"))
            .init();
    }

    Ok(())
}

/// 基于本地时间的日志 Writer，跨日自动切换文件
struct LocalTimeWriter {
    log_dir: PathBuf,
    log_filename: String,
    current_date: std::sync::Mutex<String>,
    current_file: std::sync::Mutex<std::fs::File>,
}

impl LocalTimeWriter {
    /// 检查是否跨日，如果是则切换到新文件
    fn ensure_current_file(&self) -> std::io::Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let mut current_date = self.current_date.lock().unwrap();
        if *current_date == today {
            return Ok(());
        }

        // 跨日：创建新文件
        let daily_file = self.log_dir.join(format!("{}.{}", self.log_filename, today));
        let new_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&daily_file)?;

        // 更新符号链接
        let symlink_path = self.log_dir.join(format!("{}.log", self.log_filename));
        let _ = fs::remove_file(&symlink_path);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let _ = symlink(
                format!("{}.{}", self.log_filename, today),
                &symlink_path,
            );
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            let _ = symlink_file(&daily_file, &symlink_path);
        }

        *current_date = today;
        let mut current_file = self.current_file.lock().unwrap();
        *current_file = new_file;

        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LocalTimeWriter {
    type Writer = LocalTimeFile<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        let _ = self.ensure_current_file();
        LocalTimeFile { inner: self }
    }
}

/// MakeWriter 返回的 Writer，持有对 LocalTimeWriter 的引用
struct LocalTimeFile<'a> {
    inner: &'a LocalTimeWriter,
}

impl<'a> std::io::Write for LocalTimeFile<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self.inner.current_file.lock().unwrap();
        file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self.inner.current_file.lock().unwrap();
        file.flush()
    }
}
