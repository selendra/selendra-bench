use crate::types::NodeMetrics;
use jsonrpsee::{
    core::client::ClientT,
    rpc_params,
    ws_client::WsClient,
};
use std::error::Error;
use std::time::Instant;

pub struct MetricsCollector {
    client: WsClient,
    interval: u64,
}

impl MetricsCollector {
    pub fn new(client: WsClient, interval: u64) -> Self {
        Self { client, interval }
    }

    pub async fn collect_metrics(&self) -> Result<NodeMetrics, Box<dyn Error>> {
        let timestamp = Instant::now();
        
        let peer_count = self.get_peer_count().await?;
        let tx_pool_size = self.get_transaction_pool_size().await?;
        let (memory_usage, cpu_usage) = self.get_resource_usage().await?;
        let (network_tx, network_rx) = self.get_network_stats().await?;
        let (block_height, block_size) = self.get_block_info().await?;
        let time_to_finality = self.get_time_to_finality().await?;

        Ok(NodeMetrics {
            timestamp,
            peer_count,
            transaction_pool_size: tx_pool_size,
            memory_usage_mb: memory_usage,
            cpu_usage_percent: cpu_usage,
            network_tx_bytes: network_tx,
            network_rx_bytes: network_rx,
            block_height,
            block_size_bytes: block_size,
            time_to_finality_ms: time_to_finality,
        })
    }

    async fn get_peer_count(&self) -> Result<usize, Box<dyn Error>> {
        let count: usize = self.client.request("system_peers", rpc_params![]).await?;
        Ok(count)
    }

    async fn get_transaction_pool_size(&self) -> Result<usize, Box<dyn Error>> {
        let size: usize = self.client.request("author_pendingExtrinsics", rpc_params![]).await?;
        Ok(size)
    }

    async fn get_resource_usage(&self) -> Result<(f64, f64), Box<dyn Error>> {
        let usage: serde_json::Value = self.client.request("system_health", rpc_params![]).await?;
        let memory = usage["memory"].as_f64().unwrap_or(0.0);
        let cpu = usage["cpu"].as_f64().unwrap_or(0.0);
        Ok((memory, cpu))
    }

    async fn get_network_stats(&self) -> Result<(u64, u64), Box<dyn Error>> {
        let stats: serde_json::Value = self.client.request("system_networkState", rpc_params![]).await?;
        let tx = stats["tx_bytes"].as_u64().unwrap_or(0);
        let rx = stats["rx_bytes"].as_u64().unwrap_or(0);
        Ok((tx, rx))
    }

    async fn get_block_info(&self) -> Result<(u32, usize), Box<dyn Error>> {
        let header: serde_json::Value = self.client.request("chain_getHeader", rpc_params![]).await?;
        let height = header["number"].as_u64().unwrap_or(0) as u32;
        let size = header["size"].as_u64().unwrap_or(0) as usize;
        Ok((height, size))
    }

    async fn get_time_to_finality(&self) -> Result<u64, Box<dyn Error>> {
        let finality: u64 = self.client.request("chain_getFinalizedHead", rpc_params![]).await?;
        Ok(finality)
    }
} 