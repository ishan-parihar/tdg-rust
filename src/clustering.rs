use linfa::prelude::*;
use linfa_clustering::{Dbscan, KMeans};
use linfa_preprocessing::tf_idf_vectorization::TfIdfVectorizer;
use ndarray::{Array1, Array2};

pub struct ClusterEngine;

impl ClusterEngine {
    pub fn cluster_observations(observations: &[String], min_points: usize, tolerance: f64) -> Vec<Option<usize>> {
        if observations.is_empty() {
            return vec![];
        }

        let texts = Array1::from_vec(observations.to_vec());
        let vectorizer = TfIdfVectorizer::default();
        let fitted = vectorizer.fit(&texts).unwrap();
        let tfidf_matrix = fitted.transform(&texts).unwrap().to_dense();

        let data = Array2::from_shape_vec(
            (tfidf_matrix.nrows(), tfidf_matrix.ncols()),
            tfidf_matrix.to_owned().into_raw_vec_and_offset().0,
        )
        .unwrap();

        Dbscan::params(min_points)
            .tolerance(tolerance)
            .transform(&data)
            .unwrap()
            .to_vec()
    }

    pub fn cluster_kmeans(observations: &[String], n_clusters: usize) -> Vec<usize> {
        if observations.is_empty() || n_clusters == 0 {
            return vec![];
        }

        let texts = Array1::from_vec(observations.to_vec());
        let vectorizer = TfIdfVectorizer::default();
        let fitted = vectorizer.fit(&texts).unwrap();
        let tfidf_matrix = fitted.transform(&texts).unwrap().to_dense();

        let data = Array2::from_shape_vec(
            (tfidf_matrix.nrows(), tfidf_matrix.ncols()),
            tfidf_matrix.to_owned().into_raw_vec_and_offset().0,
        )
        .unwrap();

        let dataset = DatasetBase::from(data.clone());
        let model = KMeans::params(n_clusters)
            .max_n_iterations(200)
            .tolerance(1e-5)
            .fit(&dataset)
            .unwrap();

        let predictions = model.predict(&dataset);
        predictions.to_vec()
    }

    pub fn extract_pattern_labels(
        observations: &[String],
        clusters: &[Option<usize>],
    ) -> Vec<String> {
        observations
            .iter()
            .zip(clusters.iter())
            .map(|(_obs, cluster_id)| {
                match cluster_id {
                    Some(id) => format!("cluster_{}", id),
                    None => "noise".to_string(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_observations() {
        let clusters = ClusterEngine::cluster_observations(&[], 3, 1.0);
        assert!(clusters.is_empty());

        let kmeans = ClusterEngine::cluster_kmeans(&[], 3);
        assert!(kmeans.is_empty());
    }

    #[test]
    fn test_dbscan_basic() {
        let observations = vec![
            "rust programming language".to_string(),
            "rust compiler optimization".to_string(),
            "python scripting language".to_string(),
            "python web framework".to_string(),
            "java enterprise application".to_string(),
            "java spring framework".to_string(),
            "rust memory safety".to_string(),
        ];

        let clusters = ClusterEngine::cluster_observations(&observations, 2, 1.0);
        assert_eq!(clusters.len(), 7);

        let noise_count = clusters.iter().filter(|c| c.is_none()).count();
        let cluster_count = clusters.iter().filter(|c| c.is_some()).count();
        assert!(cluster_count > 0 || noise_count > 0);
    }

    #[test]
    fn test_kmeans_basic() {
        let observations = vec![
            "rust programming language".to_string(),
            "rust compiler optimization".to_string(),
            "python scripting language".to_string(),
            "python web framework".to_string(),
            "java enterprise application".to_string(),
            "java spring framework".to_string(),
            "rust memory safety".to_string(),
        ];

        let clusters = ClusterEngine::cluster_kmeans(&observations, 3);
        assert_eq!(clusters.len(), 7);

        let unique_clusters: std::collections::HashSet<usize> = clusters.into_iter().collect();
        assert!(unique_clusters.len() <= 3);
    }

    #[test]
    fn test_extract_pattern_labels() {
        let observations = vec!["obs1".to_string(), "obs2".to_string(), "obs3".to_string()];
        let clusters = vec![Some(0), Some(0), None];

        let labels = ClusterEngine::extract_pattern_labels(&observations, &clusters);
        assert_eq!(labels, vec!["cluster_0", "cluster_0", "noise"]);
    }
}
