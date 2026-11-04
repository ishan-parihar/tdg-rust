# Why every memory carries a purpose

**What this document is.** The shape of the whole system on one page, written
for someone who will not read the code. If you want the full API surface,
`docs/` holds it. If you want to know what this thing *is*, this is the page.

---

## The one sentence

TDG-Rust is a memory for AI agents where every remembered thing carries what it
meant and why it matters, and the connections between memories rearrange
themselves as the agent works — so recall explains itself instead of returning
whatever happens to be nearby in vector space.

---

## The idea a vector database cannot express

A vector database stores what you saw: text, an embedding, a distance. TDG
stores three things per node, and the third is the one that matters.

| Part of a node | What it carries |
|---|---|
| **Content** | The text or payload, and its embedding. |
| **Embedding** | The vector, so similarity search still works. |
| **Telos** | The purpose — a drive, a goal, or a function. |

The telos is why the graph can answer "why did you keep this?" and not just
"what looks like this?". Structure then develops through interaction: nodes form
hierarchies, sequences and clusters, and the graph reorganises rather than being
migrated by hand.

---

## The parts

One Rust binary, with five parts underneath a single service. The executed
gate is 626 tests across every test binary — 449 in the library, the rest in the
MCP end-to-end, plugin, integration and smaller binaries.

- **Store** (SQLite, via `rusqlite`) — the durable base and the source of truth.
  Both the symbolic graph and the vectors live in the same database.
- **Index** (SQLite FTS5) — full-text search over node content, ranked.
- **Similarity** (cosine, in Rust) — the vector arm is computed in-process rather
  than served by a separate approximate-nearest-neighbour index.
- **Graph** (`petgraph`) — the symbolic structure: links, hierarchies, clusters,
  developmental stages.
- **Evolution** (scheduler) — what reorganises the graph over time.

Above them sits the MCP surface an agent actually talks to: `remember`,
`recall`, `link`, `evolve`, `inspect`. Those five verbs are the whole interface,
and everything the system does is reachable through them.

> **Correction, 2026-11-03.** The first version of this document named a Sled
> store and an HNSW index. Neither exists here: the repository declares
> `rusqlite` and `petgraph`, and a grep of `src/` finds no mention of either
> `sled` or `hnsw`. Retrieval is FTS5 ranking plus a cosine pass computed in
> Rust. The document was written from the README's diagram, which is looser
> than the dependency list, and the diagram should be read as an overview
> rather than as the implementation.

---

## How a memory is kept

```
 an agent calls tdg.remember with some content
        |
 digestion: the content is processed and embedded
        |
 a node is written carrying content, embedding, and a telos
        |
 store persists it; index and graph pick it up
```

---

## How recall explains itself

```
 an agent calls tdg.recall with a query
        |
 FTS5 ranks candidates by the query's terms
        |
 a cosine pass re-ranks them by embedding distance
        |
 the graph contributes what those nodes are connected to
        |
 the answer carries its telos and its links, not just its text
```

Falling back to similarity alone is the failure mode this design exists to
avoid, which is why the graph is consulted every time and not as an enrichment
step.

---

## The boundaries

- **Neural and symbolic are one graph, not two systems joined later.** An
  embedding without a symbol is opaque; a symbol without an embedding cannot be
  searched. One store holds both.
- **Schema evolves; data does not migrate.** Developmental structure is expressed
  by the graph itself, so adding a kind of relationship is not a migration.
- **A single binary, self-hosted, MIT.** No service to rent and no network call
  required to remember something.
- **The agent surface is the only surface.** The CLI and the HTTP layer reach
  the same verbs the MCP server exposes; there is no privileged path.

---

## What this respects

Comparison with vector databases, mem0/Zep and Letta is recorded honestly in the
README table, including the places the comparison is close. `docs/` holds the
page index; `eval/` and the test suite are what a claim here is checked against.
