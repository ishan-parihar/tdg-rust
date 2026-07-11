//! Synthesis helpers — extracted from `tools.rs` to reduce god-module size.
//!
//! These functions support the `tdg_reflect` tool's LLM-powered synthesis
//! pipeline. They were previously inlined in `src/mcp/tools.rs` (Phase 0
//! refactor: extract to shrink the 3,464-LOC god module).
//!
//! Contents:
//! - `try_llm_providers` — provider chain (openai / anthropic / ollama) respecting `default_provider`
//! - `parse_llm_output` — strip code fences, extract JSON
//! - `normalize_synthesis_json` — coerce keys to canonical schema
//! - `pattern_synthesis` — fallback pattern-based synthesis when LLM is unavailable
//! - `store_synthesis` — persist synthesis + insights as nodes/edges
//! - `auto_detect_edge_type` — heuristic edge-type inference from node types

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::crud::{add_edge, add_node};
use crate::models::{NewEdge, NewNode};

/// Try LLM providers in configured order, returning the first successful
/// (parsed_json, provider_name) tuple.
///
/// Provider order is determined by `cfg.default_provider`:
/// - "openai"    → [openai, anthropic, ollama]
/// - "anthropic" → [anthropic, openai, ollama]
/// - "ollama"    → [ollama, openai, anthropic]
/// - _           → [ollama, openai, anthropic]  (sensible default: local first)
///
/// Errors are surfaced via `tracing::warn!` rather than silently swallowed
/// (the previous `.ok()?` pattern made 401s indistinguishable from network
/// errors). If a provider's output can't be parsed as JSON, we log and try
/// the next provider.
pub(crate) async fn try_llm_providers(
    _client: &reqwest::Client,
    cfg: &crate::llm::config::LlmConfig,
    prompt: &str,
) -> Option<(Value, String)> {
    let request = crate::llm::LlmCompletionRequest {
        messages: vec![crate::llm::LlmMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
        model: None,
        temperature: None,
        max_tokens: None,
    };

    let order: Vec<&str> = match cfg.default_provider.as_str() {
        "openai" => vec!["openai", "anthropic", "ollama"],
        "anthropic" => vec!["anthropic", "openai", "ollama"],
        "ollama" => vec!["ollama", "openai", "anthropic"],
        _ => vec!["ollama", "openai", "anthropic"],
    };

    for provider_name in &order {
        if !cfg.provider_available(provider_name) {
            continue;
        }

        let provider: Box<dyn crate::llm::LlmProvider> = match *provider_name {
            "openai" => Box::new(crate::llm::openai::OpenAiProvider::new(cfg.openai.clone())),
            "anthropic" => Box::new(crate::llm::anthropic::AnthropicProvider::new(
                cfg.anthropic.clone(),
            )),
            "ollama" => Box::new(crate::llm::ollama::OllamaProvider::new(cfg.ollama.clone())),
            _ => continue,
        };

        match provider.complete(&request).await {
            Ok(response) => {
                tracing::info!(
                    "LLM provider '{}' succeeded (prompt_tokens={}, completion_tokens={})",
                    provider.name(),
                    response.usage.prompt_tokens,
                    response.usage.completion_tokens
                );
                if let Some(parsed) = parse_llm_output(&response.content) {
                    return Some((parsed, provider.name().to_string()));
                } else {
                    tracing::warn!(
                        "LLM provider '{}' returned unparseable output ({} chars)",
                        provider.name(),
                        response.content.len()
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    "LLM provider '{}' failed: {} — trying next provider",
                    provider.name(),
                    e
                );
            }
        }
    }

    None
}

/// Strip code fences and extract JSON from LLM output.
///
/// Handles three cases:
/// 1. Raw JSON (most reliable)
/// 2. JSON wrapped in ``` fences (common with LLMs)
/// 3. JSON embedded in prose (extracted via first-`{` to last-`}` slice)
pub(crate) fn parse_llm_output(raw: &str) -> Option<Value> {
    let mut text = raw.trim();
    if text.starts_with("```") {
        if let Some(nl) = text.find('\n') {
            text = &text[nl + 1..];
        }
        if let Some(end) = text.rfind("```") {
            text = text[..end].trim();
        }
    }
    if let Ok(data) = serde_json::from_str::<Value>(text) {
        return normalize_synthesis_json(data);
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if end > start {
            if let Ok(data) = serde_json::from_str::<Value>(&text[start..=end]) {
                return normalize_synthesis_json(data);
            }
        }
    }
    None
}

/// Coerce synthesis JSON keys to canonical schema.
///
/// LLMs sometimes return "Insights" instead of "insights", "Summary" instead
/// of "synthesis", etc. This normalises everything to:
/// `{ insights: [], patterns: [], synthesis: "", questions: [], confidence: 0.5 }`
pub(crate) fn normalize_synthesis_json(data: Value) -> Option<Value> {
    let insights = data
        .get("insights")
        .or_else(|| data.get("Insights"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let patterns = data
        .get("patterns")
        .or_else(|| data.get("Patterns"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let questions = data
        .get("questions")
        .or_else(|| data.get("Questions"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let synthesis = data
        .get("synthesis")
        .or_else(|| data.get("Synthesis"))
        .or_else(|| data.get("summary"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let confidence = data
        .get("confidence")
        .or_else(|| data.get("Confidence"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    Some(json!({
        "insights": insights,
        "patterns": patterns,
        "synthesis": synthesis,
        "questions": questions,
        "confidence": confidence,
    }))
}

/// Fallback pattern-based synthesis when no LLM provider is available.
///
/// Produces a structured analysis from graph state alone: node-type
/// distribution, entity references, edge density, temporal bursts, drive
/// averages, depleted drives, and focus-topic questions. Confidence is
/// fixed at 0.4 (vs ~0.7 for LLM synthesis) to signal lower certainty.
pub(crate) fn pattern_synthesis(
    conn: &Connection,
    context: &Value,
    total_nodes: i64,
    edge_count: i64,
    focus_topics: &[String],
) -> Value {
    let mut insights: Vec<String> = Vec::new();
    let mut patterns: Vec<String> = Vec::new();
    let mut questions: Vec<String> = Vec::new();

    let node_types = context.get("node_types").and_then(|v| v.as_object());
    let entities = context.get("entities").and_then(|v| v.as_array());
    let obs_count = node_types
        .and_then(|m| m.get("observation"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // ── Basic node type analysis ────────────────────────────────
    if let Some(types) = node_types {
        let total: i64 = types.values().filter_map(|v| v.as_i64()).sum();
        let mut sorted: Vec<_> = types.iter().collect();
        sorted.sort_by(|a, b| b.1.as_i64().unwrap_or(0).cmp(&a.1.as_i64().unwrap_or(0)));
        let type_summary: Vec<String> = sorted
            .iter()
            .take(5)
            .map(|(t, c)| format!("{}: {}", t, c))
            .collect();
        insights.push(format!(
            "Node distribution — {} total nodes across {} types. Top types: {}.",
            total,
            types.len(),
            type_summary.join(", ")
        ));
        if let Some((dominant_type, dominant_count)) = sorted.first() {
            let count = dominant_count.as_i64().unwrap_or(0);
            if total > 0 {
                patterns.push(format!(
                    "Graph is dominated by '{}' nodes ({}/{} = {:.0}%).",
                    dominant_type,
                    count,
                    total,
                    (count as f64 / total as f64) * 100.0
                ));
            }
        }
    }

    if obs_count > 0 {
        insights.push(format!(
            "Recent context includes {} observation nodes.",
            obs_count
        ));
    } else {
        insights.push("No recent observation activity detected.".to_string());
    }

    // ── Entity analysis ─────────────────────────────────────────
    if let Some(ent_arr) = entities {
        let names: Vec<&str> = ent_arr.iter().filter_map(|v| v.as_str()).collect();
        if !names.is_empty() {
            let display: Vec<&str> = names.iter().take(5).copied().collect();
            insights.push(format!(
                "Graph references {} known entities/people: {}{}.",
                names.len(),
                display.join(", "),
                if names.len() > 5 { "..." } else { "" }
            ));
        }
    }

    // ── Edge density analysis ───────────────────────────────────
    if obs_count > 0 && edge_count > 0 {
        let density = (edge_count as f64) / (obs_count as f64);
        let density_rounded = (density * 100.0).round() / 100.0;
        patterns.push(format!(
            "Edge density: {} edges per observation ({} edges / {} observations).",
            density_rounded, edge_count, obs_count
        ));
        if density < 1.0 {
            insights.push(
                "Low edge density suggests observations are under-connected — cross-linking may improve graph coherence.".to_string()
            );
        }
    }

    if let Some(types) = node_types {
        if types.len() >= 5 {
            patterns.push(format!(
                "Graph has high type diversity ({} types) — indicating a rich, multi-dimensional knowledge structure.",
                types.len()
            ));
        }
    }

    // ── Entity relationship analysis ────────────────────────────
    if let Some(ent_arr) = entities {
        let names: Vec<&str> = ent_arr.iter().filter_map(|v| v.as_str()).collect();
        if names.len() >= 2 {
            let rel_query = r#"
                SELECT e.edge_type, COUNT(*) as cnt
                FROM edges e
                JOIN nodes ns ON e.source_id = ns.id AND ns.valid_to IS NULL
                JOIN nodes nt ON e.target_id = nt.id AND nt.valid_to IS NULL
                WHERE e.valid_to IS NULL
                  AND (ns.node_type = 'people' OR nt.node_type = 'people')
                GROUP BY e.edge_type
                ORDER BY cnt DESC
                LIMIT 5
            "#;
            if let Ok(mut stmt) = conn.prepare(rel_query) {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                }) {
                    let rel_types: Vec<String> = rows
                        .filter_map(|r| r.ok())
                        .map(|(et, cnt)| format!("{}({})", et, cnt))
                        .collect();
                    if !rel_types.is_empty() {
                        patterns.push(format!(
                            "Entity relationship patterns: {}.",
                            rel_types.join(", ")
                        ));
                    }
                }
            }
        }
    }

    // ── Temporal pattern detection ──────────────────────────────
    {
        let temporal_query = r#"
            SELECT created_at, node_type
            FROM nodes
            WHERE valid_to IS NULL
            ORDER BY created_at DESC
            LIMIT 100
        "#;
        if let Ok(mut stmt) = conn.prepare(temporal_query) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                let entries: Vec<(String, String)> = rows.filter_map(|r| r.ok()).collect();
                if entries.len() >= 10 {
                    let mut day_counts: std::collections::HashMap<String, i64> =
                        std::collections::HashMap::new();
                    for (ts, _) in &entries {
                        let day = ts.chars().take(10).collect::<String>();
                        *day_counts.entry(day).or_insert(0) += 1;
                    }
                    if let Some((peak_day, peak_count)) = day_counts.iter().max_by_key(|(_, c)| *c)
                    {
                        if *peak_count >= 5 {
                            patterns.push(format!(
                                "Temporal burst: {} nodes created on {} — indicates concentrated activity.",
                                peak_count, peak_day
                            ));
                        }
                    }
                    let mut type_order: Vec<String> =
                        entries.iter().map(|(_, t)| t.clone()).collect();
                    type_order.dedup();
                    if type_order.len() <= 3 && entries.len() >= 10 {
                        patterns.push(format!(
                            "Recent activity concentrated in {} types: {} — suggests focused work phase.",
                            type_order.len(),
                            type_order.join(", ")
                        ));
                    }
                }
            }
        }
    }

    // ── Drive state analysis ────────────────────────────────────
    {
        let drive_query = r#"
            SELECT drives_json FROM nodes
            WHERE valid_to IS NULL
              AND drives_json IS NOT NULL
              AND drives_json != '{}'
            LIMIT 20
        "#;
        if let Ok(mut stmt) = conn.prepare(drive_query) {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                let drive_entries: Vec<String> = rows.filter_map(|r| r.ok()).collect();
                let mut drive_sum: std::collections::HashMap<String, f64> =
                    std::collections::HashMap::new();
                let mut drive_count = 0i64;
                for raw in &drive_entries {
                    if let Ok(val) = serde_json::from_str::<Value>(raw) {
                        if let Some(obj) = val.as_object() {
                            drive_count += 1;
                            for (k, v) in obj {
                                if let Some(f) = v.as_f64() {
                                    *drive_sum.entry(k.clone()).or_insert(0.0) += f;
                                }
                            }
                        }
                    }
                }
                if drive_count > 0 && !drive_sum.is_empty() {
                    let mut avg_drives: Vec<(String, f64)> = drive_sum
                        .iter()
                        .map(|(k, v)| (k.clone(), v / drive_count as f64))
                        .collect();
                    avg_drives
                        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    let top_drives: Vec<String> = avg_drives
                        .iter()
                        .take(3)
                        .map(|(k, v)| format!("{}({:.2})", k, v))
                        .collect();
                    insights.push(format!(
                        "Active drive dimensions across {} nodes: {}.",
                        drive_count,
                        top_drives.join(", ")
                    ));
                    let depleted: Vec<&String> = avg_drives
                        .iter()
                        .filter(|(_, v)| *v < 0.2)
                        .map(|(k, _)| k)
                        .collect();
                    if !depleted.is_empty() {
                        let depleted_names: Vec<&str> =
                            depleted.iter().map(|s| s.as_str()).collect();
                        questions.push(format!(
                            "Depleted drive dimensions ({}) may need attention — what actions could restore them?",
                            depleted_names.join(", ")
                        ));
                    }
                }
            }
        }
    }

    // ── Focus topics ────────────────────────────────────────────
    if !focus_topics.is_empty() {
        let topic_list = focus_topics.join(", ");
        insights.push(format!("Synthesis focused on: {}.", topic_list));
        for topic in focus_topics.iter().take(3) {
            questions.push(format!(
                "What patterns emerge specifically around '{}'?",
                topic
            ));
        }
    }

    // ── Open questions ──────────────────────────────────────────
    if let Some(ent_arr) = entities {
        let count = ent_arr.len();
        if count > 0 {
            questions.push(format!(
                "How do the {} known entities relate to each other and to the agent's goals?",
                count
            ));
        }
    }
    if obs_count > 50 {
        questions
            .push("With many observations, are there identifiable thematic clusters?".to_string());
    }
    questions
        .push("What emergent developmental patterns exist across the graph structure?".to_string());

    let synthesis = format!(
        "Pattern-based analysis of {} nodes ({} types, {} edges). The graph shows {} observations and {} known entities.",
        total_nodes,
        node_types.map(|m| m.len()).unwrap_or(0),
        edge_count,
        obs_count,
        entities.map(|a| a.len()).unwrap_or(0),
    );

    json!({
        "status": "ok",
        "method": "pattern",
        "insights": insights,
        "patterns": patterns,
        "synthesis": synthesis,
        "questions": questions,
        "confidence": 0.4,
    })
}

/// Persist a synthesis result (and its top-5 insights) as nodes + edges.
///
/// Creates:
/// - One `synthesis` node for the main result
/// - One `synthesis` node per insight (max 5), each with a `SYNTHESIZES` edge
///   pointing to the main synthesis node
///
/// Returns the list of created node IDs (main first, then insights).
pub(crate) fn store_synthesis(
    conn: &Connection,
    result: &Value,
    method: &str,
    synthesis_count_hint: i64,
) -> Vec<String> {
    let mut created = Vec::new();
    let summary = result
        .get("synthesis")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let short_summary: String = summary.chars().take(300).collect();
    let name_preview: String = short_summary.chars().take(80).collect();
    let confidence = result
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);

    let main_node = add_node(
        conn,
        &NewNode {
            node_type: "synthesis".to_string(),
            name: format!("Synthesis: {}", name_preview),
            description: Some(short_summary.clone()),
            properties: Some(json!({
                "insights": result.get("insights").cloned().unwrap_or(json!([])),
                "patterns": result.get("patterns").cloned().unwrap_or(json!([])),
                "questions": result.get("questions").cloned().unwrap_or(json!([])),
                "method": method,
                "confidence": confidence,
                "synthesis_count": synthesis_count_hint + 1,
            })),
            quadrants: Some(json!({"primary": "LR", "inferred": true})),
            lifecycle_state: Some("active".to_string()),
            source: Some(format!("reflect_tool/{}", method)),
            confidence: Some(confidence),
            ..Default::default()
        },
    );
    match main_node {
        Ok(node) => {
            let main_id = node.id.clone();
            created.push(main_id.clone());

            let insights = result
                .get("insights")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for (i, insight) in insights.iter().take(5).enumerate() {
                let insight_text = insight.as_str().unwrap_or("");
                let insight_preview: String = insight_text.chars().take(80).collect();
                let insight_full: String = insight_text.chars().take(500).collect();
                if let Ok(sub_node) = add_node(
                    conn,
                    &NewNode {
                        node_type: "synthesis".to_string(),
                        name: format!("Insight: {}", insight_preview),
                        description: Some(insight_full),
                        properties: Some(json!({
                            "source_node": main_id,
                            "index": i,
                            "kind": "insight",
                            "method": method,
                        })),
                        quadrants: Some(json!({"primary": "LR", "inferred": true})),
                        lifecycle_state: Some("active".to_string()),
                        source: Some(format!("reflect_tool/{}", method)),
                        confidence: Some(confidence),
                        ..Default::default()
                    },
                ) {
                    created.push(sub_node.id.clone());
                    if let Err(e) = add_edge(
                        conn,
                        &NewEdge {
                            source_id: sub_node.id.clone(),
                            target_id: main_id.clone(),
                            edge_type: "SYNTHESIZES".to_string(),
                            weight: Some(0.9),
                            properties: Some(json!({"kind": "insight_contribution"})),
                            ..Default::default()
                        },
                    ) {
                        tracing::warn!(
                            "Failed to create SYNTHESIZES edge from {}: {}",
                            sub_node.id,
                            e
                        );
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to store synthesis node: {}", e);
        }
    }
    created
}

/// Heuristic edge-type inference from (source_type, target_type).
///
/// Used by `tdg_connect` when the caller doesn't specify an edge_type.
/// Falls back to "USES" for unknown pairs.
pub(crate) fn auto_detect_edge_type(source_type: &str, target_type: &str) -> String {
    match (source_type, target_type) {
        ("action", "telos") => "ENABLES",
        ("observation", "telos") => "EVIDENCES",
        ("artifact", "telos") => "CONTEXT",
        ("people", "telos") => "PURSUES",
        ("hypothesis", "telos") => "EVIDENCES",
        ("constraint", "action") => "BLOCKS",
        ("telos", "telos") => "DECOMPOSES_TO",
        ("observation", "hypothesis") => "EVIDENCES",
        _ => "USES",
    }
    .to_string()
}
