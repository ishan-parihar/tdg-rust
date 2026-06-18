//! TDG Sections — configurable prompt section generators
//!
//! Port of `core/mind/sections.py` (427 lines).
//! Revenue targets and dates are configurable via goals.json instead of hardcoded.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::config::Config;
use crate::mind::data_loader::{load_loop_state, load_working_memory, robust_json_load};
use crate::mind::terrain::count_active_nodes_by_type;

pub struct GoalConfig {
    pub revenue_target: f64,
    pub revenue_currency: String,
    pub target_date: String,
    pub checkpoint_date: String,
    pub checkpoint_label: String,
}

impl Default for GoalConfig {
    fn default() -> Self {
        Self {
            revenue_target: 1000.0,
            revenue_currency: "₹".to_string(),
            target_date: "2026-06-30".to_string(),
            checkpoint_date: "2026-05-20".to_string(),
            checkpoint_label: "May 20".to_string(),
        }
    }
}

impl GoalConfig {
    pub fn load(cfg: &Config) -> Self {
        let path = cfg.config_dir().join("goals.json");
        let data = robust_json_load(&path, json!({}));
        Self {
            revenue_target: data
                .get("revenue_target")
                .and_then(|v| v.as_f64())
                .unwrap_or(1000.0),
            revenue_currency: data
                .get("revenue_currency")
                .and_then(|v| v.as_str())
                .unwrap_or("₹")
                .to_string(),
            target_date: data
                .get("target_date")
                .and_then(|v| v.as_str())
                .unwrap_or("2026-06-30")
                .to_string(),
            checkpoint_date: data
                .get("checkpoint_date")
                .and_then(|v| v.as_str())
                .unwrap_or("2026-05-20")
                .to_string(),
            checkpoint_label: data
                .get("checkpoint_label")
                .and_then(|v| v.as_str())
                .unwrap_or("May 20")
                .to_string(),
        }
    }
}

pub fn generate_revenue_urgency_section(cfg: &Config) -> String {
    let goals = GoalConfig::load(cfg);
    let now = chrono::Utc::now().date_naive();

    let target_date = chrono::NaiveDate::parse_from_str(&goals.target_date, "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::NaiveDate::from_ymd_opt(2026, 6, 30).unwrap());
    let checkpoint_date = chrono::NaiveDate::parse_from_str(&goals.checkpoint_date, "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::NaiveDate::from_ymd_opt(2026, 5, 20).unwrap());

    let days_to_target = (target_date - now).num_days();
    let days_to_checkpoint = (checkpoint_date - now).num_days();

    let wm = load_working_memory(cfg);
    let wm_data = wm.get("working_memory").unwrap_or(&wm);
    let revenue = wm_data
        .get("revenue_generated")
        .or_else(|| wm_data.get("total_revenue"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let progress_pct = (revenue / goals.revenue_target * 100.0).min(100.0) as usize;
    let bar_len: usize = 20;
    let filled = (bar_len * progress_pct / 100).min(bar_len);
    let bar = format!("{}{}", "▓".repeat(filled), "░".repeat(bar_len - filled));

    let mut lines = vec![
        "".to_string(),
        "=".repeat(60),
        "## 💰 Revenue Urgency Pulse".to_string(),
        "".to_string(),
        format!(
            "**Progress:** {}{:.0} / {}{:.0} target",
            goals.revenue_currency, revenue, goals.revenue_currency, goals.revenue_target
        ),
        format!("**Bar:** [{}] {}%", bar, progress_pct),
        format!("**Days to {}:** {}", goals.target_date, days_to_target),
        format!(
            "**Days to {} checkpoint:** {}",
            goals.checkpoint_label,
            days_to_checkpoint.max(0)
        ),
        "".to_string(),
    ];

    let mut constraint_status = String::new();
    if days_to_checkpoint <= 0 {
        constraint_status = format!(
            "⚠️ {} CHECKPOINT ACTIVE — Emergency action required",
            goals.checkpoint_label
        );
    } else if days_to_checkpoint <= 2 {
        constraint_status = format!(
            "🔴 {} checkpoint in {}d — REVENUE ACTION MANDATORY",
            goals.checkpoint_label, days_to_checkpoint
        );
    } else if days_to_checkpoint <= 7 {
        constraint_status = format!(
            "🟡 {} checkpoint in {}d — prioritize revenue actions",
            goals.checkpoint_label, days_to_checkpoint
        );
    }

    if days_to_target <= 0 {
        constraint_status = format!(
            "🚨 {} DEADLINE PASSED — Full protocol review triggered",
            goals.target_date
        );
    } else if days_to_target <= 7 && revenue < goals.revenue_target {
        constraint_status = format!(
            "🚨 {}d to {} — {}{:.0} short of target. CRITICAL.",
            days_to_target,
            goals.target_date,
            goals.revenue_currency,
            goals.revenue_target - revenue
        );
    } else if days_to_target <= 14 && revenue < goals.revenue_target * 0.5 {
        constraint_status = format!(
            "🔴 {}d to {} — need {}{:.0} more. Hustle mode.",
            days_to_target,
            goals.target_date,
            goals.revenue_currency,
            goals.revenue_target - revenue
        );
    }

    if !constraint_status.is_empty() {
        lines.push(constraint_status);
        lines.push("".to_string());
    }

    if revenue == 0.0 && days_to_target <= 14 {
        lines.push("**Recommendation:** Execute a revenue action NOW.".to_string());
    } else if revenue < goals.revenue_target * 0.25 && days_to_target <= 30 {
        lines.push("**Recommendation:** Heavy revenue push needed.".to_string());
    } else if revenue < goals.revenue_target * 0.5 && days_to_target <= 20 {
        lines.push(
            "**Recommendation:** Moderate push. At least one revenue action per cycle.".to_string(),
        );
    } else {
        lines.push(
            "**Recommendation:** Stay on course — revenue actions remain priority #1.".to_string(),
        );
    }

    lines.push("".to_string());
    lines.join("\n")
}

pub fn generate_pulse_section(conn: &Connection) -> String {
    let mut lines = Vec::new();
    let type_counts = count_active_nodes_by_type(conn).unwrap_or_default();

    let mut sorted: Vec<_> = type_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let total: i64 = sorted.iter().map(|(_, c)| c).sum();
    if total == 0 {
        return String::new();
    }

    lines.push("## 📊 Pulse — Structural Gaps".to_string());
    lines.push("".to_string());
    lines.push(format!(
        "**Graph composition:** {} total nodes across {} types",
        total,
        sorted.len()
    ));
    lines.push("".to_string());

    for (ntype, count) in sorted.iter().take(5) {
        let bar_len = 10;
        let filled = ((*count as f64 / total as f64 * bar_len as f64) as usize).min(bar_len);
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_len - filled));
        lines.push(format!("  {} {}: {}", bar, ntype, count));
    }

    lines.join("\n")
}

pub fn generate_sensory_field(cfg: &Config) -> String {
    let wm = load_working_memory(cfg);
    let wm_data = wm.get("working_memory").unwrap_or(&wm);
    let project = wm_data
        .get("current_project")
        .and_then(|v| v.as_str())
        .unwrap_or("No active project");
    let phase = wm_data
        .get("current_phase")
        .and_then(|v| v.as_str())
        .unwrap_or("No current phase");
    let loop_state = load_loop_state(cfg);
    let last_action = loop_state
        .get("last_action")
        .and_then(|v| v.as_str())
        .unwrap_or("No prior action");

    format!(
        "Project: {} · Phase: {} · Last action: {}",
        project,
        phase,
        truncate_str(last_action, 200)
    )
}

pub fn query_sqlite_skills(conn: &Connection) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT DISTINCT name FROM nodes
         WHERE node_type = 'skill' AND valid_to IS NULL
           AND name IS NOT NULL
         ORDER BY name LIMIT 20",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = stmt.query_map([], |row| row.get::<_, String>(0));
    match rows {
        Ok(r) => r.filter_map(|r| r.ok()).filter(|s| !s.is_empty()).collect(),
        Err(_) => Vec::new(),
    }
}

pub fn query_sqlite_constraints(conn: &Connection) -> Vec<Value> {
    let mut stmt = match conn.prepare(
        "SELECT name, description, confidence FROM nodes
         WHERE node_type = 'constraint' AND valid_to IS NULL
         ORDER BY confidence DESC LIMIT 20",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "name": row.get::<_, String>(0)?,
            "description": row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            "confidence": row.get::<_, f64>(2)?,
        }))
    });

    match rows {
        Ok(r) => r.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

pub fn generate_social_terrain_section(conn: &Connection) -> String {
    let people_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE node_type = 'people' AND valid_to IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let communication_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE node_type = 'communication' AND valid_to IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if people_count == 0 && communication_count == 0 {
        return String::new();
    }

    format!(
        "## Social Terrain\n- People nodes: {}\n- Communications: {}",
        people_count, communication_count
    )
}

pub fn detect_wisdom_signals(conn: &Connection) -> Vec<String> {
    let mut signals = Vec::new();

    let duplicates: Vec<String> = conn
        .prepare_cached(
            "SELECT name, COUNT(*) as cnt FROM nodes 
             WHERE valid_to IS NULL AND node_type = 'observation'
             GROUP BY name HAVING cnt > 2",
        )
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok(format!("'{}' repeated {} times", name, count))
            })?;
            Ok(rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    signals.extend(duplicates);
    signals
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len])
    }
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
    fn revenue_urgency_section_uses_goals() {
        let cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        let section = generate_revenue_urgency_section(&cfg);
        assert!(section.contains("Revenue Urgency Pulse"));
        assert!(section.contains("target"));
    }

    #[test]
    fn pulse_section_empty_graph() {
        let conn = setup();
        let section = generate_pulse_section(&conn);
        assert!(section.is_empty());
    }

    #[test]
    fn sensory_field_default() {
        let cfg = Config::with_db_path(
            tempfile::NamedTempFile::new()
                .unwrap()
                .into_temp_path()
                .to_path_buf(),
        );
        let field = generate_sensory_field(&cfg);
        assert!(field.contains("Project:"));
        assert!(field.contains("Phase:"));
    }

    #[test]
    fn query_skills_empty() {
        let conn = setup();
        let skills = query_sqlite_skills(&conn);
        assert!(skills.is_empty());
    }

    #[test]
    fn query_constraints_empty() {
        let conn = setup();
        let constraints = query_sqlite_constraints(&conn);
        assert!(constraints.is_empty());
    }

    #[test]
    fn goal_config_defaults() {
        let goals = GoalConfig::default();
        assert_eq!(goals.revenue_target, 1000.0);
        assert_eq!(goals.revenue_currency, "₹");
    }
}
