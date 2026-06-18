# Crate Research: Implementing Audit Gaps

**Date**: 2026-06-18
**Reference**: PYTHON_VS_RUST_AUDIT.md

---

## Summary

| Gap Area | Crate Needed | Action |
|----------|-------------|--------|
| HRR Fix (P0) | `rustfft` | Replace element-wise ops with FFT circular convolution |
| Write Guard (P2) | `fs4` | Add exclusive file locking for inter-process safety |
| Dream Engine (P2) | `linfa` ecosystem | Add clustering for observation pattern extraction |
| Async Writer (P1) | tokio (already have) | Use mpsc + CancellationToken pattern |
| SQLite Triggers (P2) | rusqlite (already have) | Use `execute_batch` for CREATE TRIGGER |

---

## 1. HRR Fix — `rustfft`

```toml
rustfft = "6.1"
```

Key: `bind(a, b) = IFFT(FFT(a) * FFT(b))`, `unbind(c, r) = IFFT(FFT(c) * conj(FFT(r)))`

---

## 2. Write Guard — `fs4`

```toml
fs4 = "1"
```

Key: `try_lock()` with timeout, auto-release on Drop, separate lock file

---

## 3. Dream Engine — `linfa`

```toml
linfa = "0.7"
linfa-clustering = "0.7"
linfa-preprocessing = "0.7"
```

Key: TF-IDF vectorization + DBSCAN/K-Means clustering

---

## 4. Async Writer — tokio (no new crates)

Use `mpsc::channel(256)` + `CancellationToken` + `Drop` for auto-drain

---

## 5. SQLite Triggers — rusqlite (no new crates)

Use `execute_batch` for `CREATE TRIGGER` statements

---

## Total New Crates: 5

- `rustfft` — HRR correctness
- `fs4` — Write guard safety
- `linfa` — Clustering
- `linfa-clustering` — Clustering algorithms
- `linfa-preprocessing` — TF-IDF vectorization

**Estimated integration effort: 16-20 hours**
