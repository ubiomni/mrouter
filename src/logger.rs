// 日志管理模块 - 使用 tracing + 本地时间日志滚动
//
// 支持：
// - 按日期自动滚动（mrouter.2026-03-13）
// - 按文件大小滚动（mrouter.2026-03-13.1, .2, ...）
// - 自动清理旧日志（max_backups > 0 时生效，0 = 不清理）

use anyhow::Result;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 初始化日志系统
pub fn init_logger(
    log_file: Option<&str>,
    log_level: &str,
    stderr: bool,
    max_size_mb: u64,
    max_backups: usize,
) -> Result<()> {
    // 没有文件也没有 stderr，禁用日志
    if log_file.is_none() && !stderr {
        tracing_subscriber::registry()
            .with(EnvFilter::new("off"))
            .init();
        return Ok(());
    }

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    let timer = fmt::time::OffsetTime::local_rfc_3339()
        .unwrap_or_else(|_| fmt::time::OffsetTime::new(
            time::UtcOffset::from_hms(8, 0, 0).unwrap(),
            time::format_description::well_known::Rfc3339,
        ));

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
        update_symlink(&log_dir, &log_filename, &today);

        // 启动时清理旧日志
        if max_backups > 0 {
            cleanup_old_logs(&log_dir, &log_filename, max_backups);
        }

        // 使用 LocalTimeWriter 包装，实现跨日自动切换 + 大小滚动
        let writer = LocalTimeWriter {
            log_dir,
            log_filename,
            max_size_bytes: max_size_mb * 1024 * 1024,
            max_backups,
            current_date: std::sync::Mutex::new(today),
            current_file: std::sync::Mutex::new(file),
            current_seq: std::sync::Mutex::new(0),
        };

        let file_layer = fmt::layer().with_writer(writer).with_ansi(false).with_timer(timer.clone());

        if stderr {
            // 文件 + stderr 双输出
            let stderr_layer = fmt::layer().with_writer(std::io::stderr).with_timer(timer);
            tracing_subscriber::registry()
                .with(env_filter)
                .with(file_layer)
                .with(stderr_layer)
                .init();
        } else {
            // 仅文件
            tracing_subscriber::registry()
                .with(env_filter)
                .with(file_layer)
                .init();
        }

        tracing::info!(
            "Logger initialized: file={}, stderr={}, level={}, max_size={}MB, max_backups={}{}",
            log_file,
            stderr,
            log_level,
            max_size_mb,
            max_backups,
            if max_backups == 0 { " (auto-cleanup disabled)" } else { "" },
        );
    } else {
        // 仅 stderr（无文件日志，适合 K8s 多副本部署）
        let stderr_layer = fmt::layer().with_writer(std::io::stderr).with_timer(timer);
        tracing_subscriber::registry()
            .with(env_filter)
            .with(stderr_layer)
            .init();

        tracing::info!("Logger initialized: stderr only, level={}", log_level);
    }

    Ok(())
}

/// 更新符号链接指向当前日志文件
fn update_symlink(log_dir: &PathBuf, log_filename: &str, date: &str) {
    let symlink_path = log_dir.join(format!("{}.log", log_filename));
    let _ = fs::remove_file(&symlink_path);

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink(
            format!("{}.{}", log_filename, date),
            &symlink_path,
        );
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_file;
        let target = log_dir.join(format!("{}.{}", log_filename, date));
        let _ = symlink_file(&target, &symlink_path);
    }
}

/// 清理旧日志文件，保留最近 max_backups 天的日志
fn cleanup_old_logs(log_dir: &PathBuf, log_filename: &str, max_backups: usize) {
    let prefix = format!("{}.", log_filename);
    // 日期格式正则：YYYY-MM-DD，可能带 .N 后缀（大小滚动产生的）
    let date_re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}").unwrap();

    let entries = match fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    // 收集所有日志日期
    let mut dates: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut all_log_files: Vec<(String, PathBuf)> = Vec::new(); // (date, path)

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(suffix) = name.strip_prefix(&prefix) {
            if let Some(m) = date_re.find(suffix) {
                let date = m.as_str().to_string();
                dates.insert(date.clone());
                all_log_files.push((date, entry.path()));
            }
        }
    }

    // 保留最近 max_backups 天
    if dates.len() <= max_backups {
        return;
    }

    let dates_vec: Vec<String> = dates.into_iter().collect();
    let cutoff = dates_vec.len() - max_backups;
    let expired_dates: std::collections::HashSet<&str> = dates_vec[..cutoff]
        .iter()
        .map(|s| s.as_str())
        .collect();

    for (date, path) in &all_log_files {
        if expired_dates.contains(date.as_str()) {
            if let Err(e) = fs::remove_file(path) {
                eprintln!("Failed to cleanup old log {:?}: {}", path, e);
            }
        }
    }
}

/// 基于本地时间的日志 Writer，跨日自动切换 + 按大小滚动
struct LocalTimeWriter {
    log_dir: PathBuf,
    log_filename: String,
    /// 单个文件最大字节数（0 = 不限制）
    max_size_bytes: u64,
    /// 保留天数（0 = 不自动清理）
    max_backups: usize,
    current_date: std::sync::Mutex<String>,
    current_file: std::sync::Mutex<std::fs::File>,
    /// 当天的序号（0 = 主文件，1+ = 滚动文件）
    current_seq: std::sync::Mutex<u32>,
}

impl LocalTimeWriter {
    /// 检查是否需要切换文件（跨日或超大小）
    fn ensure_current_file(&self) -> std::io::Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let mut current_date = self.current_date.lock().unwrap();

        if *current_date != today {
            // 跨日：重置序号，创建新日期文件
            let daily_file = self.log_dir.join(format!("{}.{}", self.log_filename, today));
            let new_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&daily_file)?;

            update_symlink(&self.log_dir, &self.log_filename, &today);

            *current_date = today.clone();
            let mut current_file = self.current_file.lock().unwrap();
            *current_file = new_file;
            let mut seq = self.current_seq.lock().unwrap();
            *seq = 0;

            // 跨日时清理旧日志
            if self.max_backups > 0 {
                cleanup_old_logs(&self.log_dir, &self.log_filename, self.max_backups);
            }

            return Ok(());
        }

        // 检查文件大小
        if self.max_size_bytes > 0 {
            let current_file = self.current_file.lock().unwrap();
            let file_size = current_file.metadata().map(|m| m.len()).unwrap_or(0);
            drop(current_file);

            if file_size >= self.max_size_bytes {
                let mut seq = self.current_seq.lock().unwrap();
                *seq += 1;
                let rotated_name = format!("{}.{}.{}", self.log_filename, today, *seq);
                let rotated_path = self.log_dir.join(&rotated_name);

                let new_file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&rotated_path)?;

                let mut current_file = self.current_file.lock().unwrap();
                *current_file = new_file;
            }
        }

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
