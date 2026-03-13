// Skills 服务 - 仓库克隆与 Skill 扫描

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::database::Database;
use crate::database::dao::{SkillDao, SkillRepoDao};
use crate::models::{Skill, SkillRepo};

pub struct SkillService;

impl SkillService {
    /// 获取 skills 存储根目录
    fn skills_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        let dir = home.join(".mrouter").join("skills");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// 添加并克隆一个 skill 仓库
    pub fn add_repo(db: &Database, name: &str, url: &str, branch: &str) -> Result<SkillRepo> {
        let skills_dir = Self::skills_dir()?;
        let local_path = skills_dir.join(name);

        // 克隆仓库
        if local_path.exists() {
            // 已存在则 pull
            Self::git_pull(&local_path, branch)?;
        } else {
            Self::git_clone(url, &local_path, branch)?;
        }

        let mut repo = SkillRepo::new(name.to_string(), url.to_string());
        repo.branch = branch.to_string();
        repo.local_path = local_path.display().to_string();
        repo.last_synced = Some(chrono::Utc::now().to_rfc3339());

        let id = SkillRepoDao::insert(db, &repo)?;
        repo.id = id;

        // 扫描并注册 skills
        Self::scan_and_register(db, &repo)?;

        Ok(repo)
    }

    /// 同步（pull）一个已有仓库并重新扫描
    pub fn sync_repo(db: &Database, repo: &SkillRepo) -> Result<usize> {
        let local_path = PathBuf::from(&repo.local_path);

        if !local_path.exists() {
            Self::git_clone(&repo.url, &local_path, &repo.branch)?;
        } else {
            Self::git_pull(&local_path, &repo.branch)?;
        }

        SkillRepoDao::update_synced(db, repo.id)?;

        // 删除旧 skills 并重新扫描
        SkillDao::delete_by_repo(db, repo.id)?;
        let count = Self::scan_and_register(db, repo)?;

        Ok(count)
    }

    /// 同步所有仓库
    pub fn sync_all(db: &Database) -> Result<usize> {
        let repos = SkillRepoDao::get_all(db)?;
        let mut total = 0;
        for repo in &repos {
            match Self::sync_repo(db, repo) {
                Ok(count) => total += count,
                Err(e) => tracing::warn!("Failed to sync repo {}: {}", repo.name, e),
            }
        }
        Ok(total)
    }

    /// 删除仓库及其所有 skills
    pub fn remove_repo(db: &Database, repo_id: i64) -> Result<()> {
        if let Some(repo) = SkillRepoDao::get_by_id(db, repo_id)? {
            // 删除本地文件
            let local_path = PathBuf::from(&repo.local_path);
            if local_path.exists() {
                std::fs::remove_dir_all(&local_path)?;
            }
        }

        SkillRepoDao::delete(db, repo_id)?;
        Ok(())
    }

    /// 扫描仓库目录，查找 skill 定义文件并注册
    fn scan_and_register(db: &Database, repo: &SkillRepo) -> Result<usize> {
        let local_path = PathBuf::from(&repo.local_path);
        if !local_path.exists() {
            return Ok(0);
        }

        let mut count = 0;

        // 扫描策略：
        // 1. 查找 *.md 文件作为 skill（Claude Code 风格）
        // 2. 查找包含 skill 定义的目录
        Self::scan_dir(db, repo.id, &local_path, &local_path, &mut count)?;

        Ok(count)
    }

    /// 递归扫描目录查找 skill 文件
    fn scan_dir(db: &Database, repo_id: i64, base: &Path, dir: &Path, count: &mut usize) -> Result<()> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // 跳过隐藏目录和 node_modules
            if file_name.starts_with('.') || file_name == "node_modules" || file_name == "target" {
                continue;
            }

            if path.is_dir() {
                // 检查是否是一个 skill 目录（包含 README.md 或 skill.toml）
                let readme = path.join("README.md");
                let skill_toml = path.join("skill.toml");

                if readme.exists() || skill_toml.exists() {
                    let rel_path = path.strip_prefix(base)
                        .unwrap_or(&path)
                        .display()
                        .to_string();

                    let description = if skill_toml.exists() {
                        Self::read_skill_description(&skill_toml)
                    } else if readme.exists() {
                        Self::read_first_line(&readme)
                    } else {
                        None
                    };

                    let mut skill = Skill::new(file_name.clone(), repo_id, rel_path);
                    skill.description = description;
                    SkillDao::insert(db, &skill)?;
                    *count += 1;
                } else {
                    // 递归扫描子目录（最多 3 层深度）
                    let depth = path.strip_prefix(base).map(|p| p.components().count()).unwrap_or(0);
                    if depth < 3 {
                        Self::scan_dir(db, repo_id, base, &path, count)?;
                    }
                }
            } else if path.extension().map(|e| e == "md").unwrap_or(false)
                && file_name != "README.md"
                && file_name != "CHANGELOG.md"
            {
                // 单文件 skill（.md 文件）
                let rel_path = path.strip_prefix(base)
                    .unwrap_or(&path)
                    .display()
                    .to_string();

                let name = file_name.trim_end_matches(".md").to_string();
                let description = Self::read_first_line(&path);

                let mut skill = Skill::new(name, repo_id, rel_path);
                skill.description = description;
                SkillDao::insert(db, &skill)?;
                *count += 1;
            }
        }

        Ok(())
    }

    /// 读取 skill.toml 中的 description
    fn read_skill_description(path: &Path) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;
        // 简单解析 description = "..."
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("description") {
                if let Some(val) = trimmed.split('=').nth(1) {
                    let val = val.trim().trim_matches('"').trim_matches('\'');
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
        None
    }

    /// 读取文件第一行非空内容作为描述
    fn read_first_line(path: &Path) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;
        for line in content.lines() {
            let trimmed = line.trim().trim_start_matches('#').trim();
            if !trimmed.is_empty() {
                return Some(trimmed.chars().take(100).collect());
            }
        }
        None
    }

    fn git_clone(url: &str, dest: &Path, branch: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["clone", "--depth", "1", "--branch", branch, url])
            .arg(dest)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git clone failed: {}", stderr);
        }

        Ok(())
    }

    fn git_pull(dir: &Path, branch: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["pull", "origin", branch])
            .current_dir(dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("git pull warning: {}", stderr);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skills_dir() {
        let dir = SkillService::skills_dir().unwrap();
        assert!(dir.to_string_lossy().contains(".mrouter/skills"));
    }
}
