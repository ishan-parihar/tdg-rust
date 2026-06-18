use ndarray::Array1;
use proptest::prelude::*;

fn vec_f64(min: f64, max: f64, len: usize) -> impl Strategy<Value = Array1<f64>> {
    prop::collection::vec(min..max, len).prop_map(Array1::from_vec)
}

proptest! {
    #[test]
    fn hrr_bind_unbind_roundtrip(a in vec_f64(-1.0, 1.0, 64), b in vec_f64(-1.0, 1.0, 64)) {
        let bound = tdg_rust::hrr::bind(&a, &b);
        let unbound = tdg_rust::hrr::unbind(&bound, &b);
        let norm_a = a.mapv(|x| x * x).sum().sqrt();
        if norm_a > 1e-10 {
            let sim = tdg_rust::hrr::cosine_similarity(&unbound, &a);
            prop_assert!(sim > 0.3, "cosine_similarity after bind/unbind = {}", sim);
        }
    }

    #[test]
    fn cosine_similarity_bounded(a in vec_f64(-10.0, 10.0, 64), b in vec_f64(-10.0, 10.0, 64)) {
        let sim = tdg_rust::hrr::cosine_similarity(&a, &b);
        prop_assert!(sim >= -1.01 && sim <= 1.01, "cosine_similarity = {}", sim);
    }

    #[test]
    fn normalize_unit_length(v in vec_f64(-10.0, 10.0, 64)) {
        let normalized = tdg_rust::hrr::normalize(&v);
        let norm = normalized.mapv(|x| x * x).sum().sqrt();
        prop_assert!(norm < 0.01 || (norm - 1.0).abs() < 0.01, "norm after normalize = {}", norm);
    }

    #[test]
    fn cosine_similarity_self(v in vec_f64(-10.0, 10.0, 64)) {
        let sim = tdg_rust::hrr::cosine_similarity(&v, &v);
        let norm = v.mapv(|x| x * x).sum().sqrt();
        if norm > 1e-10 {
            prop_assert!((sim - 1.0).abs() < 0.01, "self similarity = {}", sim);
        }
    }

    #[test]
    fn dual_pole_drive_net_invariant(positive in -10.0f64..10.0, negative in -10.0f64..10.0) {
        let drive = tdg_rust::flow::DualPoleDrive::new(positive, negative);
        let expected_net = positive.clamp(-10.0, 10.0) - negative.clamp(-10.0, 10.0);
        prop_assert!((drive.net() - expected_net).abs() < 1e-10, "net = {}, expected = {}", drive.net(), expected_net);
    }

    #[test]
    fn dual_pole_drive_variance_non_negative(positive in -10.0f64..10.0, negative in -10.0f64..10.0) {
        let drive = tdg_rust::flow::DualPoleDrive::new(positive, negative);
        prop_assert!(drive.variance() >= 0.0, "variance = {}", drive.variance());
    }

    #[test]
    fn dual_pole_drive_variance_bounded(positive in -10.0f64..10.0, negative in -10.0f64..10.0) {
        let drive = tdg_rust::flow::DualPoleDrive::new(positive, negative);
        prop_assert!(drive.variance() <= 1.01, "variance = {}", drive.variance());
    }

    #[test]
    fn dual_pole_drive_diagnosis_always_valid(positive in -10.0f64..10.0, negative in -10.0f64..10.0) {
        use tdg_rust::flow::DriveDiagnosis;
        let drive = tdg_rust::flow::DualPoleDrive::new(positive, negative);
        let diag = drive.diagnose();
        let _ = match diag {
            DriveDiagnosis::Integrated => "Integrated",
            DriveDiagnosis::Addiction => "Addiction",
            DriveDiagnosis::Allergy => "Allergy",
            DriveDiagnosis::BlindSpot => "BlindSpot",
            DriveDiagnosis::TensionPair => "TensionPair",
        };
    }

    #[test]
    fn dual_pole_drive_clamped(positive in -1000.0f64..1000.0, negative in -1000.0f64..1000.0) {
        let drive = tdg_rust::flow::DualPoleDrive::new(positive, negative);
        prop_assert!(drive.positive_pole >= -10.0 && drive.positive_pole <= 10.0);
        prop_assert!(drive.negative_pole >= -10.0 && drive.negative_pole <= 10.0);
    }

    #[test]
    fn pathfind_empty_db_no_path(source_id in "[a-z]{3,8}", target_id in "[a-z]{3,8}") {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        tdg_rust::init_schema(&conn).unwrap();
        tdg_rust::run_migrations(&conn).unwrap();
        let result = tdg_rust::db::crud::pathfind(&conn, &source_id, &target_id, 5, 100);
        match result {
            Ok(paths) => prop_assert!(paths.is_empty()),
            Err(_) => (),
        }
    }

    #[test]
    fn phase_encode_dimensional(value in -100.0f64..100.0) {
        let v = tdg_rust::hrr::phase_encode(value, 64);
        prop_assert_eq!(v.len(), 64);
    }

    #[test]
    fn bundle_size_preserved(vectors in prop::collection::vec(vec_f64(-1.0, 1.0, tdg_rust::hrr::HRR_DIM), 0..5)) {
        let vecs: Vec<Array1<f64>> = vectors;
        let result = tdg_rust::hrr::bundle(&vecs);
        prop_assert_eq!(result.len(), tdg_rust::hrr::HRR_DIM);
        if vecs.is_empty() {
            prop_assert!(result.iter().all(|&x| x == 0.0));
        }
    }
}
