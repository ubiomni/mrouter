// CLI 命令实现

use anyhow::Result;
use crate::database;
use crate::database::dao::*;
use crate::cli::{ProxyCommand, ProxyAction};
use crate::services::HealthCheckService;
use crate::services::proxy::ProxyServer;

pub async fn switch_provider(name: &str) -> Result<()> {
    use crate::models::ProviderStatus;
    let db = database::init().await?;

    let providers = ProviderDao::get_all(&db, 0)?;

    if let Some(provider) = providers.iter().find(|p| p.name == name) {
        let mut updated = provider.clone();
        updated.status = ProviderStatus::Enabled;
        ProviderDao::update(&db, &updated)?;

        println!("Switched to provider: {}", name);
    } else {
        println!("Provider not found: {}", name);
        println!("Available providers:");
        for p in &providers {
            let marker = if p.status.is_active() { "●" } else { "○" };
            println!("  {} {}", marker, p.name);
        }
    }

    Ok(())
}

pub async fn list_providers() -> Result<()> {
    let db = database::init().await?;

    let providers = ProviderDao::get_all(&db, 0)?;
    if !providers.is_empty() {
        println!("\nProviders");
        println!("{}", "=".repeat(40));
        for provider in providers {
            let marker = if provider.status.is_active() { "●" } else { "○" };
            println!("  {} {} (priority: {})", marker, provider.name, provider.priority);
        }
    }

    Ok(())
}

pub async fn show_status() -> Result<()> {
    let db = database::init().await?;

    println!("MRouter Status");
    println!("{}", "=".repeat(40));

    let providers = ProviderDao::get_all(&db, 0)?;
    let active: Vec<_> = providers.iter().filter(|p| p.status.is_active()).collect();
    if active.is_empty() {
        println!("No active providers");
    } else {
        for provider in active {
            println!("  [active] {} (priority: {})", provider.name, provider.priority);
        }
    }

    Ok(())
}

pub async fn health_check() -> Result<()> {
    let db = database::init().await?;
    let health_service = HealthCheckService::new(db.clone());

    println!("Running health check...\n");

    let providers = ProviderDao::get_all_active(&db, 0)?;
    if providers.is_empty() {
        println!("No active providers to check.");
        return Ok(());
    }

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

    Ok(())
}

pub async fn show_stats(export: Option<String>) -> Result<()> {
    let db = database::init().await?;

    let from = (chrono::Utc::now() - chrono::Duration::days(7)).timestamp_millis();
    let to = chrono::Utc::now().timestamp_millis();

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

            let client = reqwest::Client::new();
            match client.get(format!("http://127.0.0.1:{}/health", port)).send().await {
                Ok(resp) if resp.status().is_success() => {
                    println!("Proxy Status: Running on port {}", port);
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
