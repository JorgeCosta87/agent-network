use anyhow::{Result, Context};
use dac_common::types::{AvailableModel, NetworkConfig};
use ipfs_adapter::IpfsClient;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_name: String,
    pub quantization: String, // "fp16", "int8", or "int4"
    pub max_concurrent_requests: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSettings {
    pub models: Vec<ModelConfig>,
}

impl NodeSettings {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read node settings file: {:?}", path.as_ref()))?;
        
        let settings: NodeSettings = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse node settings YAML: {:?}", path.as_ref()))?;
        
        Ok(settings)
    }
}

pub struct ModelValidator;

impl ModelValidator {
    pub async fn validate_available_models(
        network_config_cid: &str,
        model_configs: Vec<ModelConfig>,
        gpu_vram_gb: Vec<u32>, 
        ipfs_client: &IpfsClient,
    ) -> Result<Vec<AvailableModel>> {
        let network_config_bytes = ipfs_client.cat(network_config_cid).await
            .context("Failed to fetch network config from IPFS")?;
        
        let network_config: NetworkConfig = serde_yaml::from_slice(&network_config_bytes)
            .context("Failed to parse network config")?;

        let mut validated_models = Vec::new();
        let mut gpu_usage = vec![0.0; gpu_vram_gb.len()];

        for model_config in model_configs {
            let allowed_model = network_config.allowed_models
                .iter()
                .find(|m| m.name == model_config.model_name)
                .ok_or_else(|| anyhow::anyhow!("Model '{}' is not in network's allowed models", model_config.model_name))?;

            let vram_required = match model_config.quantization.as_str() {
                "fp16" => allowed_model.vram_requirements.fp16,
                "int8" => allowed_model.vram_requirements.int8,
                "int4" => allowed_model.vram_requirements.int4,
                _ => anyhow::bail!("Invalid quantization: {}. Must be 'fp16', 'int8', or 'int4'", model_config.quantization),
            };

            let gpu_index = Self::find_available_gpu(
                vram_required,
                &gpu_vram_gb,
                &gpu_usage,
            )?;

            gpu_usage[gpu_index] += vram_required;

            validated_models.push(AvailableModel {
                model_name: model_config.model_name,
                quantization: model_config.quantization,
                vram_required_gb: vram_required,
                gpu_index: gpu_index as u32,
                max_concurrent_requests: model_config.max_concurrent_requests,
            });
        }

        Ok(validated_models)
    }

    fn find_available_gpu(
        vram_required: f64,
        gpu_vram_gb: &[u32],
        gpu_usage: &[f64],
    ) -> Result<usize> {
        for (i, &vram_gb) in gpu_vram_gb.iter().enumerate() {
            let available_vram = vram_gb as f64 - gpu_usage[i];
            if available_vram >= vram_required {
                return Ok(i);
            }
        }

        anyhow::bail!("No GPU has enough VRAM (required: {} GB)", vram_required)
    }
}
