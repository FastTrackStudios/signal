//! Similarity queries — brute-force cosine over the packed feature matrix.
//! At one-shot-library scale (10^4–10^5 items × ~56 dims) a full scan is
//! sub-millisecond; no index structure earns its complexity yet.

/// Top-`k` most similar items to `query_idx`, as `(index, cosine)` sorted
/// best-first. `mask` (same length as item count) filters candidates —
/// this is the XO rule: active filters re-scope every similarity list.
pub fn similar(
    features: &[f32],
    dim: usize,
    query_idx: usize,
    k: usize,
    mask: impl Fn(usize) -> bool,
) -> Vec<(usize, f32)> {
    let count = features.len() / dim;
    let q = &features[query_idx * dim..(query_idx + 1) * dim];
    let qn = norm(q);
    let mut hits: Vec<(usize, f32)> = (0..count)
        .filter(|&i| i != query_idx && mask(i))
        .map(|i| {
            let r = &features[i * dim..(i + 1) * dim];
            (i, dot(q, r) / (qn * norm(r)).max(1e-9))
        })
        .collect();
    hits.sort_by(|a, b| b.1.total_cmp(&a.1));
    hits.truncate(k);
    hits
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn norm(a: &[f32]) -> f32 {
    dot(a, a).sqrt()
}
