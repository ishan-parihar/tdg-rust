use proptest::prelude::*;

proptest! {
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

}
