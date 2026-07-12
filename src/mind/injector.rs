//! TDG Mind Injector — terrain-first context block orchestrator
//!
//! Port of `core/mind/injector.py` (498 lines).
//! Generates the full terrain-first context block for LLM consumption.

use rusqlite::Connection;
use serde_json::{json, Value};
use tracing::warn;

use crate::config::Config;
use crate::error::TdgResult;
use crate::mind::data_loader::*;
use crate::mind::diagnostic::DiagnosticEngine;
use crate::mind::feeling::{feeling_state_prompt, FeelingEngine};
use crate::mind::sections::*;
use crate::mind::terrain::{discover_skills_for_terrain, generate_terrain_context};

pub fn generate_prompt(conn: &Connection, cfg: &Config) -> TdgResult<String> {
    let lean = cfg.lean;
    let mut sections = Vec::new();

    let working_memory = load_working_memory(cfg);
    let loop_state = load_loop_state(cfg);

    if lean {
        sections.push("╔══════════════════════════════════════╗".to_string());
        sections.push("║  🔋 TDG LEAN MODE — Reduced overhead  ║".to_string());
        sections.push("╚══════════════════════════════════════╝".to_string());
        sections.push("".to_string());
    }

    // Phase 9: Metabolic context — graph-level health summary from the metabolism.
    // This surfaces G_z/P_z/state distributions so the agent knows its own
    // metabolic health without calling tdg_health on every holon.
    if !lean {
        if let Ok(metabolic) = generate_metabolic_summary(conn) {
            if !metabolic.is_empty() {
                sections.push("## 🧬 Metabolic State — Graph Health Summary".to_string());
                sections.push("".to_string());
                sections.push(metabolic);
                sections.push("".to_string());
            }
        }
    }

    sections.push("## 🌐 Terrain Context — What's Happening Now".to_string());
    sections.push("".to_string());
    sections.push(
        "*This is your primary navigation input. Read the terrain, then navigate.*".to_string(),
    );
    sections.push("".to_string());

    let terrain = generate_terrain_context(conn, &loop_state)?;

    if let Some(ctx) = terrain.get("cycle_context").and_then(|v| v.as_str()) {
        sections.push(format!("**{}**", ctx));
        sections.push("".to_string());
    }

    if let Some(anbt) = terrain
        .get("active_nodes_by_type")
        .and_then(|v| v.as_object())
    {
        let mut sorted: Vec<_> = anbt.iter().collect();
        sorted.sort_by(|a, b| b.1.as_i64().unwrap_or(0).cmp(&a.1.as_i64().unwrap_or(0)));
        let type_parts: Vec<String> = sorted
            .iter()
            .take(8)
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        sections.push(format!("**Graph composition:** {}", type_parts.join(" | ")));
        sections.push("".to_string());
    }

    if let Some(items) = terrain
        .get("highest_relevance_items")
        .and_then(|v| v.as_array())
    {
        if !items.is_empty() {
            sections.push("**Highest relevance items:**".to_string());
            for item in items.iter().take(5) {
                sections.push(format!("  - {}", item));
            }
            sections.push("".to_string());
        }
    }

    if let Some(edges) = terrain.get("open_edges").and_then(|v| v.as_array()) {
        if !edges.is_empty() {
            sections.push("**Open edges (awaiting action):**".to_string());
            for e in edges.iter() {
                sections.push(format!("  - {}", e));
            }
            sections.push("".to_string());
        }
    }

    if let Some(stale) = terrain.get("stale_items").and_then(|v| v.as_array()) {
        if !stale.is_empty() {
            sections.push("**Stale/unattended items:**".to_string());
            for s in stale.iter() {
                sections.push(format!("  - {}", s));
            }
            sections.push("".to_string());
        }
    }

    if !lean {
        let recent_events = load_recent_graph_events(conn, &loop_state)?;
        if !recent_events.is_empty() {
            sections.push("**What changed since last cycle:**".to_string());
            for evt in recent_events.iter().take(5) {
                sections.push(format!("  - {}", evt));
            }
            sections.push("".to_string());
        }
    }

    let revenue_section = generate_revenue_urgency_section(cfg);
    if !revenue_section.is_empty() {
        sections.push(revenue_section);
    }

    let pulse_section = generate_pulse_section(conn);
    if !pulse_section.is_empty() {
        sections.push(pulse_section);
    }

    if !lean {
        let diag_engine = DiagnosticEngine::new();
        // Phase 0.3: Load persisted histories so persistence-warning and
        // stuck-pattern detection actually fire. Previously these were &[].
        let drive_history = crate::mind::diagnostic::load_drive_history(conn);
        let quadrant_history = crate::mind::diagnostic::load_quadrant_history(conn);
        match diag_engine.analyze(conn, &drive_history, &quadrant_history) {
            Ok(report) => {
                // Record this snapshot for future persistence detection.
                // Best-effort: a failure here doesn't invalidate the report.
                if let Err(e) = crate::mind::diagnostic::record_diagnostic_snapshot(conn, &report) {
                    warn!("Failed to record diagnostic snapshot: {e}");
                }
                let diag_section = diag_engine.diagnostic_prompt_section(&report);
                if !diag_section.is_empty() {
                    sections.push(diag_section);
                }
            }
            Err(e) => warn!("DiagnosticEngine failed: {e}"),
        }

        let feeling_engine = FeelingEngine::new();
        match feeling_engine.generate(conn, &drive_history) {
            Ok(report) => {
                let feeling_section = feeling_state_prompt(&report);
                if !feeling_section.is_empty() {
                    sections.push(feeling_section);
                }
            }
            Err(e) => warn!("FeelingEngine failed: {e}"),
        }

        let social_section = generate_social_terrain_section(conn);
        if !social_section.is_empty() {
            sections.push(social_section);
        }

        let wisdom_signals = detect_wisdom_signals(conn);
        if !wisdom_signals.is_empty() {
            sections.push("## Wisdom Signals".to_string());
            sections.push("".to_string());
            for signal in &wisdom_signals {
                sections.push(format!("- {}", signal));
            }
            sections.push("".to_string());
        }
    }

    sections.push("## 🧩 Active Skills".to_string());
    sections.push("".to_string());

    let skills = if lean {
        Vec::new()
    } else {
        discover_skills_for_terrain(conn)?
    };

    if !skills.is_empty() {
        sections.push("Discovered by terrain relevance:".to_string());
        sections.push("".to_string());
        for s in &skills {
            sections.push(format!("- `{}`", s));
        }
    } else {
        sections.push("No terrain-connected skills found.".to_string());
    }
    sections.push("".to_string());

    sections.push("## 📋 Task Stack".to_string());
    sections.push("".to_string());
    let wm_data = working_memory
        .get("working_memory")
        .unwrap_or(&working_memory);
    let task = wm_data
        .get("current_project")
        .and_then(|v| v.as_str())
        .unwrap_or("No active project");
    sections.push(format!("*Working memory:* {}", task));
    sections.push("".to_string());

    sections.push("## 🌐 Sensory Field — Awareness".to_string());
    sections.push("".to_string());
    sections.push(generate_sensory_field(cfg));
    sections.push("".to_string());

    sections.push("=".repeat(60));
    sections.push("## 🧭 Instruction".to_string());
    sections.push("".to_string());
    sections
        .push("1. **Read the terrain context first** — what's happening matters most".to_string());
    sections.push(
        "2. **Consult the diagnostic dashboard** — use pattern flags as awareness, not constraints"
            .to_string(),
    );
    sections.push(
        "3. **Choose ONE action** based on terrain relevance, not quadrant compliance".to_string(),
    );
    sections.push("4. **Load relevant skills** before executing if needed".to_string());
    sections.push("5. **CREATE TDG nodes** for every observation, person, and action".to_string());
    sections.push(
        "6. **Tag the action** with its natural expression (drive + quadrant) as a post-hoc label"
            .to_string(),
    );
    sections.push("7. **RUN post-execution audit** immediately after execution".to_string());
    sections.push("8. **Save working memory**".to_string());
    sections.push("".to_string());
    sections.push(
        "**You are the navigator. The terrain and the dashboard are your maps.**".to_string(),
    );
    sections.push("".to_string());

    Ok(sections.join("\n"))
}

pub fn write_mind_state_file(
    conn: &Connection,
    cfg: &Config,
    prompt: &str,
    diagnostic_report: &Value,
    terrain: &Value,
) -> TdgResult<()> {
    let wm = load_working_memory(cfg);
    let skills = query_sqlite_skills(conn);
    let constraints = query_sqlite_constraints(conn);

    let feeling_data = FeelingEngine::new()
        .generate(conn, &crate::mind::diagnostic::load_drive_history(conn))
        .map(|report| {
            json!({
                "energy_level": report.energy_level,
                "dominant_drive": report.dominant_drive,
                "dominant_quadrant": report.dominant_quadrant,
                "feelings": report.feelings,
                "blind_drives": report.blind_drives,
                "pathological_drives": report.pathological_drives,
                "stuck_warning": report.stuck_warning,
                "summary": report.summary,
            })
        })
        .unwrap_or_else(|_| json!({"error": "failed to generate feeling report"}));

    let escalation_level = diagnostic_report
        .get("escalation_level")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Phase 0.3: load persisted histories (was &[]).
            let dh = crate::mind::diagnostic::load_drive_history(conn);
            let qh = crate::mind::diagnostic::load_quadrant_history(conn);
            DiagnosticEngine::new()
                .analyze(conn, &dh, &qh)
                .map(|r| format!("{:?}", r.escalation_level).to_lowercase())
                .unwrap_or_else(|_| "soft".to_string())
        });

    let state = json!({
        "feeling": feeling_data,
        "escalation_level": escalation_level,
        "lean_mode": cfg.lean,
        "diagnostic": {
            "pattern_flags": diagnostic_report.get("pattern_flags"),
        },
        "terrain": {
            "active_nodes_by_type": terrain.get("active_nodes_by_type"),
            "highest_relevance_count": terrain.get("highest_relevance_items")
                .and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
        },
        "active_constraints": constraints.iter().take(10)
            .map(|c| format!("{} (confidence: {})",
                c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                c.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0)))
            .collect::<Vec<_>>(),
        "active_skills": skills,
        "projects": wm.get("working_memory").unwrap_or(&wm)
            .get("projects"),
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "_generator": "injector_rust_v1",
        "_prompt_length": prompt.len(),
    });

    // Write to tdg-mind-snapshot.json (NOT tdg-mind-state.json).
    //
    // Previously this wrote to tdg-mind-state.json — the SAME file that
    // MindStateManager writes to with a DIFFERENT schema. The two schemas
    // share zero fields, so whichever writer ran last would destroy the
    // other's data. We now use a separate file (tdg-mind-snapshot.json)
    // for the diagnostic snapshot, leaving tdg-mind-state.json exclusively
    // for MindStateManager's session/working_memory/trust_score data.
    let state_path = cfg.state_dir.join("tdg-mind-snapshot.json");
    std::fs::create_dir_all(&cfg.state_dir)?;
    // G31 fix: atomic write (temp file + rename) to prevent corruption on crash.
    // Use unique filename per process/thread to avoid concurrency collisions.
    let pid = std::process::id();
    let thread_id = format!("{:?}", std::thread::current().id())
        .replace("ThreadId(", "")
        .replace(")", "");
    let tmp_path = cfg.state_dir.join(format!("tdg-mind-snapshot-{pid}-{thread_id}.tmp"));
    
    let res = std::fs::write(&tmp_path, serde_json::to_string_pretty(&state)?);
    if res.is_ok() {
        if let Err(e) = std::fs::rename(&tmp_path, &state_path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(crate::error::TdgError::from(e));
        }
    } else {
        let _ = std::fs::remove_file(&tmp_path);
        res?;
    }
    Ok(())
}

/// Phase 9: Generate a graph-level metabolic health summary.
///
/// Queries the health_json column across all active holons and computes:
/// - Mean G_z (integrative efficiency)
/// - Mean P_z (transcendental tension)
/// - Count of holons in each HealthState (optimal/sub-optimal/collapse/depolarized)
/// - Count of holons with active metabolic inefficiencies (shadows)
/// - Metabolism queue depth (pending jobs)
///
/// This is the "graph-level mind" — it tells the agent the overall metabolic
/// state of its memory, not just per-holon health.
fn generate_metabolic_summary(conn: &Connection) -> TdgResult<String> {
    // Load all health_json values
    let mut stmt = match conn.prepare(
        "SELECT health_json FROM nodes
         WHERE valid_to IS NULL AND health_json IS NOT NULL AND health_json != ''",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(String::new()), // table might not exist yet
    };

    let health_jsons: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .ok()
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    if health_jsons.is_empty() {
        return Ok(String::new());
    }

    let mut g_z_sum = 0.0;
    let mut p_z_sum = 0.0;
    let mut count = 0;
    let mut optimal = 0;
    let mut sub_optimal = 0;
    let mut collapse = 0;
    let mut depolarized = 0;

    for json in &health_jsons {
        if let Ok(health) = serde_json::from_str::<crate::metabolism::health::Health>(json) {
            g_z_sum += health.g_z;
            p_z_sum += health.p_z;
            count += 1;
            match health.state {
                crate::metabolism::health::HealthState::Optimal => optimal += 1,
                crate::metabolism::health::HealthState::SubOptimal => sub_optimal += 1,
                crate::metabolism::health::HealthState::Collapse => collapse += 1,
                crate::metabolism::health::HealthState::Depolarized => depolarized += 1,
            }
        }
    }

    if count == 0 {
        return Ok(String::new());
    }

    let mean_g_z = g_z_sum / count as f64;
    let mean_p_z = p_z_sum / count as f64;

    // Get metabolism queue depth
    let queue_depth: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pending_metabolism WHERE attempts < max_attempts",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let mut summary = String::new();
    summary.push_str(&format!(
        "- **Holons with health data**: {} [status: hypothesis-graded]\n",
        count
    ));
    summary.push_str(&format!(
        "- **Mean G_z** (integrative efficiency): {:.1} / 100\n",
        mean_g_z
    ));
    summary.push_str(&format!(
        "- **Mean P_z** (transcendental tension): {:.1} / 100\n",
        mean_p_z
    ));
    summary.push_str(&format!(
        "- **Health distribution**: optimal={}, sub-optimal={}, collapse={}, depolarized={}\n",
        optimal, sub_optimal, collapse, depolarized
    ));

    if depolarized > 0 {
        summary.push_str(&format!(
            "- ⚠️ **{} holon(s) depolarized** (P_z < 10 — no directional commitment). These holons need catalyst injection to restore tension.\n",
            depolarized
        ));
    }
    if collapse > 0 {
        summary.push_str(&format!(
            "- ⚠️ **{} holon(s) in collapse** (G_z < 30 — severe boundary distortion). These holons need structural repair.\n",
            collapse
        ));
    }

    summary.push_str(&format!(
        "- **Metabolism queue**: {} pending job(s)\n",
        queue_depth
    ));

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_schema, run_migrations};
    use crate::mind::sections::{
        generate_pulse_section, generate_revenue_urgency_section, generate_sensory_field,
        query_sqlite_constraints, query_sqlite_skills,
    };
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn insert_node(conn: &Connection, id: &str, name: &str, node_type: &str) {
        conn.execute(
            "INSERT INTO nodes (id, name, node_type, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, datetime('now', 'subsec'), datetime('now', 'subsec'))",
            rusqlite::params![id, name, node_type, format!("desc for {name}")],
        )
        .unwrap();
    }

    fn make_cfg(tmp: &tempfile::NamedTempFile) -> Config {
        Config::with_db_path(tmp.path().to_path_buf())
    }

    #[test]
    fn generate_prompt_empty_graph() {
        let conn = setup();
        let cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        let prompt = generate_prompt(&conn, &cfg).unwrap();
        assert!(prompt.contains("Terrain Context"));
        assert!(prompt.contains("Instruction"));
        assert!(prompt.contains("Sensory Field"));
    }

    #[test]
    fn generate_prompt_lean_mode() {
        let conn = setup();
        let mut cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        cfg.lean = true;
        let prompt = generate_prompt(&conn, &cfg).unwrap();
        assert!(prompt.contains("LEAN MODE"));
    }

    #[test]
    fn write_mind_state_file_creates_file() {
        let conn = setup();
        let temp_dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::with_db_path(temp_dir.path().join("tdg.db"));
        cfg.state_dir = temp_dir.path().to_path_buf();
        let terrain = json!({"active_nodes_by_type": {}});
        let report = json!({"pattern_flags": []});
        write_mind_state_file(&conn, &cfg, "test prompt", &report, &terrain).unwrap();
        let state_path = cfg.state_dir.join("tdg-mind-snapshot.json");
        assert!(state_path.exists());
    }

    #[test]
    fn generate_prompt_with_populated_graph() {
        let conn = setup();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let cfg = make_cfg(&tmp);

        insert_node(&conn, "n1", "Alpha", "concept");
        insert_node(&conn, "n2", "Beta", "person");
        insert_node(&conn, "n3", "Gamma", "concept");

        let prompt = generate_prompt(&conn, &cfg).unwrap();
        assert!(prompt.contains("## 🌐 Terrain Context"));
        assert!(prompt.contains("## 📋 Task Stack"));
        assert!(prompt.contains("## 🧭 Instruction"));
        assert!(prompt.contains("Active Skills"));
    }

    #[test]
    fn generate_prompt_contains_sensory_field() {
        let conn = setup();
        let cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        let prompt = generate_prompt(&conn, &cfg).unwrap();
        assert!(prompt.contains("Sensory Field"));
        assert!(prompt.contains("## 🌐 Sensory Field"));
    }

    #[test]
    fn generate_prompt_non_lean_shows_events() {
        let conn = setup();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let cfg = make_cfg(&tmp);

        insert_node(&conn, "n1", "Item", "task");

        let prompt = generate_prompt(&conn, &cfg).unwrap();
        assert!(!prompt.contains("LEAN MODE"));
        assert!(prompt.contains("## 🧩 Active Skills"));
    }

    #[test]
    fn generate_prompt_lean_no_recent_events() {
        let conn = setup();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut cfg = make_cfg(&tmp);
        cfg.lean = true;

        let prompt = generate_prompt(&conn, &cfg).unwrap();
        assert!(prompt.contains("LEAN MODE"));
        assert!(prompt.contains("Instruction"));
    }

    #[test]
    fn generate_prompt_task_stack_section() {
        let conn = setup();
        let cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        let prompt = generate_prompt(&conn, &cfg).unwrap();
        assert!(prompt.contains("## 📋 Task Stack"));
        assert!(prompt.contains("Working memory"));
    }

    #[test]
    fn write_mind_state_file_json_valid() {
        let conn = setup();
        let temp_dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::with_db_path(temp_dir.path().join("tdg.db"));
        cfg.state_dir = temp_dir.path().to_path_buf();
        let terrain = json!({"active_nodes_by_type": {"concept": 3}});
        let report = json!({"pattern_flags": ["flag1"]});
        write_mind_state_file(&conn, &cfg, "my prompt", &report, &terrain).unwrap();

        let state_path = cfg.state_dir.join("tdg-mind-snapshot.json");
        let content = std::fs::read_to_string(&state_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["_generator"], "injector_rust_v1");
        assert_eq!(parsed["lean_mode"], false);
        assert!(parsed["generated_at"].is_string());
    }

    #[test]
    fn write_mind_state_file_lean_mode() {
        let conn = setup();
        let temp_dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::with_db_path(temp_dir.path().join("tdg.db"));
        cfg.state_dir = temp_dir.path().to_path_buf();
        let terrain = json!({"active_nodes_by_type": {}});
        let report = json!({"pattern_flags": []});

        write_mind_state_file(&conn, &cfg, "lean prompt", &report, &terrain).unwrap();
        let state_path = cfg.state_dir.join("tdg-mind-snapshot.json");
        let content = std::fs::read_to_string(&state_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["_generator"], "injector_rust_v1");
        assert!(parsed["lean_mode"].is_boolean());
    }

    #[test]
    fn sections_pulse_empty_graph() {
        let conn = setup();
        let pulse = generate_pulse_section(&conn);
        assert!(pulse.is_empty());
    }

    #[test]
    fn sections_pulse_populated_graph() {
        let conn = setup();
        insert_node(&conn, "n1", "A", "concept");
        insert_node(&conn, "n2", "B", "concept");
        insert_node(&conn, "n3", "C", "person");

        let pulse = generate_pulse_section(&conn);
        assert!(pulse.contains("Pulse"));
        assert!(pulse.contains("3 total nodes"));
    }

    #[test]
    fn sections_revenue_urgency() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let cfg = make_cfg(&tmp);
        let section = generate_revenue_urgency_section(&cfg);
        assert!(section.contains("Revenue Urgency"));
        assert!(section.contains("Progress"));
        assert!(section.contains("Recommendation"));
    }

    #[test]
    fn sections_sensory_field() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let cfg = make_cfg(&tmp);
        let field = generate_sensory_field(&cfg);
        assert!(!field.is_empty());
    }

    #[test]
    fn sections_query_skills_empty() {
        let conn = setup();
        let skills = query_sqlite_skills(&conn);
        assert!(skills.is_empty());
    }

    #[test]
    fn sections_query_constraints_empty() {
        let conn = setup();
        let constraints = query_sqlite_constraints(&conn);
        assert!(constraints.is_empty());
    }

    #[test]
    fn generate_prompt_instruction_section() {
        let conn = setup();
        let cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        let prompt = generate_prompt(&conn, &cfg).unwrap();
        assert!(prompt.contains("## 🧭 Instruction"));
        assert!(prompt.contains("Read the terrain context first"));
        assert!(prompt.contains("RUN post-execution audit"));
        assert!(prompt.contains("navigator"));
    }

    #[test]
    fn generate_prompt_skill_section_empty() {
        let conn = setup();
        let cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        let prompt = generate_prompt(&conn, &cfg).unwrap();
        assert!(prompt.contains("No terrain-connected skills found."));
    }

    #[test]
    fn write_mind_state_file_includes_prompt_length() {
        let conn = setup();
        let temp_dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::with_db_path(temp_dir.path().join("tdg.db"));
        cfg.state_dir = temp_dir.path().to_path_buf();
        let terrain = json!({});
        let report = json!({"pattern_flags": []});
        write_mind_state_file(&conn, &cfg, "short", &report, &terrain).unwrap();

        let state_path = cfg.state_dir.join("tdg-mind-snapshot.json");
        let content = std::fs::read_to_string(&state_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["_prompt_length"], 5);
    }

    #[test]
    fn write_mind_state_file_includes_feeling_and_escalation() {
        let conn = setup();
        let temp_dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::with_db_path(temp_dir.path().join("tdg.db"));
        cfg.state_dir = temp_dir.path().to_path_buf();
        let terrain = json!({"active_nodes_by_type": {}});
        let report = json!({"pattern_flags": []});
        write_mind_state_file(&conn, &cfg, "test", &report, &terrain).unwrap();

        let state_path = cfg.state_dir.join("tdg-mind-snapshot.json");
        let content = std::fs::read_to_string(&state_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        let feeling = parsed.get("feeling").expect("feeling field missing");
        assert!(feeling.is_object(), "feeling should be an object");
        assert!(
            feeling.get("energy_level").is_some(),
            "missing energy_level"
        );
        assert!(
            feeling.get("dominant_drive").is_some(),
            "missing dominant_drive"
        );
        assert!(feeling.get("feelings").is_some(), "missing feelings list");
        assert!(feeling.get("summary").is_some(), "missing summary");

        let escalation = parsed
            .get("escalation_level")
            .expect("escalation_level field missing");
        assert!(
            escalation.is_string(),
            "escalation_level should be a string"
        );
        let level = escalation.as_str().unwrap();
        assert!(
            matches!(level, "soft" | "strong" | "mandatory"),
            "invalid escalation_level: {}",
            level
        );
    }
}
