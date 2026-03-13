// 工具函数模块

use anyhow::Result;
use std::path::PathBuf;

/// 获取配置目录
pub fn get_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
    Ok(home.join(".mrouter"))
}

/// 确保配置目录存在
pub fn ensure_config_dir() -> Result<PathBuf> {
    let dir = get_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
