use serde::{Deserialize, Serialize};

/// Action type for task execution (matches dac_client::types::ActionType)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    Llm,
    Tool { pubkey: String },
    Agent2Agent { pubkey: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInput {
    pub context: String, // Combined context: goal spec + agent config + memory + previous output
    pub action_type: ActionType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutput {
    pub llm_response: String,
    pub reasoning: String,
}

/// Task execution snapshot stored on IPFS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionSnapshot {
    pub task_slot_id: u64,
    pub execution_count: u64,
    pub goal_id: u64,
    pub agent_pubkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compute_node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_cid: Option<String>,
    pub timestamp: i64,
    pub chain_proof: String,
    pub validation: ValidationInfo,
}

/// Validation information from TEE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationInfo {
    pub validator_pubkey: String,
    pub tee_signature: String,
    pub payment_amount: u64,
    pub approved: bool,
}

/// Network configuration (stored on IPFS)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub name: String,
    pub description: String,
    pub version: String,
    pub allowed_models: Vec<AllowedModel>,
}

/// Model configuration in network config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedModel {
    pub name: String,
    pub provider: String,
    pub parameters: u64,
    pub use_case: String,
    pub vram_requirements: VramRequirements,
    pub context_length: u64,
    pub cost_per_1m_tokens: f64,
}

/// VRAM requirements per quantization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramRequirements {
    pub fp16: f64,
    pub int8: f64,
    pub int4: f64,
}

/// Node configuration (stored on IPFS)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub hardware: HardwareConfig,
    pub software: SoftwareConfig,
    pub available_models: Vec<AvailableModel>,
}

/// Hardware configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareConfig {
    pub cpu: CpuConfig,
    pub gpu: Vec<GpuConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuConfig {
    pub model: String,
    pub cores: u32,
    pub threads: u32,
    pub base_clock_ghz: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuConfig {
    pub vram_gb: u32,
}

/// Software configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareConfig {
    pub cuda_version: String,
}

/// Available model on compute node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableModel {
    pub model_name: String,
    pub quantization: String,
    pub vram_required_gb: f64,
    pub gpu_index: u32,
    pub max_concurrent_requests: u32,
}

/// Agent configuration (stored on IPFS)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub model: ModelConfig,
    pub capabilities: AgentCapabilities,
    pub behavior: AgentBehavior,
    pub memory: AgentMemoryConfig,
    pub specialization: Specialization,
}

/// Model configuration for agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub model_name: String,
    pub parameters: ModelParameters,
    pub context: ContextConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelParameters {
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: u32,
    pub max_tokens: u32,
    pub repetition_penalty: f64,
    pub stop_sequences: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    pub max_context_length: u32,
    pub specialization: String,
}

/// Agent capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub languages: Vec<LanguageSupport>,
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageSupport {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libraries: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crates: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frameworks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_commands: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported_databases: Option<Vec<String>>,
}

/// Agent behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBehavior {
    pub max_memory_usage_mb: u32,
    pub max_file_size_mb: u32,
    pub safety: SafetyConfig,
    pub output_format: String,
    pub structured_output: bool,
    pub error_handling: ErrorHandling,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    pub allow_network_access: bool,
    pub allow_file_system_write: bool,
    pub require_code_review: bool,
    pub max_iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandling {
    pub retry_on_failure: bool,
    pub max_retries: u32,
    pub retry_delay_seconds: u32,
    pub fail_fast: bool,
}

/// Agent memory configuration (metadata)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryConfig {
    #[serde(rename = "type")]
    pub memory_type: String,
    pub max_memory_entries: u32,
    pub memory_retention_days: u32,
    pub compression: bool,
}

/// Agent specialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Specialization {
    pub primary_domain: String,
    pub secondary_domains: Vec<String>,
    pub expertise_level: String,
}

/// Goal specification (stored on IPFS)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalSpecification {
    pub title: String,
    pub description: String,
    pub category: String,
    pub deliverables: Vec<Deliverable>,
    pub success_criteria: Vec<String>,
    pub expected_output: ExpectedOutput,
    pub constraints: Constraints,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration_plan: Option<Vec<IterationPlan>>,
    pub validation: ValidationCriteria,
    pub completion_detection: CompletionDetection,
    pub limits: Limits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deliverable {
    pub name: String,
    #[serde(rename = "type")]
    pub deliverable_type: String,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirements: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_target: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedOutput {
    pub format: String,
    pub structure: Vec<OutputField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputField {
    pub field: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraints {
    pub technology_stack: Vec<String>,
    pub performance: PerformanceConstraints,
    pub security: Vec<String>,
    pub code_quality: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConstraints {
    pub response_time_ms: u32,
    pub throughput_rps: u32,
    pub concurrent_users: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationPlan {
    pub iteration: u32,
    pub goal: String,
    pub deliverables: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCriteria {
    pub automated: Vec<AutomatedCheck>,
    pub manual: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomatedCheck {
    #[serde(rename = "type")]
    pub check_type: String,
    pub tool: String,
    pub checks: Option<Vec<String>>,
    pub requirements: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionDetection {
    pub method: String,
    pub indicators: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limits {
    pub max_iterations: u32,
    pub max_cost_sol: f64,
    pub timeout_hours: u32,
}
