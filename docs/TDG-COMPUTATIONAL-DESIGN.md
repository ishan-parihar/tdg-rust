# TDG Computational Design — How Holonic Operations Actually Run

**Companion to:** `HOLONIC-SCIENCE-AUDIT-AND-REFACTOR-PLAN.md`
**Question answered:** How do holonic operations (lesser cycle, greater cycle, attractor field, resonance, phase transitions) execute *computationally* within TDG such that the system remains efficient at scale AND effective as the mind/memory infrastructure for AI agents?
**Status:** Design doc, ready for implementation planning

---

## 0. The Problem Statement

The audit identified 20 holonic primitives tdg-rust needs to embody. But embodying them naively — ticking a lesser cycle on every holon every 60 seconds, recomputing attractor fields on every read, scanning the graph for resonance partners — produces a system that is **correct in theory and unusable in practice**. At 100K nodes with a 60s heartbeat, you burn 100K ticks/minute of CPU for a graph where 95% of holons are dormant at any moment. Attractor field recomputation on every read turns a 2ms lookup into a 50ms scan. Resonance queries become O(N²).

This document specifies the computational strategy that makes holonic operations both **efficient** (sub-10ms agent-facing reads, sub-100ms async metabolism, no full-graph scans in the hot path) and **effective** (the system actually behaves as a mind — it metabolises, integrates, and feeds back, rather than just storing and reporting).

The core move is a shift from **time-driven to event-driven computation**, organised into **three tiers** with different latency budgets, and unified by a single agent-facing API contract (the ContextPack).

---

## 1. The Core Insight — Events, Not Clocks

A naive reading of holonic theory suggests a heartbeat: every holon runs its lesser cycle on a timer. This is wrong, for three reasons:

1. **It's not how real holons work.** A cell doesn't metabolise on a metronome — it metabolises when substrate arrives. A neuron doesn't fire on a schedule — it fires when threshold is reached. Holonic metabolism is *response to perturbation*, not *obedience to a clock*.
2. **It wastes 95% of the computation.** In a healthy graph, most holons are dormant at any moment — they've integrated their last catalyst and are waiting. Ticking them anyway burns CPU for no state change.
3. **It breaks locality.** A holon's lesser cycle depends on its own state plus incoming catalyst (edges). If nothing has touched it, there's nothing to compute. The timer model forces global coordination; the event model preserves locality.

**The computational model is event-driven.** A holon's lesser cycle ticks when one of four events occurs:

| Event | Trigger | Source |
|---|---|---|
| **Catalyst injection** | A new edge is created touching this holon | `tdg_connect`, `tdg_observe` (entity wiring) |
| **Upward pressure** | A child's accumulated Experience crosses a threshold | Child's lesser-cycle tick |
| **Downward pressure** | A parent's Transformation fires | Parent's greater-cycle tick |
| **Explicit tick** | `tdg_tick <id>` MCP call (for testing/debugging) | Agent or admin |

This means dormant holons pay zero CPU. Active holons metabolise immediately on perturbation. The graph is reactive, not periodic.

### 1.1 What "Tick" Actually Computes

A lesser-cycle tick is **not** "advance the state machine by one phase." A tick is "process the pending catalyst and transition if thresholds are crossed." Concretely:

```
tick(holon, event):
  1. Read pending_catalyst (queued since last tick)
  2. If pending_catalyst == 0 and phase == Dormant: return (no-op)
  3. Apply the metabolic step for current phase:
     - Ingesting: add catalyst to Matrix reservoir
     - ProcessingSkewed/Integrated: Matrix processes catalyst → stores Experience
                                 Potentiator processes Experience → stores latent Catalyst
     - Integrating: diagnose shadows, update attractor field dirty flag
     - Quiescent: reset for next cycle
     - Dormant: if catalyst > threshold, transition to Ingesting
  4. Check transition conditions; transition if met
  5. If Experience crossed upward-pressure threshold: enqueue tick(child) for each parent
  6. If transition was to Integrating: mark attractor_field dirty, health dirty
  7. Persist state + emit events
```

Steps 1–7 are O(1) per holon (constant work, no graph traversal). The only graph traversal is step 5, which is O(parent_count) — typically 1–3.

### 1.2 The Greater Cycle Is Also Event-Driven (But Different)

The greater cycle is **discontinuous/ratcheting** — it fires when transformation pressure exceeds the Significator's threshold. This is inherently event-driven: the event is "pressure crossed threshold," not "timer fired."

```
greater_tick(holon, event):
  1. Read transformation_pressure (accumulated from lesser-cycle Experience)
  2. If pressure < threshold and phase == SignificatorStable: return (no-op)
  3. If pressure >= threshold: transition to TransformationPreCrucible
  4. Run phase machine (guarded transitions per HoloOS state_machine.py)
  5. If reached ChoiceLocked: commit choice, reset pressure, fire downward event to children
  6. If stage advanced: update developmental_stage, invalidate ContextPack cache
```

The greater cycle runs **on a slower cadence** — not because of a timer, but because transformation pressure accumulates slowly (it's the integral of lesser-cycle Experience over time). In practice, greater-cycle ticks fire 100–1000x less often than lesser-cycle ticks.

---

## 2. The Three-Tier Computation Model

Every holonic operation falls into one of three tiers, each with a different latency budget, concurrency model, and consistency requirement. Mixing tiers (e.g. doing tier-2 work synchronously in a tier-1 write) is the primary cause of performance regressions.

### Tier 1 — Synchronous (the Write Path)

**Latency budget:** < 10ms per operation
**Concurrency:** serialized via WriteGuard (existing)
**Consistency:** strong — the operation must be durable before returning

**What runs here:**
- Node/edge CRUD (`tdg_create`, `tdg_connect`, `tdg_observe`, `tdg_update`)
- Provenance event append (`mutation_log`, `events`)
- FTS5 index update (via triggers — already async-safe in WAL mode)
- Trust score adjustments
- Synthesis submission (write to `_Agent_Drafts/`, set status = `AiDraft`)
- 5-Gate Validation (it's a read-only check, but blocks submission)

**What does NOT run here:**
- Lesser-cycle ticks (async — see Tier 2)
- Attractor field recomputation (async)
- Health recomputation (async)
- Resonance graph updates (async)
- Greater-cycle ticks (scheduled — see Tier 3)
- Mind pipeline runs (scheduled)

**The pattern:** Tier 1 writes the *intent* (the node, the edge) and enqueues *metabolic work* (a dirty flag or a job queue entry). The agent gets a fast acknowledgement. The metabolism catches up in Tier 2.

```rust
// Tier 1 write path (synchronous, < 10ms)
pub fn observe(conn: &Connection, desc: &str, ...) -> TdgResult<ObservationResult> {
    let tx = conn.transaction()?;
    
    // 1. Create the observation node
    let node = add_node(&tx, ...)?;
    
    // 2. Wire entities (MENTIONS edges)
    for entity in entities { upsert_entity_and_connect(&tx, &node.id, entity)?; }
    
    // 3. Append provenance event
    record_mutation(&tx, "create", "node", &node.id, None, &node.to_json(), agent_id)?;
    
    // 4. Enqueue metabolic work (Tier 2) — NON-BLOCKING
    enqueue_catalyst_injection(&tx, &node.id, catalyst_amount)?;
    // This inserts a row into `pending_metabolism` table:
    //   (holon_id, event_type, payload, enqueued_at, priority)
    // A background worker picks it up (see Tier 2).
    
    tx.commit()?;
    
    // 5. Optional: trigger digestion cascade (also Tier 2, enqueued)
    if trigger_digestion {
        enqueue_digestion_cascade(&conn, &node.id)?;
    }
    
    Ok(ObservationResult { observation_id: node.id, ... })
}
```

**Key invariant:** Tier 1 never blocks on Tier 2/3 work. If the metabolism is slow, the agent still gets fast writes. The graph may be temporarily "metabolically stale" (attractor field not yet recomputed) but it is never *inconsistent* (the node exists, the edges exist, provenance is logged).

### Tier 2 — Asynchronous Metabolism (the Hot Background Path)

**Latency budget:** < 100ms per job
**Concurrency:** parallel worker pool (e.g. 4–8 threads)
**Consistency:** eventual — typically converges within 1 second of the triggering write

**What runs here:**
- Lesser-cycle ticks (catalyst injection, upward pressure)
- Attractor field recomputation (when dirty flag set)
- Health recomputation (G_z, P_z — when attractor field changes)
- Resonance graph incremental updates (when attractor field changes)
- Digestion cascade (hypothesis creation from observation clusters)
- Reflect engine (skill creation from observation clustering)
- Enricher (embedding backfill, drive enrichment, stage enrichment)

**The job queue:**

```sql
-- New table: pending_metabolism
CREATE TABLE pending_metabolism (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    holon_id TEXT NOT NULL,
    job_type TEXT NOT NULL,        -- 'lesser_tick', 'recompute_attractor', 'recompute_health', 'resonance_update', 'digestion_cascade', 'reflect', 'enrich'
    payload TEXT,                  -- JSON
    enqueued_at TEXT NOT NULL,
    priority INTEGER DEFAULT 0,    -- 0=low, 1=normal, 2=high (e.g. agent-requested)
    attempts INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 3,
    FOREIGN KEY (holon_id) REFERENCES nodes(id)
);
CREATE INDEX idx_pending_holon ON pending_metabolism(holon_id);
CREATE INDEX idx_pending_priority ON pending_metabolism(priority DESC, enqueued_at);
```

**The worker pool:**

```rust
// src/metabolism/worker.rs
pub struct MetabolismWorker {
    pool: DbPool,
    num_workers: usize,  // default 4, configurable via TDG_METABOLISM_WORKERS
}

impl MetabolismWorker {
    pub async fn run(self) {
        let mut handles = Vec::new();
        for _ in 0..self.num_workers {
            handles.push(tokio::spawn(self.clone().worker_loop()));
        }
        futures::future::join_all(handles).await;
    }
    
    async fn worker_loop(self) {
        loop {
            // Claim a job (atomic via SQLite UPDATE...RETURNING or a lease)
            let job = match self.claim_job() {
                Some(j) => j,
                None => { tokio::time::sleep(Duration::from_millis(100)).await; continue; }
            };
            
            // Execute with timeout (100ms budget)
            let result = tokio::time::timeout(
                Duration::from_millis(100),
                self.execute_job(&job)
            ).await;
            
            match result {
                Ok(Ok(())) => self.mark_done(&job),
                Ok(Err(e)) => self.mark_failed(&job, e),
                Err(_) => self.mark_timeout(&job),  // re-enqueue with attempts++
            }
        }
    }
    
    fn execute_job(&self, job: &Job) -> TdgResult<()> {
        match job.job_type.as_str() {
            "lesser_tick" => self.run_lesser_tick(&job.holon_id, &job.payload),
            "recompute_attractor" => self.recompute_attractor(&job.holon_id),
            "recompute_health" => self.recompute_health(&job.holon_id),
            "resonance_update" => self.update_resonance(&job.holon_id),
            "digestion_cascade" => self.run_digestion_cascade(&job.holon_id),
            "reflect" => self.run_reflect(),
            "enrich" => self.run_enrich(),
            _ => Err(TdgError::UnknownJobType(job.job_type.clone())),
        }
    }
}
```

**Job coalescing:** If multiple catalyst injections arrive for the same holon within a short window, coalesce them into a single tick. The `pending_metabolism` table supports this: claim all pending jobs for a holon atomically, sum the catalyst, run one tick.

```sql
-- Coalesce: claim all pending lesser_tick jobs for a holon
DELETE FROM pending_metabolism
WHERE holon_id = ?1 AND job_type = 'lesser_tick'
RETURNING payload;
-- Application code sums the catalyst_amount from each payload, runs one tick.
```

**Backpressure:** If the queue depth exceeds a threshold (e.g. 10K jobs), the Tier 1 write path starts rejecting non-essential writes (e.g. `tdg_observe` returns a "metabolism saturated" error, but `tdg_create` still succeeds). This prevents unbounded queue growth under load.

### Tier 3 — Scheduled Integration (the Cold Background Path)

**Latency budget:** < 10 seconds per run
**Concurrency:** single-threaded (sequential to avoid contention)
**Consistency:** weak — runs every N minutes, may skip runs if overloaded

**What runs here:**
- Greater-cycle ticks (every 5 min — but see §1.2, they're really event-driven; the schedule is just a sweep for holons whose pressure crossed threshold but weren't enqueued)
- Mind pipeline integration pass (every 15 min — reads graph-level state, diagnoses graph-level shadows, feeds catalyst back to specific holons)
- Resonance graph full rebuild (every 1 hour — corrects incremental drift)
- SelfManager cycle (every 6 hours — existing: HealthMonitor → Janitor → Enricher → Archiver)
- Stage advancement sweep (every 1 hour — checks all holons for evidence/age gate eligibility)

**The scheduler:**

```rust
// src/metabolism/scheduler.rs
pub struct Scheduler {
    schedules: Vec<Schedule>,
}

struct Schedule {
    name: &'static str,
    interval: Duration,
    last_run: Instant,
    job: fn(&DbPool) -> TdgResult<()>,
    skip_if_overloaded: bool,
}

impl Scheduler {
    pub async fn run(mut self, pool: DbPool) {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            let now = Instant::now();
            for sched in &mut self.schedules {
                if now.duration_since(sched.last_run) >= sched.interval {
                    if sched.skip_if_overloaded && self.queue_depth() > 10_000 {
                        tracing::warn!("Skipping {} due to backlog", sched.name);
                        continue;
                    }
                    if let Err(e) = (sched.job)(&pool) {
                        tracing::error!("Schedule {} failed: {}", sched.name, e);
                    }
                    sched.last_run = now;
                }
            }
        }
    }
}
```

**The mind pipeline as a Tier 3 schedule:** This is the key redesign. The mind pipeline (consolidation, diagnostic, feeling, pulse) currently runs on-demand via `tdg_context`. In the new model, it runs as a scheduled Tier 3 job every 15 minutes:

1. Reads graph-level state (distributions of G_z, P_z, attractor polarities, shadow diagnoses)
2. Diagnoses graph-level shadows (e.g. "the graph is in dark-addiction — too many observations, not enough integration")
3. **Feeds catalyst back** to specific holons (the integration step — this is what makes it a metabolism, not a dashboard)

```
mind_pipeline_run():
  1. Compute graph-level health:
     - mean G_z, mean P_z, % holons in sinkhole, % in collapse
     - drive distribution skew, quadrant imbalance
     - orphan ratio, edge density
  2. Diagnose graph-level shadows:
     - DarkAddiction: observation_count / integration_count > 10
     - DarkAllergy: observation_count / integration_count < 0.1 (starvation)
     - GoldenAddiction: hypothesis_count / evidence_count > 5 (speculation)
     - GoldenAllergy: hypothesis_count < 0.01 * observation_count (no emergence)
  3. Select target holons for catalyst injection:
     - For DarkAddiction: inject catalyst into Integrating-phase holons (force them to process)
     - For DarkAllergy: inject catalyst into Dormant holons connected to active regions
     - For GoldenAddiction: inject catalyst into high-resonance pairs (force bonding)
     - For GoldenAllergy: inject catalyst into observation clusters (force hypothesis creation)
  4. Enqueue catalyst injections (Tier 2 jobs) for selected holons
  5. Write graph-level mind_state snapshot (for tdg_mind_state queries)
```

**This is the closed loop.** The mind pipeline reads graph state, diagnoses, and feeds back as catalyst. The catalyst triggers lesser-cycle ticks, which update attractor fields, which update health, which the next mind pipeline run reads. The system metabolises at the graph level, not just per-holon.

---

## 3. The Data Path — Write, Read, Metabolism

### 3.1 The Write Path (agent writes an observation)

```
Agent → tdg_observe(desc, entities, ...)
  ↓ Tier 1 (synchronous, < 10ms)
  1. Validate input (schema, length limits)
  2. BEGIN IMMEDIATE transaction
  3. Insert observation node (synthesis_status = AiDraft)
  4. Upsert entities, create MENTIONS edges
  5. Append mutation_log, events
  6. FTS5 trigger fires (async-safe in WAL)
  7. Enqueue Tier 2 jobs:
     - lesser_tick(observation_id, catalyst=injection)  [priority=normal]
     - lesser_tick(each entity_id, catalyst=mention)    [priority=low]
     - recompute_attractor(observation_id)              [priority=normal]
     - recompute_health(observation_id)                 [priority=normal]
     - resonance_update(observation_id)                 [priority=low]
     - digestion_cascade(observation_id)                [priority=low, delay=30s]
  8. COMMIT
  9. Return {observation_id, entities_connected, ...}
  
  ↓ Tier 2 (asynchronous, < 1s convergence)
  Worker picks up lesser_tick(observation_id):
    - Observation is in Dormant phase (just created)
    - Catalyst > threshold → transition to Ingesting
    - Process catalyst → store Experience
    - Transition to Integrating → diagnose shadows
    - Mark attractor_field dirty, health dirty
    - If Experience > upward_pressure_threshold:
        enqueue lesser_tick(parent_id, catalyst=upward_pressure)
  Worker picks up recompute_attractor(observation_id):
    - Read lesser_cycle_state, drives, edges
    - Compute A_M, A_P, A_G, Γ
    - Compute π, type_class, choice_flag
    - Write to attractor_field_json column
    - Enqueue recompute_health(observation_id)
    - Enqueue resonance_update(observation_id) [if type_class changed]
  Worker picks up recompute_health(observation_id):
    - Read attractor_field, lesser_cycle_state
    - Compute G_z, P_z, total, state classification
    - Write to health_json column
  Worker picks up resonance_update(observation_id):
    - Compute R(observation, top-K candidates by type_class complementarity)
    - Update resonance_graph table (incremental)
  Worker picks up digestion_cascade (after 30s delay):
    - Check if 3+ observations share MENTIONS entity
    - If so, create hypothesis, create SUPPORTS edges
    - Enqueue lesser_tick(hypothesis_id, catalyst=creation)
  
  ↓ Tier 3 (scheduled, eventually consistent)
  Next mind_pipeline_run (within 15 min):
    - Reads updated graph state
    - Diagnoses graph-level shadows
    - May inject catalyst into observation_id (if it's a target)
```

**Total time to "fully metabolised":** < 1 second for the per-holon metabolism (Tier 2). < 15 minutes for the graph-level integration (Tier 3). The agent never waits for either — it gets a fast acknowledgement at Tier 1.

### 3.2 The Read Path (agent fetches context)

```
Agent → tdg_fetch_context(holon_id, scope, depth, token_budget)
  ↓ Cache check
  1. Check context_cache table for (holon_id, scope, depth, token_budget)
  2. If cached and fresh (within TTL — default 5 min): return cached
  3. If cached but stale: recompute (see below), update cache, return
  
  ↓ Cold path (cache miss or stale)
  4. Load holon identity (1 query)
  5. If scope includes "intra":
     - Load attractor_field_json (1 query, JSON column)
     - Load health_json (1 query, JSON column)
     - Load lesser_cycle_json (1 query)
     - Load greater_cycle_json (1 query)
     - Load archetypal_loads (from attractor_field)
     - Load drives_json, quadrants_json (1 query)
  6. If scope includes "inter":
     - Load edges (1 query, filtered by edge_type)
     - Load top-5 resonances from resonance_graph table (1 query, indexed)
     - Load bridges (1 query)
  7. If scope includes "extra":
     - Load parent_chain (recursive query, max depth 5)
     - Load sub_holons (1 query via DECOMPOSES_TO edges)
     - Load great_way (from greater_cycle_state + parent_chain)
  8. If depth >= 3:
     - Load analogues (cross-domain type_class homologues, max 10) (1 query)
     - Load provenance summary (last 5 events) (1 query)
  9. Assemble ContextPack struct
  10. Render to markdown with [status: {synthesis_status}] tags
  11. Apply token-budget truncation (drop cheapest-to-lose first, NEVER drop synthesis_status/grounding/type_class)
  12. Write to context_cache (with TTL)
  13. Return
  
  ↓ Total queries: 6–10 (depending on scope/depth)
  ↓ Total latency: < 20ms cached, < 100ms cold
```

**Cache invalidation:** Any Tier 1 write to a holon invalidates its context_cache entry (and the entries of holons within 1 hop, since inter/extra context may reference them). This is cheap — a single `DELETE FROM context_cache WHERE holon_id IN (...)`.

### 3.3 The Metabolism Path (the closed loop)

This is the path that turns TDG from a database into a mind. It's the combination of Tier 2 (per-holon metabolism) and Tier 3 (graph-level integration).

```
                    Agent
                      ↓
            ┌─── tdg_observe ───┐
            │                   │
            ↓                   ↓
       Tier 1 write      tdg_fetch_context
       (synchronous)     (cached read)
            │                   ↑
            ↓                   │
       pending_metabolism   context_cache
            │                   ↑
            ↓                   │
       Tier 2 workers ───→ recompute ───→ attractor_field
            │                                health
            ↓                                resonance_graph
       lesser_cycle ticks                    │
       attractor updates                     │
       health updates                        │
            │                                │
            ↓                                │
       ┌────┴────┐                           │
       │         │                           │
       ↓         ↓                           │
  upward    downward                     (agent reads
  pressure  pressure                      updated state
       │         │                       on next fetch)
       ↓         ↓
  parent     child
  lesser     lesser
  cycle      cycle
  tick       tick
       │
       ↓
  (accumulates into
   transformation_pressure)
       │
       ↓
  Tier 3 scheduler (every 5 min)
  greater-cycle sweep:
    - For each holon with pressure > threshold:
      - Run greater-cycle tick
      - If Transformation fires: 
        - Update Significator
        - Reset pressure
        - Enqueue downward pressure to children
        - Maybe advance stage
        - Invalidate context_cache
       │
       ↓
  Tier 3 scheduler (every 15 min)
  mind pipeline run:
    - Read graph-level state (from cached health/attractor fields)
    - Diagnose graph-level shadows
    - Select target holons
    - Enqueue catalyst injections (Tier 2 jobs)
       │
       ↓
  (catalyst triggers lesser-cycle ticks → the loop continues)
```

The loop is: **agent writes → per-holon metabolises → graph integrates → catalyst feeds back → per-holon metabolises again → agent reads updated state.** This is a metabolism. The "mind" is not a module — it's the Tier 3 integration pass that closes the circuit.

---

## 4. Caching and Materialization Strategy

The efficiency of the system depends on never recomputing anything that hasn't changed. The strategy:

### 4.1 The Dirty-Flag Pattern

Every computed field has a dirty flag. The flag is set by Tier 1 writes; cleared by Tier 2 recomputation.

```sql
-- Add to nodes table (Migration Phase 10)
ALTER TABLE nodes ADD COLUMN attractor_field_dirty INTEGER DEFAULT 1;
ALTER TABLE nodes ADD COLUMN health_dirty INTEGER DEFAULT 1;
ALTER TABLE nodes ADD COLUMN lesser_cycle_json TEXT;
ALTER TABLE nodes ADD COLUMN greater_cycle_json TEXT;
ALTER TABLE nodes ADD COLUMN attractor_field_json TEXT;
ALTER TABLE nodes ADD COLUMN health_json TEXT;
```

**What sets the dirty flags:**
- `attractor_field_dirty = 1`: any write to `drives_json`, `quadrants_json`, `lesser_cycle_json`, `greater_cycle_json`, or edge creation/deletion touching this holon
- `health_dirty = 1`: any change to `attractor_field_json` or `lesser_cycle_json`

**What clears them:**
- Tier 2 worker recomputes the field, writes the JSON, sets dirty = 0

### 4.2 The ContextCache Table

```sql
CREATE TABLE context_cache (
    holon_id TEXT NOT NULL,
    scope TEXT NOT NULL,           -- 'intra', 'inter', 'extra', 'intra+inter', 'intra+inter+extra', 'analogues'
    depth INTEGER NOT NULL,
    token_budget INTEGER NOT NULL,
    context_pack_json TEXT NOT NULL,
    rendered_markdown TEXT NOT NULL,
    computed_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    PRIMARY KEY (holon_id, scope, depth, token_budget)
);
CREATE INDEX idx_context_expires ON context_cache(expires_at);
```

**TTL:** 5 minutes (configurable via `TDG_CONTEXT_CACHE_TTL_SECS`).

**Invalidation:** 
- Any Tier 1 write to holon H: `DELETE FROM context_cache WHERE holon_id = H`
- Any Tier 1 write creating an edge (H1, H2): delete cache for both H1 and H2
- Tier 3 mind pipeline run: delete cache for all holons that received catalyst injection

**Memory bound:** If the cache exceeds N entries (default 10K), evict oldest by `computed_at`. Alternatively, use an LRU.

### 4.3 The Resonance Graph (Materialized View)

Resonance R(H1, H2) is too expensive to compute on every query (it requires reading both attractor fields). Precompute the top-K resonance partners per holon.

```sql
CREATE TABLE resonance_graph (
    holon_id TEXT NOT NULL,
    partner_id TEXT NOT NULL,
    resonance_score REAL NOT NULL,    -- [0, 1]
    complementarity REAL NOT NULL,    -- factor 1
    gamma_compat REAL NOT NULL,       -- factor 2
    great_way_intersect REAL NOT NULL,-- factor 3
    computed_at TEXT NOT NULL,
    PRIMARY KEY (holon_id, partner_id),
    FOREIGN KEY (holon_id) REFERENCES nodes(id),
    FOREIGN KEY (partner_id) REFERENCES nodes(id)
);
CREATE INDEX idx_resonance_holon_score ON resonance_graph(holon_id, resonance_score DESC);
```

**Population:**
- **Incremental (Tier 2):** When a holon's attractor field changes, recompute its resonance against candidate partners. Candidates = holons with complementary type_class (donor↔acceptor, sharer↔sharer) within the same scale_code or adjacent scales. Limit to top 50 candidates.
- **Full rebuild (Tier 3, hourly):** Recompute the entire resonance_graph table to correct incremental drift. This is O(N · 50) — at 100K nodes, 5M operations, ~30 seconds.

**Query:** `SELECT * FROM resonance_graph WHERE holon_id = ? ORDER BY resonance_score DESC LIMIT 5` — sub-millisecond.

### 4.4 The HolonSummary View (for Audit/Stats)

Many MCP tools (`tdg_audit`, `tdg_graph_stats`, `tdg_mind_state`) need aggregated views of the graph. Pre-materialize a summary:

```sql
CREATE TABLE holon_summary (
    holon_id TEXT PRIMARY KEY,
    node_type TEXT,
    scale_code TEXT,
    type_class TEXT,
    synthesis_status TEXT,
    developmental_stage INTEGER,
    g_z REAL,
    p_z REAL,
    health_state TEXT,
    lesser_phase TEXT,
    greater_phase TEXT,
    edge_count INTEGER,
    resonance_partner_count INTEGER,
    updated_at TEXT
);
CREATE INDEX idx_summary_type_class ON holon_summary(type_class);
CREATE INDEX idx_summary_health_state ON holon_summary(health_state);
CREATE INDEX idx_summary_stage ON holon_summary(developmental_stage);
```

**Population:** Updated by Tier 2 workers whenever a holon's attractor field or health is recomputed. One row per holon.

**Query examples:**
- `SELECT COUNT(*) FROM holon_summary WHERE health_state = 'sinkhole'` — instant
- `SELECT * FROM holon_summary WHERE type_class LIKE 'strong-donor%' ORDER BY g_z DESC LIMIT 10` — instant
- `SELECT AVG(g_z), AVG(p_z) FROM holon_summary` — instant

---

## 5. Efficiency Analysis — Concrete Numbers

Assumptions: 100K active nodes, 500K edges, 8-core CPU, NVMe SSD, 16GB RAM. SQLite WAL mode, 4 metabolism workers.

### 5.1 Tier 1 (Synchronous Write) Latency

| Operation | Latency | Breakdown |
|---|---|---|
| `tdg_observe` (simple) | 3–5ms | 1ms node insert + 1ms entity upsert/edge + 1ms provenance + 0.5ms FTS trigger + 0.5ms job enqueue |
| `tdg_observe` (with 5 entities) | 5–8ms | 1ms node + 3ms entity/edge × 5 + 1ms provenance + 1ms FTS + 1ms enqueue |
| `tdg_create` (with auto-wire) | 4–7ms | 1ms node + 2ms auto-wire edges + 1ms provenance + 1ms FTS + 1ms enqueue |
| `tdg_connect` | 2–4ms | 1ms edge insert + 1ms validation + 0.5ms provenance + 0.5ms enqueue + 1ms flow propagation (existing) |
| `tdg_submit_synthesis` | 8–12ms | 1ms write draft + 5ms 5-gate validation + 1ms provenance + 1ms enqueue |

**Within budget (< 10ms).** The existing `tdg_observe` is already ~5ms; adding job enqueue is < 1ms overhead.

### 5.2 Tier 2 (Async Metabolism) Throughput

| Job type | Latency/job | Jobs/sec (4 workers) |
|---|---|---|
| `lesser_tick` (Dormant → Ingesting) | 0.5ms | 8,000 |
| `lesser_tick` (full cycle) | 2ms | 2,000 |
| `recompute_attractor` | 5ms | 800 |
| `recompute_health` | 1ms | 4,000 |
| `resonance_update` (incremental, 50 candidates) | 10ms | 400 |
| `digestion_cascade` | 20ms | 200 |
| `reflect` (full run) | 500ms | 8 |

**Sustained throughput:** ~2,000 lesser_ticks/sec, ~800 attractor recomputes/sec. At 100K nodes, if every node ticks once per minute on average, that's 1,667 ticks/sec — within budget.

**Burst handling:** If 1,000 observations arrive in a burst (e.g. agent catching up after downtime), the queue fills with 1,000 × 5 = 5,000 jobs (observe + entity ticks + attractor + health + resonance). At 2,000 jobs/sec throughput, the backlog clears in 2.5 seconds. The agent sees fast Tier 1 acknowledgements throughout; the metabolism catches up within seconds.

### 5.3 Tier 3 (Scheduled) Cost

| Schedule | Frequency | Cost per run | Notes |
|---|---|---|---|
| Greater-cycle sweep | every 5 min | ~500ms | Scans holon_summary for pressure > threshold; typically < 1% of holons qualify |
| Mind pipeline | every 15 min | ~2s | Reads holon_summary aggregates, diagnoses, enqueues catalyst |
| Resonance full rebuild | every 1 hour | ~30s | O(N · 50) at 100K nodes |
| Stage advancement sweep | every 1 hour | ~1s | Scans holon_summary for evidence/age eligibility |
| SelfManager | every 6 hours | ~10s | Existing: HealthMonitor → Janitor → Enricher → Archiver |

**Total Tier 3 CPU:** < 5% of one core on average. Does not interfere with Tier 1/2.

### 5.4 Tier 1 Read Latency

| Operation | Cold | Cached |
|---|---|---|
| `tdg_fetch_context` (depth 2) | 50–100ms | 2–5ms |
| `tdg_attractor` | 5–10ms | 1ms (JSON column read) |
| `tdg_health` | 2–5ms | 1ms |
| `tdg_resonance` (H1, H2) | 0.5ms (indexed) | 0.5ms |
| `tdg_get_node` | 1–2ms | 1ms |
| `tdg_search` (FTS5) | 5–15ms | 1ms (query cache) |
| `tdg_audit` (full bundle) | 200–500ms | 50ms (uses holon_summary) |
| `tdg_graph_stats` | 1–5ms (uses holon_summary) | < 1ms (60s TTL cache, existing) |

**Within budget.** Agent-facing reads are sub-100ms cold, sub-5ms cached.

### 5.5 Memory Footprint

| Component | Size at 100K nodes |
|---|---|
| SQLite database (nodes + edges + events + mutation_log) | ~500 MB |
| attractor_field_json, health_json, lesser/greater_cycle_json columns | ~200 MB (avg 2KB/holon) |
| resonance_graph table | ~50 MB (top-50 partners × 100K holons × 50 bytes) |
| context_cache table | ~100 MB (10K cached ContextPacks × 10KB avg) |
| holon_summary table | ~10 MB (100K × 100 bytes) |
| pending_metabolism queue | ~1 MB (typical < 1K jobs) |
| Connection pool (8 connections) | ~50 MB |
| **Total RSS** | **~1 GB** |

Within the existing lean-mode target (< 50MB RSS is unrealistic with attractor fields; realistic target is < 2GB for full operation, < 500MB for lean mode with caching disabled).

---

## 6. Effectiveness Analysis — Does It Actually Serve Agents?

Efficiency is necessary but not sufficient. The system must be *effective* — it must actually make the agent smarter, not just faster. The test: does the agent's behavior change because of what TDG computes?

### 6.1 The Agent Interaction Contract

The agent interacts with TDG through exactly four operations:

| Operation | Purpose | Frequency |
|---|---|---|
| `tdg_fetch_context` | Get context for current task | Every turn (1–5/sec) |
| `tdg_observe` | Record what happened | End of turn (1/sec) |
| `tdg_submit_synthesis` | Submit a hypothesis/synthesis | Rare (1–10/session) |
| `tdg_search` | Find specific memories | Occasional (1–5/sec) |

Everything else is internal. The agent never calls `tdg_attractor` or `tdg_health` directly — those feed into `tdg_fetch_context`. The agent never triggers lesser-cycle ticks directly — those happen automatically.

**This is the API simplicity invariant:** the agent sees 4 tools, not 36. The complexity is internal; the interface is minimal.

### 6.2 What the Agent Gets Back

When the agent calls `tdg_fetch_context(holon_id, scope="intra+inter+extra", depth=2)`, it gets a ContextPack containing:

1. **Identity** — what is this holon? (type, scale, type_class, synthesis_status)
2. **Intra** — what is its interior state? (attractor field, health, lesser/greater cycle phase, drives, shadows)
3. **Inter** — what is it connected to? (bonds, bridges, top-5 resonance partners)
4. **Extra** — what is its context? (parent chain, sub-holons, great-way trajectory)
5. **Grounding** — what is the epistemic status? (anchor docs, hypothesis docs, status)

Every claim carries a `[status: {synthesis_status}]` tag. The agent knows whether it's reading canonical truth, a canonical-hypothesis, or an ai-draft. This is the epistemic spine — it prevents the agent from treating its own prior outputs as ground truth.

### 6.3 How This Changes Agent Behavior

Compare two agents — one with the current tdg-rust, one with the refactored version — facing the same task:

**Current agent (dashboard memory):**
- Calls `tdg_context` → gets a terrain-first prompt with drive averages, node counts, structural gaps
- The prompt is descriptive ("you have 50 observations, 3 skills, drive polarity is integrated")
- The agent decides what to do based on its own reasoning over this description
- The memory doesn't *push back* — it just reports

**Refactored agent (metabolic memory):**
- Calls `tdg_fetch_context` → gets a ContextPack with attractor field, health, resonance partners, synthesis_status tags
- The ContextPack includes graph-level mind diagnoses: "the graph is in dark-addiction (observation_count / integration_count = 15); you are creating observations faster than you're integrating them"
- The ContextPack includes resonance-based suggestions: "this observation resonates strongly (R=0.82) with holon X; consider connecting them"
- The ContextPack includes phase-transition warnings: "holon Y is at bifurcation (readiness=0.87); your next action may trigger a Transformation"
- The agent's behavior changes because the memory *informs* — it doesn't just describe, it diagnoses and recommends

**The key difference:** the refactored memory is *directive*. It tells the agent not just what state it's in, but what state the *graph* is in, and what the agent can do about it. This is what "a mind that actually works" means — the memory participates in the agent's cognition, not just stores its outputs.

### 6.4 The Closed Loop in Practice

A concrete scenario:

1. **Agent is working on a coding task.** It calls `tdg_fetch_context(project_id)` at the start of each turn.
2. **Turn 1:** Agent observes a bug, writes observation via `tdg_observe`. Tier 1 enqueues metabolism.
3. **Turn 2 (30 seconds later):** Agent calls `tdg_fetch_context(project_id)` again. By now, Tier 2 has metabolised the observation:
   - Lesser cycle ran: observation is in Integrating phase
   - Attractor field computed: type_class = "weak-donor-sts" (the observation has unmet integration demands)
   - Health computed: G_z = 45 (sub-optimal), P_z = 60 (building — good tension)
   - Resonance computed: R(observation, existing_skill_X) = 0.78
   - ContextPack includes: "this observation resonates with skill X; consider whether skill X applies"
4. **Agent connects the observation to skill X** via `tdg_connect`. This injects catalyst into both holons.
5. **Turn 3:** Tier 2 has metabolised the connection:
   - Observation's lesser cycle: catalyst processed, Experience accumulated, shadow diagnosed (no shadow — healthy integration)
   - Skill X's lesser cycle: catalyst processed, Experience accumulated
   - Both attractor fields updated; both healths improved (G_z rose due to increased bondability)
   - If Experience crossed threshold: greater-cycle pressure accumulated on the project holon
6. **Turn 10 (after several integrations):** The project holon's greater-cycle pressure crosses threshold. Tier 3 greater-cycle sweep fires:
   - Transformation event: project's Significator restructures
   - Stage may advance (e.g. from Rational to Pluralistic — the project has integrated enough diverse perspectives)
   - ContextPack for project_id now shows: greater_phase = ChoicePolarizing, stage = Pluralistic, health_state = Optimal
   - Agent sees: "your project has reached a new developmental stage; your integration work is paying off"
7. **Periodically (every 15 min):** Tier 3 mind pipeline runs:
   - Reads graph state: 80 observations, 5 skills, 15 connections, mean G_z = 55, mean P_z = 45
   - Diagnoses: GoldenAllergy (hypothesis_count = 0, observation_count = 80 — no emergence)
   - Injects catalyst into 3 observation clusters (forces digestion cascade to create hypotheses)
   - Next `tdg_fetch_context` reflects the new hypotheses

**This is a mind at work.** The agent isn't just storing memories — it's participating in a metabolic system that integrates, diagnoses, and evolves. The agent's cognition is *extended* by the memory's metabolism.

---

## 7. Scheduling and Concurrency

### 7.1 Write Concurrency

**Existing:** WriteGuard (file lock per DB) serializes all writes. This is correct but coarse-grained.

**Refactored:** Keep WriteGuard for Tier 1. Tier 2 workers each hold their own connection (from the pool) and use `BEGIN IMMEDIATE` transactions for their specific writes. Conflicts are rare because:
- Each Tier 2 job touches one holon (locality)
- Attractor field / health writes are to JSON columns on the holon's own row (no cross-holon contention)
- Resonance graph writes are to a separate table (no contention with holon writes)

**Conflict resolution:** SQLite's busy_timeout (default 5s) handles contention. If a Tier 2 job times out, it's re-enqueued with attempts++.

### 7.2 Read Concurrency

**Existing:** Connection pool (8 connections), WAL mode allows concurrent readers.

**Refactored:** Same. The context_cache table reduces read load further — most reads hit the cache, not the base tables.

### 7.3 Tier 2 Worker Count

Default 4 workers. Tunable via `TDG_METABOLISM_WORKERS`. Rule of thumb: 1 worker per 25K nodes. At 100K nodes, 4 workers. At 1M nodes, 40 workers (but at that scale, you've outgrown SQLite — migrate to PostgreSQL).

### 7.4 Job Priority

Three priority levels:
- **High (2):** Agent-requested (e.g. `tdg_tick <id>` explicit call, or `tdg_fetch_context` cold path that triggers recompute). Processed first.
- **Normal (1):** Standard metabolism (lesser_tick from catalyst injection, recompute_attractor from dirty flag).
- **Low (0):** Background maintenance (digestion_cascade, reflect, enrich, resonance_update).

Workers pick highest-priority first, FIFO within priority.

### 7.5 Backpressure

If `pending_metabolism` exceeds 10K jobs:
- Tier 1 starts rejecting `tdg_observe` with a "metabolism saturated" error (agents should retry with exponential backoff)
- Tier 1 still accepts `tdg_create`, `tdg_connect`, `tdg_fetch_context` (essential operations)
- Tier 3 schedules skip their runs (except SelfManager, which always runs)
- An alert fires (logged, surfaced in `tdg_system_health`)

If `pending_metabolism` exceeds 100K jobs:
- Tier 1 rejects all writes except `tdg_fetch_context` and `tdg_search`
- The system is in distress; manual intervention needed (run `tdg_self_manage --aggressive`, or scale out)

---

## 8. Failure Modes and Recovery

### 8.1 Tier 2 Job Failure

If a job fails (panic, OOM, SQLite error), it's re-enqueued with attempts++. After 3 attempts, it's moved to `failed_metabolism` table for inspection. The holon's dirty flag remains set; the next read of that holon will attempt on-demand recompute.

```sql
CREATE TABLE failed_metabolism (
    id INTEGER PRIMARY KEY,
    holon_id TEXT,
    job_type TEXT,
    payload TEXT,
    error TEXT,
    failed_at TEXT,
    attempts INTEGER
);
```

**Recovery:** `tdg_retry_failed` MCP tool re-enqueues all failed jobs. Or manual: inspect, fix, re-enqueue.

### 8.2 Crash Recovery

On startup:
1. Scan `pending_metabolism` for jobs claimed but not completed (stale `claimed_at`). Reset them to unclaimed.
2. Scan `nodes` for dirty flags set but no pending recompute job. Enqueue recompute jobs.
3. Rebuild `context_cache` is NOT needed (it's a cache; cold reads will repopulate).
4. Rebuild `resonance_graph` is NOT needed (incremental updates will correct it; full rebuild runs hourly anyway).

**Recovery time:** < 5 seconds at 100K nodes. The system is operational immediately; full metabolism convergence happens within 1 minute.

### 8.3 Consistency Repair

If the database is restored from backup (or replicated), some holons may have inconsistent state (e.g. attractor_field_json doesn't match drives_json). The `tdg_renormalize` MCP tool (existing) handles this:

```
tdg_renormalize:
  1. For each holon with attractor_field_dirty = 1: enqueue recompute_attractor
  2. For each holon with health_dirty = 1: enqueue recompute_health
  3. Rebuild resonance_graph (full)
  4. Rebuild holon_summary (full)
  5. Clear context_cache (force cold reads)
```

This is the "reset to consistent" button. Run it after crashes, restores, or version upgrades.

---

## 9. The Memory vs Mind Distinction

This is the conceptual key that makes the design coherent.

### Memory (Passive Storage)

The graph itself — nodes, edges, attractor fields, health, resonance. This is the *substrate*. It's passive: it stores state, it doesn't act. Memory operations are CRUD. The memory is what persists across agent sessions.

**Implementation:** The SQLite database + cached JSON columns + materialized views. Tier 1 writes to memory; Tier 2 updates memory's computed fields; Tier 3 maintains memory's aggregates.

### Mind (Active Metabolism)

The Tier 3 integration pass that reads memory, diagnoses, and feeds back. This is the *process*. It's active: it acts on memory, it doesn't just store. Mind operations are diagnosis + catalyst injection. The mind is what makes the system alive between agent sessions.

**Implementation:** The Tier 3 scheduler running the mind pipeline every 15 minutes. It's a process, not a data structure. It has no persistent state of its own — it reads memory, acts, and the results are stored back in memory.

### The Relationship

- **Memory without mind** = a database. Stores everything, integrates nothing. The current tdg-rust.
- **Mind without memory** = a stateless reasoner. Diagnoses nothing because it has no history. Useless.
- **Memory + mind** = a metabolism. Stores, integrates, feeds back, evolves. The target.

The agent interacts with both, but only through the ContextPack. The ContextPack is the *projection* of memory + mind into a single agent-readable object. The agent doesn't know (or need to know) which parts are stored vs computed vs diagnosed — it just reads the projection and acts.

### Why This Matters for "The Mind That Actually Works"

A mind that actually works must do three things:
1. **Remember** (memory) — persist state across time
2. **Integrate** (mind) — combine disparate memories into coherent understanding
3. **Inform** (the ContextPack projection) — make the integration available to the agent

The current tdg-rust does (1) well. It doesn't do (2) at all (the "mind pipeline" is descriptive, not integrative). It does (3) poorly (the `tdg_context` output is unstructured terrain, not a diagnosed projection).

The refactored tdg-rust does all three:
1. Memory = the graph + attractor fields + health + resonance
2. Mind = the Tier 3 integration pass (diagnose graph-level shadows, inject catalyst)
3. Inform = the ContextPack with status tags, resonance suggestions, phase-transition warnings

This is what makes it "the mind that actually works."

---

## 10. Implementation Sequence

This document specifies the computational model. The implementation sequence (which builds on the audit's 6-phase refactor plan):

| Phase | Computational additions |
|---|---|
| **Phase 0 (Hygiene)** | Split tools.rs; fix dead diagnostic histories. No new computation. |
| **Phase 1 (Holon + Status + Scale)** | Add columns; add `Holon` newtype. No new computation yet — just scaffolding. |
| **Phase 2 (Lesser Cycle)** | **Add Tier 2 worker pool.** Add `pending_metabolism` table. Implement `lesser_tick` job. Event-driven: enqueue on `tdg_connect` / `tdg_observe`. This is where the metabolism begins. |
| **Phase 3 (Attractor + Health + Resonance)** | **Add dirty-flag pattern.** Implement `recompute_attractor`, `recompute_health`, `resonance_update` jobs. Add `resonance_graph` materialized view. ContextCache table. |
| **Phase 4 (Greater Cycle + Phase Transitions)** | **Add Tier 3 scheduler.** Implement greater-cycle sweep (every 5 min). Implement phase-transition detector. Greater-cycle ticks enqueue downward-pressure events (Tier 2). |
| **Phase 5 (ContextPack + 5-Gate)** | **Add `tdg_fetch_context` tool.** Implement ContextPack struct, rendering, truncation, caching. Implement 5-Gate Validation (Tier 1, synchronous). Implement `tdg_submit_synthesis` and `tdg_elevate`. |
| **Phase 6 (Type System + Archetypes)** | Implement `classify_type` (called during `recompute_attractor`). Implement T1/T2/T3 validation (Tier 3 schedule). Add 22 archetypes as a library. |
| **Phase 7 (Mind Pipeline Redesign)** | **Rewrite `src/mind/` as the Tier 3 integration pass.** The mind pipeline runs every 15 min: reads `holon_summary`, diagnoses graph-level shadows, enqueues catalyst injections. This is where the system becomes a mind. |

**Phases 2 and 7 are the keystones.** Phase 2 starts the metabolism. Phase 7 closes the loop. Without Phase 7, the system metabolises but doesn't integrate — it's a body without a mind. Without Phase 2, the system integrates nothing — it's a mind without a body.

---

## 11. Open Questions

1. **SQLite at scale.** At what point does SQLite become the bottleneck? 1M nodes? 10M? The design assumes 100K. If the system needs to scale beyond ~500K nodes, the migration path is: keep the same computational model, swap SQLite for PostgreSQL (or DuckDB for analytical loads). The Tier 1/2/3 separation makes this a storage-layer swap, not an architecture change.

2. **Multi-agent contention.** If multiple agents write to the same graph simultaneously, the metabolism queue could become a bottleneck. Mitigation: per-agent write rate limiting (e.g. max 10 writes/sec per agent_id). The queue depth alerting catches sustained overload.

3. **Catalyst quantification.** The theory says catalyst is "boundary-crossing pressure" but doesn't give a formula. The design uses `catalyst_amount = edge_weight × drive_complementarity × resonance` (Phase 2, §2.4 of the audit). This is a heuristic; it needs empirical tuning. The `config/catalyst_weights.yaml` file should be created and tuned based on observed graph health.

4. **Mind pipeline diagnosis accuracy.** The graph-level shadow diagnoses (DarkAddiction = observation_count / integration_count > 10, etc.) are heuristics. They need validation against real agent usage. The thresholds should be configurable and tuned over time.

5. **ContextPack token-budget truncation.** The "drop cheapest-to-lose first" order is specified, but the actual token counting (especially for markdown with status tags) needs a real tokenizer. Use the existing `tokenizers` crate (already a dependency via ONNX feature).

6. **Resonance candidate selection.** The "top-50 candidates by type_class complementarity within same/adjacent scale" is a heuristic. At 100K nodes, even scanning for candidates is expensive if not indexed. Mitigation: `holon_summary` table has an index on `type_class` — candidate selection is `SELECT holon_id FROM holon_summary WHERE type_class IN (complementary_classes) AND scale_code IN (target_scales) LIMIT 50`. Sub-millisecond.

7. **Greater-cycle sweep vs event-driven.** The design says greater-cycle is event-driven (pressure crossing threshold) but also has a Tier 3 sweep every 5 min. The sweep is a safety net — it catches holons whose pressure crossed threshold but didn't enqueue a tick (e.g. due to a crash). In steady state, the sweep is a no-op. This is fine; the redundancy is intentional.

8. **ContextCache invalidation radius.** When holon H is written, we invalidate H's cache and the cache of holons within 1 hop. But what about holons within 2 hops (whose `extra.parent_chain` might include H)? At depth 3+, the cache could be stale. Mitigation: short TTL (5 min) bounds staleness. For depth-3 reads, accept eventual consistency. If an agent needs strong freshness, it can pass `fresh=true` to bypass the cache.

---

## 12. Summary — The Computational Contract

| Property | Guarantee |
|---|---|
| **Agent write latency** | < 10ms (Tier 1, synchronous) |
| **Agent read latency (cached)** | < 5ms (Tier 1, cache hit) |
| **Agent read latency (cold)** | < 100ms (Tier 1, cache miss) |
| **Metabolism convergence** | < 1 second (Tier 2, per-holon) |
| **Graph-level integration** | < 15 minutes (Tier 3, mind pipeline) |
| **Consistency** | Strong for Tier 1 writes; eventual for computed fields; weak for graph-level aggregates |
| **Failure recovery** | < 5 seconds to operational; < 1 minute to metabolism convergence |
| **Scale ceiling** | ~500K nodes on single SQLite; horizontal scaling requires storage migration |
| **Agent API surface** | 4 tools (`fetch_context`, `observe`, `submit_synthesis`, `search`) — complexity is internal |
| **Memory vs Mind** | Memory = the graph (passive); Mind = the Tier 3 integration pass (active); both projected through ContextPack |

The system is efficient because it computes only what changed, when it changed, at the right tier. The system is effective because the mind pipeline closes the loop — it reads memory, diagnoses, and feeds back as catalyst, making the system a metabolism rather than a database. The agent sees a simple interface (ContextPack) backed by a complex but well-organised computational machine.

This is how holonic operations run computationally. This is how TDG becomes the mind that actually works.

---

*Design doc completed 2026-07-03. Companion to `HOLONIC-SCIENCE-AUDIT-AND-REFACTOR-PLAN.md`. Read together: the audit specifies what to build; this document specifies how it runs.*
