// Provider 切换服务

use anyhow::Result;
use crate::database::Database;
use crate::database::dao::ProviderDao;
use crate::models::AppType;
use crate::services::ConfigSyncService;

pub struct ProviderSwitchService;

impl ProviderSwitchService {
    /// 切换到指定的 Provider
    ///
    /// 切换流程：
    /// 1. 验证目标 Provider 存在
    /// 2. 更新数据库中的 is_active 标记
    /// 3. **条件性同步**: 只同步到 sync_to_cli_tools 列表中的 CLI Tool
    ///
    /// **新行为**: 默认情况下，Provider 使用 LLM Gateway Router 模式（sync_to_cli_tools=[]）
    /// 只有当用户明确添加 CLI Tool 到 sync_to_cli_tools 时，才会同步配置到对应的 CLI Tool 文件
    pub fn switch_provider(db: &Database, app_type: AppType, provider_id: i64) -> Result<()> {
        // 获取所有 Provider
        let providers = ProviderDao::get_all(db, app_type)?;

        // 查找目标 Provider
        let target_provider = providers
            .iter()
            .find(|p| p.id == provider_id)
            .ok_or_else(|| anyhow::anyhow!("Provider with id {} not found", provider_id))?;

        // 更新数据库中的 is_active 标记
        ProviderDao::set_active(db, app_type, provider_id)?;

        // 条件性同步：只同步到 sync_to_cli_tools 列表中的 CLI Tool
        if target_provider.should_sync_to(app_type.as_str()) {
            tracing::info!(
                "Provider '{}' has '{}' in sync_to_cli_tools, syncing to CLI Tool config",
                target_provider.name,
                app_type.as_str()
            );
            ConfigSyncService::sync_to_file(target_provider)?;
        } else {
            tracing::info!(
                "Provider '{}' does not have '{}' in sync_to_cli_tools, skipping config sync (using LLM Gateway Router mode)",
                target_provider.name,
                app_type.as_str()
            );
        }

        Ok(())
    }

    /// 切换 Provider 并同步到所有指定的 CLI Tool
    ///
    /// 这个方法会同步到 Provider 的 sync_to_cli_tools 列表中的所有 CLI Tool
    pub fn switch_and_sync_all(db: &Database, provider_id: i64) -> Result<()> {
        // 获取 Provider（需要先知道它属于哪个 app_type）
        // 这里我们遍历所有 app_type 来查找
        for app_type in AppType::all() {
            let providers = ProviderDao::get_all(db, app_type)?;
            if let Some(provider) = providers.iter().find(|p| p.id == provider_id) {
                // 找到了 Provider，同步到所有指定的 CLI Tool
                for cli_tool_str in &provider.sync_to_cli_tools {
                    if let Ok(cli_tool) = cli_tool_str.parse::<AppType>() {
                        tracing::info!(
                            "Syncing provider '{}' to CLI Tool '{}'",
                            provider.name,
                            cli_tool_str
                        );

                        // 创建一个临时的 Provider 副本，设置正确的 app_type
                        let mut provider_copy = provider.clone();
                        provider_copy.app_type = cli_tool;

                        ConfigSyncService::sync_to_file(&provider_copy)?;
                    } else {
                        tracing::warn!("Invalid CLI tool name in sync_to_cli_tools: {}", cli_tool_str);
                    }
                }

                return Ok(());
            }
        }

        Err(anyhow::anyhow!("Provider with id {} not found", provider_id))
    }

    /// 切换单个 CLI Tool 的同步状态
    ///
    /// 添加或移除 CLI Tool 到 sync_to_cli_tools 列表
    pub fn toggle_sync_to_cli_tool(
        db: &Database,
        provider_id: i64,
        cli_tool: &str,
        enable: bool,
    ) -> Result<()> {
        // 验证 CLI Tool 名称
        let _app_type = cli_tool.parse::<AppType>()
            .map_err(|_| anyhow::anyhow!("Invalid CLI tool: {}", cli_tool))?;

        // 查找 Provider（遍历所有 app_type）
        for app_type in AppType::all() {
            let providers = ProviderDao::get_all(db, app_type)?;
            if let Some(mut provider) = providers.into_iter().find(|p| p.id == provider_id) {
                // 更新 sync_to_cli_tools 列表
                if enable {
                    if !provider.sync_to_cli_tools.contains(&cli_tool.to_string()) {
                        provider.sync_to_cli_tools.push(cli_tool.to_string());
                    }
                } else {
                    provider.sync_to_cli_tools.retain(|t| t != cli_tool);
                }

                // 保存到数据库
                ProviderDao::update(db, &provider)?;

                // 如果启用同步且该 Provider 是当前激活的，立即同步
                if enable && provider.is_active {
                    tracing::info!(
                        "Enabling sync to {} for active provider '{}', syncing immediately",
                        cli_tool,
                        provider.name
                    );

                    let mut provider_copy = provider.clone();
                    provider_copy.app_type = cli_tool.parse().unwrap();
                    ConfigSyncService::sync_to_file(&provider_copy)?;
                }

                return Ok(());
            }
        }

        Err(anyhow::anyhow!("Provider with id {} not found", provider_id))
    }

    /// 设置 Provider 的完整 sync_to_cli_tools 列表
    ///
    /// 注意：此方法会立即同步一次（用于用户设置同步时的即时反馈）
    /// 但只有 active 的 Provider 才会在切换时自动同步
    pub fn set_sync_to_cli_tools(
        db: &Database,
        provider_id: i64,
        cli_tools: Vec<String>,
        proxy_url: Option<String>,
    ) -> Result<()> {
        // 验证所有 CLI Tool 名称
        for cli_tool in &cli_tools {
            cli_tool.parse::<AppType>()
                .map_err(|_| anyhow::anyhow!("Invalid CLI tool: {}", cli_tool))?;
        }

        // 查找 Provider（遍历所有 app_type）
        for app_type in AppType::all() {
            let providers = ProviderDao::get_all(db, app_type)?;
            if let Some(mut provider) = providers.into_iter().find(|p| p.id == provider_id) {
                provider.sync_to_cli_tools = cli_tools.clone();

                // 保存到数据库
                ProviderDao::update(db, &provider)?;

                // 立即同步一次（给用户即时反馈）
                // 注意：这是设置同步时的一次性操作，不影响后续的 active 逻辑
                for cli_tool_str in &cli_tools {
                    if let Ok(cli_tool) = cli_tool_str.parse::<AppType>() {
                        tracing::info!(
                            "Syncing provider '{}' to CLI Tool '{}' (initial sync)",
                            provider.name,
                            cli_tool_str
                        );

                        let mut provider_copy = provider.clone();
                        provider_copy.app_type = cli_tool;

                        // Apply proxy mode if specified
                        if let Some(ref url) = proxy_url {
                            provider_copy.base_url = url.clone();
                            provider_copy.api_key = "mrouter-proxy".to_string();
                        }

                        ConfigSyncService::sync_to_file(&provider_copy)?;
                    }
                }

                return Ok(());
            }
        }

        Err(anyhow::anyhow!("Provider with id {} not found", provider_id))
    }
}
