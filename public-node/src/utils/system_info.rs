use anyhow::{Result, Context};
use dac_common::types::{
    NodeConfig, HardwareConfig, CpuConfig, GpuConfig, SoftwareConfig, AvailableModel,
};
use std::process::Command;
use std::fs;

pub struct SystemInfoCollector;

impl SystemInfoCollector {
    pub async fn collect_node_config(
        node_id: String,
        name: String,
        description: String,
        location: Option<String>,
        available_models: Vec<AvailableModel>,
    ) -> Result<NodeConfig> {
        let hardware = Self::collect_hardware_info().await?;
        let software = Self::collect_software_info().await?;

        Ok(NodeConfig {
            node_id,
            name,
            description,
            location,
            hardware,
            software,
            available_models,
        })
    }

    pub async fn collect_hardware_info() -> Result<HardwareConfig> {
        let cpu = Self::collect_cpu_info().await?;
        let gpu = Self::collect_gpu_info().await?;

        Ok(HardwareConfig {
            cpu,
            gpu,
        })
    }

    async fn collect_cpu_info() -> Result<CpuConfig> {
        let cpuinfo = fs::read_to_string("/proc/cpuinfo")
            .context("Failed to read /proc/cpuinfo")?;
        
        let model = cpuinfo
            .lines()
            .find(|line| line.starts_with("model name"))
            .and_then(|line| line.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Unknown CPU".to_string());

        let cores = num_cpus::get() as u32;
        let threads = num_cpus::get() as u32;

        let base_clock_ghz = cpuinfo
            .lines()
            .find(|line| line.starts_with("cpu MHz"))
            .and_then(|line| line.split(':').nth(1))
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|mhz| mhz / 1000.0)
            .unwrap_or(2.0);



        Ok(CpuConfig {
            model,
            cores,
            threads,
            base_clock_ghz,
            boost_clock_ghz: None,
            architecture: Some("x86_64".to_string()),
        })
    }


    async fn collect_gpu_info() -> Result<Vec<GpuConfig>> {
        let mut gpus = Vec::new();

        let nvidia_smi_output = Command::new("nvidia-smi")
            .args(&["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
            .output();

        match nvidia_smi_output {
            Ok(output) if output.status.success() => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                
                for line in output_str.lines() {
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        let name = parts[0].to_string();
                        let vram_mb = parts[1].parse::<u32>().unwrap_or(0);
                        let vram_gb = vram_mb / 1024;
                        
                        if vram_gb > 0 {
                            gpus.push(GpuConfig {
                                name,
                                vram_gb,
                            });
                        }
                    }
                }
            }
            _ => {
                println!("No NVIDIA GPUs found or nvidia-smi not available");
            }
        }

        Ok(gpus)
    }

    pub async fn collect_software_info() -> Result<SoftwareConfig> {
        let cuda_version = Self::get_cuda_version().await?;

        Ok(SoftwareConfig {
            cuda_version,
        })
    }

    async fn get_cuda_version() -> Result<String> {
        if let Ok(smi_output) = Command::new("nvidia-smi")
            .output()
        {
            if smi_output.status.success() {
                let output_str = String::from_utf8_lossy(&smi_output.stdout);
                for line in output_str.lines() {
                    if line.contains("CUDA Version:") {
                        if let Some(version) = line.split("CUDA Version:").nth(1) {
                            return Ok(version.trim().to_string());
                        }
                    }
                }
            }
        }

        Ok("Not installed".to_string())
    }

}
