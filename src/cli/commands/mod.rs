// CLI 命令实现

use anyhow::Result;
use crate::database;
use crate::database::dao::*;
use crate::models::AppType;
use crate::cli::{ProxyCommand, ProxyAction};
use crate::services::HealthCheckService;
use crate::services::proxy::ProxyServer;

pub async fn switch_provider(name: &str) -> Result<()> {
    let db = database::init().await?;

    // 默认切换 Claude Code 的 provider
    let app_type = AppType::ClaudeCode;
    let providers = ProviderDao::get_all(&db, app_type)?;

    if let Some(provider) = providers.iter().find(|p| p.name == name) {
        ProviderDao::set_active(&db, app_type, provider.id)?;

        // 同步配置文件
        if let Some(active) = ProviderDao::get_active(&db, app_type)? {
            if let Err(e) = crate::services::ConfigSyncService::sync_to_file(&active) {
                eprintln!("Warning: Failed to sync config: {}", e);
            }
        }

        println!("Switched to provider: {}", name);
    } else {
        println!("Provider not found: {}", name);
        println!("Available providers:");
        for p in &providers {
            let marker = if p.is_active { "●" } else { "○" };
            println!("  {} {}", marker, p.name);
        }
    }

    Ok(())
}

pub async fn list_providers() -> Result<()> {
    let db = database::init().await?;

    for app_type in AppType::all() {
        let providers = ProviderDao::get_all(&db, app_type)?;
        if !providers.is_empty() {
            println!("\n{}", app_type.display_name());
            println!("{}", "=".repeat(40));
            for provider in providers {
                let marker = if provider.is_active { "●" } else { "○" };
                let model = provider.model.as_deref().unwrap_or("default");
                println!("  {} [{}] {} ({})", marker, provider.provider_type.display_name(), provider.name, model);
            }
        }
    }

    Ok(())
}

pub async fn show_status() -> Result<()> {
    let db = database::init().await?;

    println!("MRouter Status");
    println!("{}", "=".repeat(40));

    for app_type in AppType::all() {
        if let Some(provider) = ProviderDao::get_active(&db, app_type)? {
            let model = provider.model.as_deref().unwrap_or("default");
            println!("{}: {} [{}] [{}]", app_type.display_name(), provider.name, provider.provider_type.display_name(), model);
        } else {
            println!("{}: No active provider", app_type.display_name());
        }
    }

    Ok(())
}

pub async fn health_check() -> Result<()> {
    let db = database::init().await?;
    let health_service = HealthCheckService::new(db.clone());

    println!("Running health check...\n");

    for app_type in AppType::all() {
        let providers = ProviderDao::get_all(&db, app_type)?;
        if providers.is_empty() {
            continue;
        }

        println!("{}", app_type.display_name());
        println!("{}", "-".repeat(40));

        let results = health_service.check_all_providers(&providers).await?;

        for (provider, health) in providers.iter().zip(results.iter()) {
            let icon = if health.is_healthy { "●" } else { "✗" };
            let latency = health.latency_ms
                .map(|ms| format!("{}ms", ms))
                .unwrap_or_else(|| "N/A".to_string());

            println!("  {} {} - {} (latency: {}, rate: {:.0}%)",
                icon,
                provider.name,
                if health.is_healthy { "Healthy" } else { "Unhealthy" },
                latency,
                health.success_rate * 100.0,
            );

            if let Some(err) = &health.last_error {
                println!("    Error: {}", err);
            }
        }
        println!();
    }

    Ok(())
}

pub async fn show_stats(export: Option<String>) -> Result<()> {
    let db = database::init().await?;

    let from = chrono::Utc::now() - chrono::Duration::days(7);
    let to = chrono::Utc::now();

    let summary = StatsDao::get_summary(&db, from, to)?;

    if let Some(format) = export {
        match format.as_str() {
            "json" => {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            }
            "csv" => {
                println!("provider,requests,tokens,cost");
                for p in &summary.by_provider {
                    println!("{},{},{},{:.2}", p.provider_name, p.requests, p.tokens, p.cost);
                }
            }
            _ => {
                println!("Unknown export format: {}", format);
            }
        }
    } else {
        println!("Usage Statistics (Last 7 Days)");
        println!("{}", "=".repeat(40));
        println!("Total Requests: {}", summary.total_requests);
        println!("Total Tokens: {}", summary.total_tokens);
        println!("Total Cost: ${:.2}", summary.total_cost);
        println!("Avg Cost/Request: ${:.4}", summary.avg_cost_per_request);

        if !summary.by_provider.is_empty() {
            println!("\nBy Provider:");
            for p in &summary.by_provider {
                println!("  {} - {} requests, {} tokens, ${:.2}",
                    p.provider_name, p.requests, p.tokens, p.cost);
            }
        }
    }

    Ok(())
}

pub async fn handle_proxy(cmd: ProxyCommand) -> Result<()> {
    match cmd.action {
        ProxyAction::Start => {
            let db = database::init().await?;
            let config = crate::config::AppConfig::load()?;
            let bind = config.proxy.bind;
            let port = config.proxy.port;

            println!("Starting proxy server on {}:{}...", bind, port);
            let proxy = ProxyServer::new(db, bind, port);
            proxy.start().await?;
        }
        ProxyAction::Stop => {
            println!("Stopping proxy server...");
            println!("(Use daemon stop for background proxy)");
        }
        ProxyAction::Status => {
            let config = crate::config::AppConfig::load()?;
            let port = config.proxy.port;

            // Try to check if proxy is running
            let client = reqwest::Client::new();
            match client.get(format!("http://127.0.0.1:{}/health", port)).send().await {
                Ok(resp) if resp.status().is_success() => {
                    println!("Proxy Status: Running on port {}", port);
                    // Get detailed status
                    if let Ok(status_resp) = client.get(format!("http://127.0.0.1:{}/status", port)).send().await {
                        if let Ok(body) = status_resp.text().await {
                            println!("{}", body);
                        }
                    }
                }
                _ => {
                    println!("Proxy Status: Not running");
                }
            }
        }
        ProxyAction::Logs => {
            println!("Proxy logs:");
            println!("(Use 'mrouter daemon logs' for daemon logs)");
        }
    }
    Ok(())
}
