//! 2D projection: PCA (own top-k power iteration) to compact the feature
//! matrix, then Barnes-Hut t-SNE (cosine-ish via normalized vectors +
//! euclidean) down to the map plane, normalized to 0..1.

/// Reduce `data` (row-major, `count` x `dim`) to `k` principal components.
pub fn pca(data: &[f32], count: usize, dim: usize, k: usize) -> Vec<f32> {
    assert_eq!(data.len(), count * dim);
    let k = k.min(dim).min(count.max(1));
    // Center.
    let mut mean = vec![0.0f64; dim];
    for row in data.chunks_exact(dim) {
        for (m, &v) in mean.iter_mut().zip(row) {
            *m += v as f64;
        }
    }
    for m in mean.iter_mut() {
        *m /= count.max(1) as f64;
    }
    let mut centered = vec![0.0f64; count * dim];
    for (r, row) in data.chunks_exact(dim).enumerate() {
        for (c, &v) in row.iter().enumerate() {
            centered[r * dim + c] = v as f64 - mean[c];
        }
    }
    // Top-k eigenvectors of X^T X by power iteration + deflation.
    // Deterministic seed vectors (no RNG — resume-safe by construction).
    let mut components: Vec<Vec<f64>> = Vec::with_capacity(k);
    for comp_i in 0..k {
        let mut v: Vec<f64> = (0..dim)
            .map(|i| (((i * 2654435761 + comp_i * 40503) % 1000) as f64 / 500.0) - 1.0)
            .collect();
        for _ in 0..60 {
            // w = X^T (X v)
            let mut xv = vec![0.0f64; count];
            for (r, row) in centered.chunks_exact(dim).enumerate() {
                let mut s = 0.0;
                for (a, b) in row.iter().zip(&v) {
                    s += a * b;
                }
                xv[r] = s;
            }
            let mut w = vec![0.0f64; dim];
            for (r, row) in centered.chunks_exact(dim).enumerate() {
                let s = xv[r];
                for (wc, a) in w.iter_mut().zip(row) {
                    *wc += a * s;
                }
            }
            // Deflate against previous components.
            for prev in &components {
                let dot: f64 = w.iter().zip(prev).map(|(a, b)| a * b).sum();
                for (wc, p) in w.iter_mut().zip(prev) {
                    *wc -= dot * p;
                }
            }
            let norm: f64 = w.iter().map(|x| x * x).sum::<f64>().sqrt();
            if norm < 1e-12 {
                break;
            }
            for (vc, wc) in v.iter_mut().zip(&w) {
                *vc = wc / norm;
            }
        }
        components.push(v);
    }
    // Project.
    let mut out = vec![0.0f32; count * k];
    for (r, row) in centered.chunks_exact(dim).enumerate() {
        for (c, comp) in components.iter().enumerate() {
            let mut s = 0.0;
            for (a, b) in row.iter().zip(comp) {
                s += a * b;
            }
            out[r * k + c] = s as f32;
        }
    }
    out
}

/// Project to 2D map coordinates (0..1). PCA-compact → t-SNE for real sets;
/// small sets (< 32) fall straight through PCA-2D.
pub fn project_2d(data: &[f32], count: usize, dim: usize) -> Vec<(f32, f32)> {
    if count == 0 {
        return Vec::new();
    }
    let coords: Vec<(f32, f32)> = if count < 32 {
        let p = pca(data, count, dim, 2);
        (0..count).map(|i| (p[i * 2], p[i * 2 + 1])).collect()
    } else {
        let k = 24.min(dim);
        let compact = pca(data, count, dim, k);
        // L2-normalize rows so euclidean ≈ cosine distance.
        let rows: Vec<Vec<f32>> = compact
            .chunks_exact(k)
            .map(|r| {
                let n = r.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
                r.iter().map(|v| v / n).collect()
            })
            .collect();
        let perplexity = (count as f32 / 12.0).clamp(5.0, 40.0);
        let mut tsne = bhtsne::tSNE::new(&rows);
        tsne.embedding_dim(2)
            .perplexity(perplexity)
            .epochs(750)
            .barnes_hut(0.5, |a, b| {
                a.iter()
                    .zip(b.iter())
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum::<f32>()
                    .sqrt()
            });
        let emb = tsne.embedding();
        emb.chunks_exact(2).map(|c| (c[0], c[1])).collect()
    };
    normalize_01(coords)
}

fn normalize_01(coords: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &(x, y) in &coords {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let sx = (max_x - min_x).max(1e-9);
    let sy = (max_y - min_y).max(1e-9);
    coords
        .into_iter()
        .map(|(x, y)| ((x - min_x) / sx, (y - min_y) / sy))
        .collect()
}
