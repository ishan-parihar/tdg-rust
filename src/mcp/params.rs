use rmcp::schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "Search query text")]
    pub query: String,
    #[schemars(description = "Optional filter by node type")]
    pub node_type: Option<String>,
    #[schemars(description = "Number of results (max 50)")]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetNodeParams {
    #[schemars(description = "Node ID")]
    pub node_id: String,
    #[schemars(description = "Include neighbors and paths")]
    pub include_context: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryEventsParams {
    #[schemars(description = "Filter by event action")]
    pub action: Option<String>,
    #[schemars(description = "Filter by node ID")]
    pub node_id: Option<String>,
    #[schemars(description = "Start timestamp (ISO 8601)")]
    pub after: Option<String>,
    #[schemars(description = "End timestamp (ISO 8601)")]
    pub before: Option<String>,
    #[schemars(description = "Max records (500)")]
    pub limit: Option<i64>,
    #[schemars(description = "Pagination offset")]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateParams {
    #[schemars(description = "Node ID (auto-generated if omitted)")]
    pub node_id: Option<String>,
    #[schemars(description = "Node type (e.g. 'task', 'note')")]
    pub node_type: String,
    #[schemars(description = "Text payload")]
    pub text: String,
    #[schemars(description = "Embedding vector (auto-generated if omitted)")]
    pub embedding: Option<Vec<f32>>,
    #[schemars(description = "Optional aliases")]
    pub aliases: Option<Vec<String>>,
    #[schemars(description = "Optional metadata JSON")]
    pub meta: Option<serde_json::Value>,
    #[schemars(description = "Optional trust score (0.0–1.0)")]
    pub trust: Option<f64>,
    #[schemars(description = "Node name/title")]
    pub name: String,
    #[schemars(description = "Parent node IDs to connect")]
    pub parent_ids: Option<String>,
    #[schemars(description = "Quadrant (UL, UR, LL, LR)")]
    pub quadrant: Option<String>,
    #[schemars(description = "Telos level (T0-T6)")]
    pub t_level: Option<String>,
    #[schemars(description = "Developmental stage")]
    pub stage: Option<i64>,
    #[schemars(description = "Description text")]
    pub description: Option<String>,
    #[schemars(description = "Source provenance")]
    pub source: Option<String>,
    #[schemars(description = "Lifecycle state")]
    pub lifecycle_state: Option<String>,
    #[schemars(description = "Node IDs this blocks/targets")]
    pub blocks_targets: Option<String>,
    #[schemars(description = "Node IDs providing evidence")]
    pub evidence_targets: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateParams {
    #[schemars(description = "Node ID to update")]
    pub node_id: String,
    #[schemars(description = "New text payload")]
    pub text: Option<String>,
    #[schemars(description = "New type")]
    pub node_type: Option<String>,
    #[schemars(description = "New aliases (replaces existing)")]
    pub aliases: Option<Vec<String>>,
    #[schemars(description = "Merge metadata")]
    pub meta: Option<serde_json::Value>,
    #[schemars(description = "Node name/title")]
    pub name: Option<String>,
    #[schemars(description = "Description text")]
    pub description: Option<String>,
    #[schemars(description = "Lifecycle state")]
    pub lifecycle_state: Option<String>,
    #[schemars(description = "Telos level (T0-T6)")]
    pub t_level: Option<String>,
    #[schemars(description = "Developmental stage")]
    pub stage: Option<i64>,
    #[schemars(description = "Parent node IDs to add")]
    pub add_parent_ids: Option<String>,
    #[schemars(description = "Parent node IDs to remove")]
    pub remove_parent_ids: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConnectParams {
    #[schemars(description = "Source node ID")]
    pub source_id: String,
    #[schemars(description = "Target node ID")]
    pub target_id: String,
    #[schemars(description = "Edge type (e.g. 'related_to')")]
    pub edge_type: String,
    #[schemars(description = "Optional weight (0.0–1.0)")]
    pub weight: Option<f64>,
    #[schemars(description = "Optional metadata JSON")]
    pub meta: Option<serde_json::Value>,
    #[schemars(description = "Assert edge (skip validation)")]
    pub as_edge: Option<String>,
    #[schemars(description = "Force create even if invalid")]
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BulkCreateParams {
    #[schemars(description = "Array of node specs (max 500)")]
    pub nodes: Vec<CreateParams>,
    #[schemars(description = "JSON string of nodes")]
    pub nodes_json: String,
    #[schemars(description = "JSON string of edges")]
    pub edges_json: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordExecParams {
    #[schemars(description = "Node ID of the skill")]
    pub node_id: String,
    #[schemars(description = "Was the execution helpful?")]
    pub helpful: bool,
    #[schemars(description = "Optional reason")]
    pub reason: Option<String>,
    #[schemars(description = "Action type")]
    pub action_type: String,
    #[schemars(description = "Description of execution")]
    pub description: String,
    #[schemars(description = "Metrics JSON")]
    pub metrics_json: Option<String>,
    #[schemars(description = "Result of execution")]
    pub result: String,
    #[schemars(description = "Tags")]
    pub tags: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RateMemoryParams {
    #[schemars(description = "Node ID to rate")]
    pub node_id: String,
    #[schemars(description = "helpful or unhelpful")]
    pub rating: String,
    #[schemars(description = "Optional reason")]
    pub reason: Option<String>,
    #[schemars(description = "Is this memory helpful?")]
    pub helpful: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MindStateParams {
    #[schemars(description = "Get terrain context for a skill")]
    pub terrain_for: Option<String>,
    #[schemars(description = "Get injection status")]
    pub injection_status: Option<bool>,
    #[schemars(description = "Get mind summary")]
    pub summary: Option<bool>,
    #[schemars(description = "Get full mind state")]
    pub full: Option<bool>,
    #[schemars(description = "Get detailed state")]
    pub detail: Option<bool>,
    #[schemars(description = "Get health status")]
    pub health: Option<bool>,
    #[schemars(description = "Verify graph integrity")]
    pub verify: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ObserveParams {
    #[schemars(description = "Conversation text to observe")]
    pub text: String,
    #[schemars(description = "Optional speaker name")]
    pub speaker: Option<String>,
    #[schemars(description = "Optional turn number")]
    pub turn: Option<i64>,
    #[schemars(description = "Optional conversation topic")]
    pub topic: Option<String>,
    #[schemars(description = "Digestion cycle number")]
    pub cycle: Option<i64>,
    #[schemars(description = "Observation description")]
    pub description: String,
    #[schemars(description = "Entities mentioned")]
    pub entities: Option<String>,
    #[schemars(description = "Quadrant (UL, UR, LL, LR)")]
    pub quadrant: Option<String>,
    #[schemars(description = "Trigger graph digestion")]
    pub trigger_digestion: Option<bool>,
    #[schemars(description = "Trust level for this observation")]
    pub trust: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetRelatedParams {
    #[schemars(description = "Node ID to find related nodes for")]
    pub node_id: String,
    #[schemars(description = "Max related nodes to return")]
    pub limit: Option<i64>,
    #[schemars(description = "Edge type to filter by")]
    pub edge_type: Option<String>,
    #[schemars(description = "Direction: outgoing, incoming, or both")]
    pub direction: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MaintenanceParams {
    #[schemars(description = "Action: rebuild_fts, rebuild_embeddings, gc_nodes, gc_edges, gc_all, health")]
    pub action: Option<String>,
    #[schemars(description = "Batch size for operations (default 500)")]
    pub batch_size: Option<i64>,
    #[schemars(description = "Deprecated: use 'action' instead")]
    pub phase: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SelfManageParams {
    #[schemars(description = "Action: gc_nodes, gc_edges, gc_all, health")]
    pub action: String,
    #[schemars(description = "Dry run mode (default true)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BankParams {
    #[schemars(description = "Action: list, set_context, or get_nodes")]
    pub action: Option<String>,
    #[schemars(description = "Profile name")]
    pub profile: Option<String>,
    #[schemars(description = "Bank ID")]
    pub bank_id: Option<String>,
    #[schemars(description = "Filter by node type")]
    pub node_type: Option<String>,
    #[schemars(description = "Max results")]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntityParams {
    #[schemars(description = "Entity name to resolve")]
    pub name: Option<String>,
    #[schemars(description = "Text to extract entities from")]
    pub text: Option<String>,
    #[schemars(description = "Node ID for alias operations")]
    pub node_id: Option<String>,
    #[schemars(description = "Action: resolve, get, add, or update")]
    pub action: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReflectParams {
    #[schemars(description = "Number of recent turns to consider")]
    pub turns: Option<i64>,
    #[schemars(description = "Comma-separated focus topics")]
    pub focus_topics: Option<String>,
    #[schemars(description = "Check API/Ollama status only")]
    pub status_only: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReflectRunParams {
    #[schemars(description = "Number of recent turns to consider (unused, engine uses internal config)")]
    pub turns: Option<i64>,
    #[schemars(description = "Dry run mode (unused, engine uses internal config)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConsolidateParams {
    #[schemars(description = "Lean mode quick snapshot (skip reflection)")]
    pub lean_mode: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetTrustParams {
    #[schemars(description = "Agent name to query")]
    pub agent_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AdjustTrustParams {
    #[schemars(description = "Agent name")]
    pub agent_name: String,
    #[schemars(description = "Delta to apply (positive or negative)")]
    pub delta: f64,
    #[schemars(description = "Optional reason")]
    pub reason: Option<String>,
    #[schemars(description = "Trust source identifier")]
    pub source: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HealthCheckParams {
    #[schemars(description = "Service name")]
    pub service: String,
    #[schemars(description = "Latency in milliseconds")]
    pub latency_ms: f64,
    #[schemars(description = "Was the check successful?")]
    pub success: bool,
    #[schemars(description = "Optional error message")]
    pub error_message: Option<String>,
    #[schemars(description = "Optional metadata")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SystemHealthParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphStatsParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveMindStateParams {
    #[schemars(description = "Optional label for this checkpoint")]
    pub label: Option<String>,
    #[schemars(description = "Session ID")]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoadMindStateParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetProjectContextParams {
    #[schemars(description = "Project context text")]
    pub context: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PrefetchParams {
    #[schemars(description = "Topic or query to prefetch")]
    pub topic: String,
    #[schemars(description = "Max related nodes to fetch")]
    pub limit: Option<i64>,
    #[schemars(description = "Search query")]
    pub query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportParams {
    #[schemars(description = "Output file path (default: stdout)")]
    pub output_path: Option<String>,
    #[schemars(description = "Export format: json (default)")]
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportParams {
    #[schemars(description = "Input file path to import")]
    pub input_path: String,
    #[schemars(description = "Skip duplicate nodes (default: true)")]
    pub skip_duplicates: Option<bool>,
}
