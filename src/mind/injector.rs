//! TDG Mind Injector — terrain-first context block orchestrator
//!
//! Port of `core/mind/injector.py` (498 lines).
//! Generates the full terrain-first context block for LLM consumption.

use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::atomic::AtomicUsize;

use crate::config::Config;
use crate::error::TdgResult;
use crate::mind::data_loader::*;
use crate::mind::sections::*;
use crate::mind::terrain::{discover_skills_for_terrain, generate_terrain_context};

static WISDOM_CALL_COUNTER: AtomicUsize = AtomicUsize::new(0);
const WISDOM_CADENCE: usize = 5;

pub fn generate_prompt(conn: &Connection, cfg: &Config) -> TdgResult<String> {
    let lean = cfg.lean;
    let mut sections = Vec::new();

    let _meta_view = load_meta_view(cfg);
    let working_memory = load_working_memory(cfg);
    let loop_state = load_loop_state(cfg);

    if lean {
        sections.push("╔══════════════════════════════════════╗".to_string());
        sections.push("║  🔋 TDG LEAN MODE — Reduced overhead  ║".to_string());
        sections.push("╚══════════════════════════════════════╝".to_string());
        sections.push("".to_string());
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
    sections.push("8. **Record to MetricsEngine** and save working memory".to_string());
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

    let state = json!({
        "feeling": format!("prompt_length={}", prompt.len()),
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

    let state_path = cfg.state_dir.join("tdg-mind-state.json");
    std::fs::create_dir_all(&cfg.state_dir)?;
    std::fs::write(&state_path, serde_json::to_string_pretty(&state)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_schema, run_migrations};
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
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
        let cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        let terrain = json!({"active_nodes_by_type": {}});
        let report = json!({"pattern_flags": []});
        write_mind_state_file(&conn, &cfg, "test prompt", &report, &terrain).unwrap();
        let state_path = cfg.state_dir.join("tdg-mind-state.json");
        assert!(state_path.exists());
    }
}
