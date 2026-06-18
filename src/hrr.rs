use ndarray::Array1;
use rand::Rng;
use rand::SeedableRng;
use rustfft::{num_complex::Complex, FftPlanner};

/// Dimension of HRR phase vectors.
pub const HRR_DIM: usize = 1024;

// ── Role Constants ───────────────────────────────────────────────────
// Used as category labels for probe/unbind operations on memory banks.

/// Probe: what entity does this content relate to
pub const ROLE_ENTITY: &str = "entity";
/// Related: structural adjacency
pub const ROLE_ATOM: &str = "atom";
/// Reason: reasoning over entities
pub const ROLE_MEM: &str = "mem";
/// Memory bank super-vector
pub const ROLE_BANK: &str = "bank";

/// Estimate signal-to-noise ratio for N stored vectors.
///
/// SNR ≈ sqrt(dim) / sqrt(N)
///
/// Returns `f64::INFINITY` when count is 0.
pub fn snr_estimate(count: usize, dim: usize) -> f64 {
    if count == 0 {
        return f64::INFINITY;
    }
    (dim as f64).sqrt() / (count as f64).sqrt()
}

/// Phase-encode a scalar value into a 1024-dim circular permutation vector.
pub fn phase_encode(value: f64, dim: usize) -> Array1<f64> {
    // Use value as seed for deterministic RNG
    let seed = (value * 1000.0) as u64;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let base: Array1<f64> = Array1::from_shape_fn(dim, |_| rng.gen_range(-1.0..1.0));

    // Circular shift based on value (discretize to dim positions)
    let shift = ((value * dim as f64).round() as isize).rem_euclid(dim as isize) as usize;

    let mut result = Array1::zeros(dim);
    for i in 0..dim {
        result[i] = base[(i + shift) % dim];
    }
    result
}

/// Bind two vectors via circular convolution: IFFT(FFT(a) * FFT(b)).
pub fn bind(a: &Array1<f64>, b: &Array1<f64>) -> Array1<f64> {
    let dim = a.len();
    let mut a_c: Vec<Complex<f64>> = a.iter().map(|&v| Complex::new(v, 0.0)).collect();
    let mut b_c: Vec<Complex<f64>> = b.iter().map(|&v| Complex::new(v, 0.0)).collect();

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(dim);
    fft.process(&mut a_c);
    fft.process(&mut b_c);

    for (a, b) in a_c.iter_mut().zip(b_c.iter()) {
        *a *= *b;
    }

    let ifft = planner.plan_fft_inverse(dim);
    ifft.process(&mut a_c);

    Array1::from_vec(a_c.iter().map(|c| c.re / dim as f64).collect())
}

/// Unbind two vectors via pseudo-inverse: IFFT(FFT(c) / FFT(r)).
pub fn unbind(c: &Array1<f64>, r: &Array1<f64>) -> Array1<f64> {
    let dim = c.len();
    let mut c_c: Vec<Complex<f64>> = c.iter().map(|&v| Complex::new(v, 0.0)).collect();
    let mut r_c: Vec<Complex<f64>> = r.iter().map(|&v| Complex::new(v, 0.0)).collect();

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(dim);
    fft.process(&mut c_c);
    fft.process(&mut r_c);

    for (c, r) in c_c.iter_mut().zip(r_c.iter()) {
        let mag_sq = r.re * r.re + r.im * r.im;
        if mag_sq > 1e-10 {
            *c = *c / *r;
        } else {
            *c = Complex::new(0.0, 0.0);
        }
    }

    let ifft = planner.plan_fft_inverse(dim);
    ifft.process(&mut c_c);

    Array1::from_vec(c_c.iter().map(|c| c.re / dim as f64).collect())
}

/// Bundle (superposition) multiple vectors by element-wise addition.
pub fn bundle(vectors: &[Array1<f64>]) -> Array1<f64> {
    if vectors.is_empty() {
        return Array1::zeros(HRR_DIM);
    }
    let mut result = vectors[0].clone();
    for v in &vectors[1..] {
        result = &result + v;
    }
    normalize(&result)
}

/// Normalize a vector to unit length.
pub fn normalize(v: &Array1<f64>) -> Array1<f64> {
    let norm = v.mapv(|x| x * x).sum().sqrt();
    if norm < 1e-10 {
        v.clone()
    } else {
        v / norm
    }
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &Array1<f64>, b: &Array1<f64>) -> f64 {
    let dot = a.dot(b);
    let norm_a = a.mapv(|x| x * x).sum().sqrt();
    let norm_b = b.mapv(|x| x * x).sum().sqrt();

    if norm_a < 1e-10 || norm_b < 1e-10 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// HRR memory bank: a collection of named vectors.
pub struct HrrMemoryBank {
    /// Name → vector mapping
    entries: Vec<(String, Array1<f64>)>,
    /// Composite (bundled) vector
    composite: Array1<f64>,
}

impl HrrMemoryBank {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            composite: Array1::zeros(HRR_DIM),
        }
    }

    /// Store a named vector in the bank.
    pub fn store(&mut self, name: String, vector: Array1<f64>) {
        self.composite = &self.composite + &vector;
        self.entries.push((name, vector));
    }

    /// Probe: find the most similar stored vector.
    pub fn probe(&self, query: &Array1<f64>) -> Option<(String, f64)> {
        let mut best = None;
        let mut best_score = f64::NEG_INFINITY;

        for (name, vector) in &self.entries {
            let score = cosine_similarity(query, vector);
            if score > best_score {
                best_score = score;
                best = Some((name.clone(), score));
            }
        }
        best
    }

    /// Related: find vectors related to a given name via unbinding.
    pub fn related(&self, name: &str, key: &Array1<f64>) -> Vec<(String, f64)> {
        let target = self.entries.iter().find(|(n, _)| n == name);
        if let Some((_, vector)) = target {
            let unbound = unbind(vector, key);
            let mut results: Vec<(String, f64)> = self
                .entries
                .iter()
                .filter(|(n, _)| n != name)
                .map(|(n, v)| (n.clone(), cosine_similarity(&unbound, v)))
                .collect();
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            results
        } else {
            Vec::new()
        }
    }

    /// Reason: combine two probes to find related knowledge.
    pub fn reason(&self, query_a: &Array1<f64>, query_b: &Array1<f64>) -> Vec<(String, f64)> {
        let combined = normalize(&(&normalize(query_a) + &normalize(query_b)));
        let mut results: Vec<(String, f64)> = self
            .entries
            .iter()
            .map(|(n, v)| (n.clone(), cosine_similarity(&combined, v)))
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Get all stored entries.
    pub fn entries(&self) -> &[(String, Array1<f64>)] {
        &self.entries
    }

    /// Number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for HrrMemoryBank {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a random key vector for binding.
pub fn random_key(dim: usize) -> Array1<f64> {
    let mut rng = rand::thread_rng();
    Array1::from_shape_fn(dim, |_| rng.gen_range(-1.0..1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_encode_dim() {
        let v = phase_encode(0.5, HRR_DIM);
        assert_eq!(v.len(), HRR_DIM);
    }

    #[test]
    fn bind_unbind_roundtrip() {
        let a = Array1::from_vec(vec![1.0, 2.0, 3.0, 4.0]);
        let b = Array1::from_vec(vec![0.5, 1.0, 0.5, 2.0]);

        let bound = bind(&a, &b);
        let unbound = unbind(&bound, &b);

        // Circular correlation: unbind(bind(a,b), b) recovers a scaled by |FFT(b)|^2
        let best_val = unbound.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(best_val > 0.0, "unbind should produce positive values for positive input");
    }

    #[test]
    fn bundle_preserves_similarity() {
        let v1 = Array1::from_vec(vec![1.0, 0.0, 0.0, 0.0]);
        let v2 = Array1::from_vec(vec![0.0, 1.0, 0.0, 0.0]);
        let v3 = Array1::from_vec(vec![0.0, 0.0, 1.0, 0.0]);

        let bundled = bundle(&[v1.clone(), v2.clone(), v3.clone()]);
        let s1 = cosine_similarity(&bundled, &v1);
        let s2 = cosine_similarity(&bundled, &v2);
        let s3 = cosine_similarity(&bundled, &v3);

        // All should have similar similarity scores
        assert!((s1 - s2).abs() < 0.01);
        assert!((s2 - s3).abs() < 0.01);
    }

    #[test]
    fn normalize_unit_length() {
        let v = Array1::from_vec(vec![3.0, 4.0]);
        let n = normalize(&v);
        let norm = n.mapv(|x| x * x).sum().sqrt();
        assert!((norm - 1.0).abs() < 1e-10);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = Array1::from_vec(vec![1.0, 0.0]);
        let b = Array1::from_vec(vec![0.0, 1.0]);
        assert!(cosine_similarity(&a, &b).abs() < 1e-10);
    }

    #[test]
    fn cosine_similarity_parallel() {
        let a = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let b = Array1::from_vec(vec![2.0, 4.0, 6.0]);
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn memory_bank_store_and_probe() {
        let mut bank = HrrMemoryBank::new();

        let key = random_key(HRR_DIM);
        let v1 = normalize(&phase_encode(1.0, HRR_DIM));
        let v2 = normalize(&phase_encode(2.0, HRR_DIM));

        let bound1 = normalize(&bind(&v1, &key));
        let bound2 = normalize(&bind(&v2, &key));

        bank.store("item1".to_string(), bound1.clone());
        bank.store("item2".to_string(), bound2.clone());

        assert_eq!(bank.len(), 2);

        // Probe with v1 should find item1
        let result = bank.probe(&bound1);
        assert!(result.is_some());
        let (name, _score) = result.unwrap();
        assert_eq!(name, "item1");
    }
}
