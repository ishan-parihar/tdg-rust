use std::collections::HashMap;

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::TdgResult;
use crate::models::{Edge, NewEdge, NewNode, Node, NodeQuery};

/// Generate a node ID: "n" + 12 hex chars from UUID v4.
fn gen_node_id() -> String {
    let uuid = Uuid::new_v4();
    let hex = uuid.as_simple().to_string();
    format!("n{}", &hex[..12])
}

/// Generate an edge ID: "e" + 12 hex chars from UUID v4.
fn gen_edge_id() -> String {
    let uuid = Uuid::new_v4();
    let hex = uuid.as_simple().to_string();
    format!("e{}", &hex[..12])
}

/// Current ISO 8601 timestamp.
pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ─── Node CRUD ───────────────────────────────────────────────────────────────

/// Create a new node. Returns the created node.
pub fn add_node(conn: &Connection, new: &NewNode) -> TdgResult<Node> {
    let id = gen_node_id();
    let now = now_iso();
    let node_type = new.node_type.clone();
    let name = new.name.clone();
    let description = new.description.clone().unwrap_or_default();
    let properties = new
        .properties
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());
    let quadrants = new
        .quadrants
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());
    let drives = new
        .drives
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());
    let lifecycle_state = new
        .lifecycle_state
        .clone()
        .unwrap_or_else(|| "active".to_string());
    let teleological_level = new.teleological_level.clone();
    let developmental_stage = new.developmental_stage;
    let confidence = new.confidence.unwrap_or(1.0);
    let source = new.source.clone().unwrap_or_default();
    let parent_ids = new
        .parent_ids
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()))
        .unwrap_or_else(|| "[]".to_string());
    let agent_id = new.agent_id.clone();

    // Compute agent_path from parent_ids
    let agent_path = compute_agent_path(conn, &parent_ids)?;

    conn.execute(
        "INSERT INTO nodes (id, node_type, name, description, properties_json, quadrants_json,
         drives_json, lifecycle_state, teleological_level, developmental_stage, confidence,
         source, parent_ids, agent_path, created_at, updated_at, valid_from, agent_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            id,
            node_type,
            name,
            description,
            properties,
            quadrants,
            drives,
            lifecycle_state,
            teleological_level,
            developmental_stage,
            confidence,
            source,
            parent_ids,
            agent_path,
            now,
            now,
            now,
            agent_id,
        ],
    )?;

    get_node(conn, &id)?.ok_or_else(|| {
        crate::error::TdgError::Custom("Failed to retrieve created node".to_string())
    })
}

/// Get a single node by ID. Returns `None` if not found or soft-deleted.
pub fn get_node(conn: &Connection, node_id: &str) -> TdgResult<Option<Node>> {
    let mut stmt = conn.prepare(
        "SELECT id, node_type, name, description, properties_json, quadrants_json, drives_json,
         lifecycle_state, teleological_level, developmental_stage, confidence, source,
         parent_ids, agent_path, created_at, updated_at, valid_from, valid_to,
         helpful_count, retrieval_count, agent_id
         FROM nodes WHERE id = ?1 AND valid_to IS NULL",
    )?;

    let mut rows = stmt.query_map(params![node_id], row_to_node)?;

    match rows.next() {
        Some(Ok(node)) => Ok(Some(node)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Get a node including soft-deleted ones (for internal use).
pub fn get_node_including_deleted(conn: &Connection, node_id: &str) -> TdgResult<Option<Node>> {
    let mut stmt = conn.prepare(
        "SELECT id, node_type, name, description, properties_json, quadrants_json, drives_json,
         lifecycle_state, teleological_level, developmental_stage, confidence, source,
         parent_ids, agent_path, created_at, updated_at, valid_from, valid_to,
         helpful_count, retrieval_count, agent_id
         FROM nodes WHERE id = ?1",
    )?;

    let mut rows = stmt.query_map(params![node_id], row_to_node)?;

    match rows.next() {
        Some(Ok(node)) => Ok(Some(node)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Update a node with the given fields. Only provided fields are updated.
pub fn update_node(
    conn: &Connection,
    node_id: &str,
    updates: &HashMap<String, serde_json::Value>,
) -> TdgResult<Option<Node>> {
    if updates.is_empty() {
        return get_node(conn, node_id);
    }

    let now = now_iso();
    let mut set_clauses = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    for (key, value) in updates {
        match key.as_str() {
            "name" | "description" | "lifecycle_state" | "teleological_level" | "source"
            | "agent_path" | "agent_id" => {
                set_clauses.push(format!("{key} = ?{idx}"));
                param_values.push(Box::new(value.as_str().unwrap_or("").to_string()));
                idx += 1;
            }
            "developmental_stage" => {
                set_clauses.push(format!("{key} = ?{idx}"));
                param_values.push(Box::new(value.as_i64().unwrap_or(0)));
                idx += 1;
            }
            "confidence" => {
                set_clauses.push(format!("{key} = ?{idx}"));
                param_values.push(Box::new(value.as_f64().unwrap_or(1.0)));
                idx += 1;
            }
            "properties_json" | "quadrants_json" | "drives_json" | "parent_ids" => {
                set_clauses.push(format!("{key} = ?{idx}"));
                param_values.push(Box::new(value.to_string()));
                idx += 1;
            }
            _ => continue,
        }
    }

    if set_clauses.is_empty() {
        return get_node(conn, node_id);
    }

    // Add updated_at
    set_clauses.push(format!("updated_at = ?{idx}"));
    param_values.push(Box::new(now.clone()));
    idx += 1;

    // node_id goes at the NEXT index
    let sql = format!(
        "UPDATE nodes SET {} WHERE id = ?{idx} AND valid_to IS NULL",
        set_clauses.join(", ")
    );

    param_values.push(Box::new(node_id.to_string()));

    let mut stmt = conn.prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();
    stmt.execute(params_ref.as_slice())?;

    get_node(conn, node_id)
}

/// Soft-delete a node (set valid_to).
pub fn delete_node(conn: &Connection, node_id: &str) -> TdgResult<bool> {
    let now = now_iso();
    let affected = conn.execute(
        "UPDATE nodes SET valid_to = ?1 WHERE id = ?2 AND valid_to IS NULL",
        params![now, node_id],
    )?;

    if affected > 0 {
        conn.execute(
            "UPDATE edges SET valid_to = ?1 WHERE (source_id = ?2 OR target_id = ?2) AND valid_to IS NULL",
            params![now, node_id],
        )?;
    }

    Ok(affected > 0)
}

/// Hard-delete a node (actually remove from DB).
pub fn hard_delete_node(conn: &Connection, node_id: &str) -> TdgResult<bool> {
    conn.execute(
        "DELETE FROM edges WHERE source_id = ?1 OR target_id = ?1",
        params![node_id],
    )?;
    conn.execute(
        "DELETE FROM embeddings WHERE node_id = ?1",
        params![node_id],
    )?;
    let affected = conn.execute("DELETE FROM nodes WHERE id = ?1", params![node_id])?;
    Ok(affected > 0)
}

// ─── Edge CRUD ───────────────────────────────────────────────────────────────

/// Create a new edge. Returns the created edge.
pub fn add_edge(conn: &Connection, new: &NewEdge) -> TdgResult<Edge> {
    let id = gen_edge_id();
    let now = now_iso();
    let weight = new.weight.unwrap_or(1.0);
    let properties = new
        .properties
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());
    let agent_id = new.agent_id.clone();

    conn.execute(
        "INSERT INTO edges (id, source_id, target_id, edge_type, weight, properties_json,
         valid_from, created_at, agent_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            new.source_id,
            new.target_id,
            new.edge_type,
            weight,
            properties,
            now,
            now,
            agent_id,
        ],
    )?;

    // Auto-update parent_ids if DECOMPOSES_TO edge
    if new.edge_type == "DECOMPOSES_TO" {
        update_parent_ids_on_decompose(conn, &new.target_id)?;
    }

    get_edge(conn, &id)?.ok_or_else(|| {
        crate::error::TdgError::Custom("Failed to retrieve created edge".to_string())
    })
}

/// Get a single edge by ID.
pub fn get_edge(conn: &Connection, edge_id: &str) -> TdgResult<Option<Edge>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_id, target_id, edge_type, weight, properties_json,
         valid_from, valid_to, created_at, updated_at, agent_id
         FROM edges WHERE id = ?1 AND valid_to IS NULL",
    )?;

    let mut rows = stmt.query_map(params![edge_id], row_to_edge)?;

    match rows.next() {
        Some(Ok(edge)) => Ok(Some(edge)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Get edges by filter criteria.
pub fn get_edges(
    conn: &Connection,
    source_id: Option<&str>,
    target_id: Option<&str>,
    edge_type: Option<&str>,
    agent_id: Option<&str>,
    limit: i64,
) -> TdgResult<Vec<Edge>> {
    let mut conditions = vec!["e.valid_to IS NULL".to_string()];
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(sid) = source_id {
        conditions.push(format!("e.source_id = ?{idx}"));
        param_values.push(Box::new(sid.to_string()));
        idx += 1;
    }
    if let Some(tid) = target_id {
        conditions.push(format!("e.target_id = ?{idx}"));
        param_values.push(Box::new(tid.to_string()));
        idx += 1;
    }
    if let Some(et) = edge_type {
        conditions.push(format!("e.edge_type = ?{idx}"));
        param_values.push(Box::new(et.to_string()));
        idx += 1;
    }
    if let Some(aid) = agent_id {
        conditions.push(format!("e.agent_id = ?{idx}"));
        param_values.push(Box::new(aid.to_string()));
        idx += 1;
    }

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        "SELECT e.id, e.source_id, e.target_id, e.edge_type, e.weight, e.properties_json,
         e.valid_from, e.valid_to, e.created_at, e.updated_at, e.agent_id
         FROM edges e WHERE {where_clause} ORDER BY e.created_at DESC LIMIT ?{idx}"
    );

    param_values.push(Box::new(limit));

    let mut stmt = conn.prepare(&sql)?;
    let all_params: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(all_params.as_slice(), row_to_edge)?;

    let mut edges = Vec::new();
    for row in rows {
        edges.push(row?);
    }
    Ok(edges)
}

/// Soft-delete an edge.
pub fn delete_edge(conn: &Connection, edge_id: &str) -> TdgResult<bool> {
    let now = now_iso();
    let affected = conn.execute(
        "UPDATE edges SET valid_to = ?1 WHERE id = ?2 AND valid_to IS NULL",
        params![now, edge_id],
    )?;
    Ok(affected > 0)
}

/// Update edge weight and/or properties.
pub fn update_edge(
    conn: &Connection,
    edge_id: &str,
    weight: Option<f64>,
    properties: Option<&serde_json::Value>,
) -> TdgResult<Option<Edge>> {
    let now = now_iso();
    let mut set_clauses = vec!["updated_at = ?1".to_string()];
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(now)];
    let mut idx = 2;

    if let Some(w) = weight {
        set_clauses.push(format!("weight = ?{idx}"));
        param_values.push(Box::new(w));
        idx += 1;
    }

    if let Some(p) = properties {
        set_clauses.push(format!("properties_json = ?{idx}"));
        param_values.push(Box::new(p.to_string()));
        idx += 1;
    }

    // edge_id goes at the NEXT index
    let sql = format!(
        "UPDATE edges SET {} WHERE id = ?{idx} AND valid_to IS NULL",
        set_clauses.join(", ")
    );

    param_values.push(Box::new(edge_id.to_string()));

    let mut stmt = conn.prepare(&sql)?;
    let all_params: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    stmt.execute(all_params.as_slice())?;

    get_edge(conn, edge_id)
}

// ─── Batch Operations ────────────────────────────────────────────────────────

/// Batch-insert nodes in a single transaction. Returns created nodes.
pub fn add_nodes_batch(conn: &Connection, nodes: &[NewNode]) -> TdgResult<Vec<Node>> {
    let tx = conn.unchecked_transaction()?;
    let now = now_iso();
    let mut ids = Vec::new();

    for new in nodes {
        let id = gen_node_id();
        let node_type = &new.node_type;
        let name = &new.name;
        let description = new.description.as_deref().unwrap_or("");
        let properties = new
            .properties
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());
        let quadrants = new
            .quadrants
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());
        let drives = new
            .drives
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());
        let lifecycle_state = new.lifecycle_state.as_deref().unwrap_or("active");
        let teleological_level = new.teleological_level.as_deref();
        let developmental_stage = new.developmental_stage;
        let confidence = new.confidence.unwrap_or(1.0);
        let source = new.source.as_deref().unwrap_or("");
        let parent_ids = new
            .parent_ids
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or_else(|| "[]".to_string());
        let agent_id = new.agent_id.as_deref();

        tx.execute(
            "INSERT OR IGNORE INTO nodes (id, node_type, name, description, properties_json,
             quadrants_json, drives_json, lifecycle_state, teleological_level, developmental_stage,
             confidence, source, parent_ids, agent_path, created_at, updated_at, valid_from, agent_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                id,
                node_type,
                name,
                description,
                properties,
                quadrants,
                drives,
                lifecycle_state,
                teleological_level,
                developmental_stage,
                confidence,
                source,
                parent_ids,
                "", // agent_path computed post-commit
                now,
                now,
                now,
                agent_id,
            ],
        )?;

        ids.push(id);
    }

    tx.commit()?;

    for id in &ids {
        if let Some(node) = get_node_including_deleted(conn, id)? {
            let parent_ids_json =
                serde_json::to_string(&node.parent_ids).unwrap_or_else(|_| "[]".to_string());
            let computed_path = compute_agent_path(conn, &parent_ids_json)?;
            conn.execute(
                "UPDATE nodes SET agent_path = ?1 WHERE id = ?2",
                params![computed_path, id],
            )?;
        }
    }

    // Return created nodes by ID
    let mut result = Vec::new();
    for id in &ids {
        if let Some(node) = get_node(conn, id)? {
            result.push(node);
        }
    }
    Ok(result)
}

/// Batch-insert edges in a single transaction.
pub fn add_edges_batch(conn: &Connection, edges: &[NewEdge]) -> TdgResult<Vec<Edge>> {
    let tx = conn.unchecked_transaction()?;
    let now = now_iso();
    let mut ids = Vec::new();

    for new in edges {
        let id = gen_edge_id();
        let weight = new.weight.unwrap_or(1.0);
        let properties = new
            .properties
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());
        let agent_id = new.agent_id.as_deref();

        tx.execute(
            "INSERT INTO edges (id, source_id, target_id, edge_type, weight, properties_json,
             valid_from, created_at, agent_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                new.source_id,
                new.target_id,
                new.edge_type,
                weight,
                properties,
                now,
                now,
                agent_id,
            ],
        )?;

        ids.push(id);
    }

    tx.commit()?;

    let mut result = Vec::new();
    for id in &ids {
        if let Some(edge) = get_edge(conn, id)? {
            result.push(edge);
        }
    }
    Ok(result)
}

// ─── Count Queries ───────────────────────────────────────────────────────────

/// Count active nodes, optionally filtered by type.
pub fn count_nodes(conn: &Connection, node_type: Option<&str>) -> TdgResult<i64> {
    match node_type {
        Some(nt) => conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL AND node_type = ?1",
            params![nt],
            |row| row.get(0),
        ),
        None => conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE valid_to IS NULL",
            [],
            |row| row.get(0),
        ),
    }
    .map_err(|e| e.into())
}

/// Count active edges, optionally filtered by type.
pub fn count_edges(conn: &Connection, edge_type: Option<&str>) -> TdgResult<i64> {
    match edge_type {
        Some(et) => conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL AND edge_type = ?1",
            params![et],
            |row| row.get(0),
        ),
        None => conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL",
            [],
            |row| row.get(0),
        ),
    }
    .map_err(|e| e.into())
}

// ─── Event Recording ─────────────────────────────────────────────────────────

/// Record an event in the event store.
pub fn record_event(
    conn: &Connection,
    event_action: &str,
    node_id: Option<&str>,
    source_id: Option<&str>,
    target_id: Option<&str>,
    payload: Option<&serde_json::Value>,
) -> TdgResult<String> {
    let event_id = Uuid::new_v4().as_simple().to_string();
    let now = now_iso();
    let payload_str = payload.map(|p| p.to_string());

    conn.execute(
        "INSERT INTO events (event_id, event_action, timestamp, node_id, source_id, target_id, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![event_id, event_action, now, node_id, source_id, target_id, payload_str],
    )?;

    Ok(event_id)
}

// ─── Phase 3: Query Engine ──────────────────────────────────────────────────

/// Query nodes with filters. Matches Python `query_nodes()`.
pub fn query_nodes(conn: &Connection, query: &NodeQuery) -> TdgResult<Vec<Node>> {
    let mut conditions = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if !query.include_deleted {
        conditions.push("valid_to IS NULL".to_string());
    }

    if let Some(ref nt) = query.node_type {
        conditions.push(format!("node_type = ?{idx}"));
        param_values.push(Box::new(nt.clone()));
        idx += 1;
    }
    if let Some(ref ls) = query.lifecycle_state {
        conditions.push(format!("lifecycle_state = ?{idx}"));
        param_values.push(Box::new(ls.clone()));
        idx += 1;
    }
    if let Some(ref src) = query.source {
        conditions.push(format!("source = ?{idx}"));
        param_values.push(Box::new(src.clone()));
        idx += 1;
    }
    if let Some(ref tl) = query.teleological_level {
        conditions.push(format!("teleological_level = ?{idx}"));
        param_values.push(Box::new(tl.clone()));
        idx += 1;
    }
    if let Some(ds) = query.developmental_stage {
        conditions.push(format!("developmental_stage = ?{idx}"));
        param_values.push(Box::new(ds));
        idx += 1;
    }
    if let Some(ref aid) = query.agent_id {
        conditions.push(format!("agent_id = ?{idx}"));
        param_values.push(Box::new(aid.clone()));
        idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let limit = query.limit.unwrap_or(100).min(crate::validation::MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);

    let sql = format!(
        "SELECT id, node_type, name, description, properties_json, quadrants_json, drives_json,
         lifecycle_state, teleological_level, developmental_stage, confidence, source,
         parent_ids, agent_path, created_at, updated_at, valid_from, valid_to,
         helpful_count, retrieval_count, agent_id
         FROM nodes {where_clause}
         ORDER BY created_at DESC
         LIMIT ?{idx} OFFSET ?{}",
        idx + 1
    );

    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let mut stmt = conn.prepare(&sql)?;
    let all_params: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(all_params.as_slice(), row_to_node)?;

    let mut nodes = Vec::new();
    for row in rows {
        nodes.push(row?);
    }
    Ok(nodes)
}

/// Full-text search using FTS5. Matches Python `search()`.
pub fn search(conn: &Connection, query: &str, limit: i64) -> TdgResult<Vec<(Node, f64)>> {
    let limit = limit.min(crate::validation::MAX_LIMIT);

    let sql = "
        SELECT n.id, n.node_type, n.name, n.description, n.properties_json, n.quadrants_json,
               n.drives_json, n.lifecycle_state, n.teleological_level, n.developmental_stage,
               n.confidence, n.source, n.parent_ids, n.agent_path, n.created_at, n.updated_at,
               n.valid_from, n.valid_to, n.helpful_count, n.retrieval_count, n.agent_id,
               rank
        FROM nodes_fts fts
        JOIN nodes n ON fts.rowid = n.rowid
        WHERE nodes_fts MATCH ?1 AND n.valid_to IS NULL
        ORDER BY rank
        LIMIT ?2
    ";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![query, limit], |row| {
        let rank: f64 = row.get(21)?;
        let node = row_to_node(row)?;
        Ok((node, rank))
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }

    // Fallback to LIKE search if FTS returns nothing
    if results.is_empty() {
        let like_sql = "
            SELECT id, node_type, name, description, properties_json, quadrants_json,
                   drives_json, lifecycle_state, teleological_level, developmental_stage,
                   confidence, source, parent_ids, agent_path, created_at, updated_at,
                   valid_from, valid_to, helpful_count, retrieval_count, agent_id
            FROM nodes
            WHERE valid_to IS NULL AND (name LIKE ?1 OR description LIKE ?2)
            ORDER BY created_at DESC
            LIMIT ?3
        ";
        let pattern = format!("%{query}%");
        let mut stmt = conn.prepare(like_sql)?;
        let rows = stmt.query_map(params![pattern, pattern, limit], row_to_node)?;
        for row in rows {
            results.push((row?, 0.0));
        }
    }

    Ok(results)
}

/// Brute-force cosine similarity search. Matches Python `search_similar()`.
pub fn search_similar(
    conn: &Connection,
    query_vector: &[f32],
    limit: usize,
) -> TdgResult<Vec<(Node, f64)>> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.node_type, n.name, n.description, n.properties_json, n.quadrants_json,
         n.drives_json, n.lifecycle_state, n.teleological_level, n.developmental_stage,
         n.confidence, n.source, n.parent_ids, n.agent_path, n.created_at, n.updated_at,
         n.valid_from, n.valid_to, n.helpful_count, n.retrieval_count, n.agent_id,
         e.vector
         FROM nodes n
         JOIN embeddings e ON n.id = e.node_id
         WHERE n.valid_to IS NULL",
    )?;

    let _dim = query_vector.len() as f32;
    let query_norm: f32 = query_vector.iter().map(|x| x * x).sum::<f32>().sqrt();

    let rows = stmt.query_map([], |row| {
        let node = row_to_node(row)?;
        let vector_blob: Option<Vec<u8>> = row.get(21)?;
        Ok((node, vector_blob))
    })?;

    let mut scored: Vec<(Node, f64)> = Vec::new();

    for row in rows {
        let (node, vector_blob) = row?;
        if let Some(blob) = vector_blob {
            let embedding = deserialize_embedding(&blob);
            if embedding.len() == query_vector.len() {
                // Cosine similarity
                let dot: f32 = query_vector
                    .iter()
                    .zip(embedding.iter())
                    .map(|(a, b)| a * b)
                    .sum();
                let emb_norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

                if query_norm > 0.0 && emb_norm > 0.0 {
                    let cosine = (dot / (query_norm * emb_norm)) as f64;
                    scored.push((node, cosine));
                }
            }
        }
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    Ok(scored)
}

/// BFS shortest path. Matches Python `pathfind()`.
pub fn pathfind(
    conn: &Connection,
    source_id: &str,
    target_id: &str,
    max_depth: i64,
    max_edges: i64,
) -> TdgResult<Vec<Vec<String>>> {
    use std::collections::{HashSet, VecDeque};

    let mut all_paths = Vec::new();
    let mut queue: VecDeque<(String, Vec<String>)> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();

    queue.push_back((source_id.to_string(), vec![source_id.to_string()]));
    visited.insert(source_id.to_string());

    let mut edges_traversed = 0;

    while let Some((current, path)) = queue.pop_front() {
        if path.len() as i64 > max_depth {
            continue;
        }
        if edges_traversed >= max_edges {
            break;
        }

        if current == target_id {
            all_paths.push(path.clone());
            if all_paths.len() >= 5 {
                break;
            }
            continue;
        }

        // Get neighbors via active edges
        let neighbors = get_neighbor_ids(conn, &current)?;
        edges_traversed += neighbors.len() as i64;

        for neighbor in neighbors {
            if !visited.contains(&neighbor) {
                visited.insert(neighbor.clone());
                let mut new_path = path.clone();
                new_path.push(neighbor.clone());
                queue.push_back((neighbor, new_path));
            }
        }
    }

    Ok(all_paths)
}

/// Get outgoing neighbor IDs for a node.
fn get_neighbor_ids(conn: &Connection, node_id: &str) -> TdgResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT target_id FROM edges WHERE source_id = ?1 AND valid_to IS NULL
         UNION
         SELECT source_id FROM edges WHERE target_id = ?1 AND valid_to IS NULL",
    )?;

    let neighbors = stmt
        .query_map(params![node_id], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(neighbors)
}

/// Export a subgraph centered on a node. Matches Python `node_graph()`.
pub fn node_graph(
    conn: &Connection,
    node_id: &str,
    depth: i64,
    max_nodes: i64,
) -> TdgResult<serde_json::Value> {
    use std::collections::{HashSet, VecDeque};

    let mut nodes = Vec::new();
    let mut edge_ids_seen = HashSet::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back((node_id.to_string(), 0));
    visited.insert(node_id.to_string());

    while let Some((current, current_depth)) = queue.pop_front() {
        if current_depth >= depth || nodes.len() as i64 >= max_nodes {
            continue;
        }

        if let Some(node) = get_node(conn, &current)? {
            // Get outgoing edges
            let edges = get_edges(conn, Some(&current), None, None, None, 100)?;
            for edge in &edges {
                if !edge_ids_seen.contains(&edge.id) {
                    edge_ids_seen.insert(edge.id.clone());
                    if !visited.contains(&edge.target_id) {
                        visited.insert(edge.target_id.clone());
                        queue.push_back((edge.target_id.clone(), current_depth + 1));
                    }
                }
            }

            // Get incoming edges
            let in_edges = get_edges(conn, None, Some(&current), None, None, 100)?;
            for edge in &in_edges {
                if !edge_ids_seen.contains(&edge.id) {
                    edge_ids_seen.insert(edge.id.clone());
                    if !visited.contains(&edge.source_id) {
                        visited.insert(edge.source_id.clone());
                        queue.push_back((edge.source_id.clone(), current_depth + 1));
                    }
                }
            }

            nodes.push(serde_json::to_value(&node).unwrap_or(serde_json::json!({})));
        }
    }

    // Collect all edges between visited nodes
    let mut all_edges = Vec::new();
    let node_ids: HashSet<String> = nodes
        .iter()
        .filter_map(|n| n.get("id")?.as_str().map(|s| s.to_string()))
        .collect();

    for nid in &node_ids {
        let out_edges = get_edges(conn, Some(nid), None, None, None, 200)?;
        for edge in out_edges {
            if node_ids.contains(&edge.target_id) {
                all_edges.push(serde_json::to_value(&edge).unwrap_or(serde_json::json!({})));
            }
        }
    }

    Ok(serde_json::json!({
        "center": node_id,
        "nodes": nodes,
        "edges": all_edges,
    }))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Convert a SQLite row to a Node.
pub(crate) fn row_to_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    let properties_json: String = row.get(4)?;
    let quadrants_json: String = row.get(5)?;
    let drives_json: String = row.get(6)?;
    let parent_ids_json: String = row.get(12)?;

    let properties: serde_json::Value =
        serde_json::from_str(&properties_json).unwrap_or(serde_json::json!({}));
    let quadrants: serde_json::Value =
        serde_json::from_str(&quadrants_json).unwrap_or(serde_json::json!({}));
    let drives: serde_json::Value =
        serde_json::from_str(&drives_json).unwrap_or(serde_json::json!({}));
    let parent_ids: Vec<String> = serde_json::from_str(&parent_ids_json).unwrap_or_default();

    Ok(Node {
        id: row.get(0)?,
        node_type: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        properties,
        quadrants,
        drives,
        lifecycle_state: row.get(7)?,
        teleological_level: row.get(8)?,
        developmental_stage: row.get(9)?,
        confidence: row.get(10)?,
        source: row.get(11)?,
        parent_ids,
        agent_path: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        valid_from: row.get(16)?,
        valid_to: row.get(17)?,
        helpful_count: row.get(18)?,
        retrieval_count: row.get(19)?,
        agent_id: row.get(20)?,
    })
}

/// Convert a SQLite row to an Edge.
fn row_to_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<Edge> {
    let properties_json: String = row.get(5)?;

    let properties: serde_json::Value =
        serde_json::from_str(&properties_json).unwrap_or(serde_json::json!({}));

    Ok(Edge {
        id: row.get(0)?,
        source_id: row.get(1)?,
        target_id: row.get(2)?,
        edge_type: row.get(3)?,
        weight: row.get(4)?,
        properties,
        valid_from: row.get(6)?,
        valid_to: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        agent_id: row.get(10)?,
    })
}

/// Compute agent_path from parent_ids.
fn compute_agent_path(conn: &Connection, parent_ids_json: &str) -> TdgResult<String> {
    let parent_ids: Vec<String> = serde_json::from_str(parent_ids_json).unwrap_or_default();

    if parent_ids.is_empty() {
        return Ok(String::new());
    }

    let parent_id = &parent_ids[0];
    let path: Option<String> = conn
        .query_row(
            "SELECT agent_path FROM nodes WHERE id = ?1",
            params![parent_id],
            |row| row.get(0),
        )
        .ok();

    match path {
        Some(p) if !p.is_empty() => Ok(format!("{p}/{parent_id}")),
        _ => Ok(format!("/{parent_id}")),
    }
}

/// Update parent_ids when a DECOMPOSES_TO edge is created.
/// Merges with existing parent_ids instead of replacing.
fn update_parent_ids_on_decompose(conn: &Connection, target_id: &str) -> TdgResult<()> {
    // Get existing parent_ids
    let existing: Option<String> = conn
        .query_row(
            "SELECT parent_ids FROM nodes WHERE id = ?1",
            params![target_id],
            |row| row.get(0),
        )
        .ok();
    let mut all_parents: Vec<String> = existing
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // Get all DECOMPOSES_TO source IDs for this target
    let mut stmt = conn.prepare(
        "SELECT source_id FROM edges WHERE target_id = ?1 AND edge_type = 'DECOMPOSES_TO' AND valid_to IS NULL",
    )?;

    let source_ids: Vec<String> = stmt
        .query_map(params![target_id], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Merge: add any missing source_ids
    for sid in &source_ids {
        if !all_parents.contains(sid) {
            all_parents.push(sid.clone());
        }
    }

    let parent_ids = serde_json::to_string(&all_parents).unwrap_or_else(|_| "[]".to_string());
    let now = now_iso();

    conn.execute(
        "UPDATE nodes SET parent_ids = ?1, updated_at = ?2 WHERE id = ?3",
        params![parent_ids, now, target_id],
    )?;

    let agent_path = compute_agent_path(conn, &parent_ids)?;
    conn.execute(
        "UPDATE nodes SET agent_path = ?1 WHERE id = ?2",
        params![agent_path, target_id],
    )?;

    Ok(())
}

/// Serialize f32 vector to bytes.
pub fn serialize_embedding(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Deserialize bytes to f32 vector.
pub fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_fts, init_schema, run_migrations};

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_fts(&conn).unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn make_node(node_type: &str, name: &str) -> NewNode {
        NewNode {
            node_type: node_type.to_string(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn add_and_get_node() {
        let conn = setup_db();
        let node = add_node(&conn, &make_node("observation", "Test")).unwrap();
        assert_eq!(node.node_type, "observation");
        assert!(node.id.starts_with('n'));

        let fetched = get_node(&conn, &node.id).unwrap().unwrap();
        assert_eq!(fetched.id, node.id);
    }

    #[test]
    fn test_update_node() {
        let conn = setup_db();
        let node = add_node(&conn, &make_node("telos", "Original")).unwrap();

        let mut updates = HashMap::new();
        updates.insert("name".to_string(), serde_json::json!("Updated"));
        let updated = update_node(&conn, &node.id, &updates).unwrap().unwrap();
        assert_eq!(updated.name, "Updated");
    }

    #[test]
    fn delete_node_soft() {
        let conn = setup_db();
        let node = add_node(&conn, &make_node("action", "Delete Me")).unwrap();
        assert!(delete_node(&conn, &node.id).unwrap());
        assert!(get_node(&conn, &node.id).unwrap().is_none());
        assert!(get_node_including_deleted(&conn, &node.id)
            .unwrap()
            .is_some());
    }

    #[test]
    fn test_hard_delete_node() {
        let conn = setup_db();
        let node = add_node(&conn, &make_node("artifact", "Gone")).unwrap();
        assert!(hard_delete_node(&conn, &node.id).unwrap());
        assert!(get_node(&conn, &node.id).unwrap().is_none());
        assert!(get_node_including_deleted(&conn, &node.id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn add_and_get_edge() {
        let conn = setup_db();
        let n1 = add_node(&conn, &make_node("telos", "Parent")).unwrap();
        let n2 = add_node(&conn, &make_node("action", "Child")).unwrap();

        let edge = add_edge(
            &conn,
            &NewEdge {
                source_id: n1.id.clone(),
                target_id: n2.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(edge.source_id, n1.id);
        assert_eq!(edge.edge_type, "DECOMPOSES_TO");
    }

    #[test]
    fn get_edges_filtered() {
        let conn = setup_db();
        let n1 = add_node(&conn, &make_node("telos", "A")).unwrap();
        let n2 = add_node(&conn, &make_node("action", "B")).unwrap();

        add_edge(
            &conn,
            &NewEdge {
                source_id: n1.id.clone(),
                target_id: n2.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let edges = get_edges(&conn, Some(&n1.id), None, None, None, 100).unwrap();
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn test_delete_edge() {
        let conn = setup_db();
        let n1 = add_node(&conn, &make_node("telos", "X")).unwrap();
        let n2 = add_node(&conn, &make_node("action", "Y")).unwrap();

        let edge = add_edge(
            &conn,
            &NewEdge {
                source_id: n1.id.clone(),
                target_id: n2.id.clone(),
                edge_type: "ENABLES".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(delete_edge(&conn, &edge.id).unwrap());
        assert!(get_edges(&conn, Some(&n1.id), None, None, None, 100)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn batch_nodes_and_edges() {
        let conn = setup_db();

        let nodes: Vec<NewNode> = (0..5)
            .map(|i| make_node("observation", &format!("Batch {i}")))
            .collect();

        let created = add_nodes_batch(&conn, &nodes).unwrap();
        assert_eq!(created.len(), 5);
        assert_eq!(count_nodes(&conn, Some("observation")).unwrap(), 5);

        let edges: Vec<NewEdge> = created
            .windows(2)
            .map(|w| NewEdge {
                source_id: w[0].id.clone(),
                target_id: w[1].id.clone(),
                edge_type: "RELATES_TO".to_string(),
                ..Default::default()
            })
            .collect();

        let created_edges = add_edges_batch(&conn, &edges).unwrap();
        assert_eq!(created_edges.len(), 4);
    }

    #[test]
    fn count_queries() {
        let conn = setup_db();
        assert_eq!(count_nodes(&conn, None).unwrap(), 0);

        add_node(&conn, &make_node("telos", "T1")).unwrap();
        add_node(&conn, &make_node("action", "A1")).unwrap();

        assert_eq!(count_nodes(&conn, None).unwrap(), 2);
        assert_eq!(count_nodes(&conn, Some("telos")).unwrap(), 1);
        assert_eq!(count_nodes(&conn, Some("action")).unwrap(), 1);
    }

    #[test]
    fn record_event_test() {
        let conn = setup_db();
        let event_id = record_event(
            &conn,
            "test_action",
            Some("n_test"),
            None,
            None,
            Some(&serde_json::json!({"key": "value"})),
        )
        .unwrap();
        assert!(!event_id.is_empty());
    }

    #[test]
    fn auto_parent_ids_on_decompose() {
        let conn = setup_db();
        let parent = add_node(&conn, &make_node("telos", "Parent")).unwrap();
        let child = add_node(&conn, &make_node("action", "Child")).unwrap();

        add_edge(
            &conn,
            &NewEdge {
                source_id: parent.id.clone(),
                target_id: child.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let updated = get_node(&conn, &child.id).unwrap().unwrap();
        assert_eq!(updated.parent_ids, vec![parent.id.clone()]);
    }

    // Phase 3 tests
    #[test]
    fn query_nodes_by_type() {
        let conn = setup_db();
        add_node(&conn, &make_node("telos", "T1")).unwrap();
        add_node(&conn, &make_node("telos", "T2")).unwrap();
        add_node(&conn, &make_node("action", "A1")).unwrap();

        let q = NodeQuery {
            node_type: Some("telos".to_string()),
            ..Default::default()
        };
        let results = query_nodes(&conn, &q).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|n| n.node_type == "telos"));
    }

    #[test]
    fn search_fts() {
        let conn = setup_db();
        add_node(&conn, &make_node("observation", "Rust memory safety")).unwrap();
        add_node(&conn, &make_node("observation", "Python GIL limitations")).unwrap();

        let results = search(&conn, "Rust", 10).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].0.name.contains("Rust"));
    }

    #[test]
    fn pathfind_test() {
        let conn = setup_db();
        let a = add_node(&conn, &make_node("telos", "A")).unwrap();
        let b = add_node(&conn, &make_node("action", "B")).unwrap();
        let c = add_node(&conn, &make_node("action", "C")).unwrap();

        add_edge(
            &conn,
            &NewEdge {
                source_id: a.id.clone(),
                target_id: b.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        add_edge(
            &conn,
            &NewEdge {
                source_id: b.id.clone(),
                target_id: c.id.clone(),
                edge_type: "DEPENDS_ON".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let paths = pathfind(&conn, &a.id, &c.id, 5, 100).unwrap();
        assert!(!paths.is_empty());
        assert_eq!(paths[0], vec![a.id.clone(), b.id.clone(), c.id.clone()]);
    }

    #[test]
    fn node_graph_test() {
        let conn = setup_db();
        let root = add_node(&conn, &make_node("telos", "Root")).unwrap();
        let child = add_node(&conn, &make_node("action", "Child")).unwrap();

        add_edge(
            &conn,
            &NewEdge {
                source_id: root.id.clone(),
                target_id: child.id.clone(),
                edge_type: "DECOMPOSES_TO".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

        let graph = node_graph(&conn, &root.id, 2, 10).unwrap();
        assert!(graph.get("nodes").is_some());
        assert!(graph.get("edges").is_some());
    }

    #[test]
    fn embedding_roundtrip() {
        let vec = vec![0.1, 0.2, -0.3, 0.0];
        let bytes = serialize_embedding(&vec);
        let recovered = deserialize_embedding(&bytes);
        assert_eq!(vec.len(), recovered.len());
        for (a, b) in vec.iter().zip(recovered.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
