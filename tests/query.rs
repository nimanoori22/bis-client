use bis_client::Query;

#[test]
fn versioned_flow_multicountry_key_and_inclusive_filters() {
    let mut q = Query::new("BIS,WS_CBPOL,1.0", "D.JP+US");
    q.start_period = Some("2025-01-23".into());
    q.end_period = Some("2025-01-28".into());
    q.last_n_observations = Some(4);
    let url = q.url().unwrap();
    assert_eq!(url.host_str(), Some("stats.bis.org"));
    assert!(url.path().contains("D.JP+US/all"));
    let params: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(params["startPeriod"], "2025-01-23");
    assert_eq!(params["endPeriod"], "2025-01-28");
    assert_eq!(params["lastNObservations"], "4");
    assert_eq!(params["detail"], "full");
}

#[test]
fn wildcard_and_common_sdmx_reporting_periods_are_supported() {
    for (start, end) in [
        ("2024", "2025"),
        ("2024-Q1", "2024-Q4"),
        ("2024-01", "2024-12"),
        ("2024-S1", "2024-S2"),
        ("2024-02-29", "2024-02-29"),
    ] {
        let mut q = Query::new("WS_CBPOL", "D.");
        q.start_period = Some(start.into());
        q.end_period = Some(end.into());
        q.url().unwrap();
    }
}

#[test]
fn invalid_periods_ranges_and_limits_are_rejected() {
    for start in [
        "+025",
        "2025-+1",
        "",
        "2025-02-29",
        "2025-13",
        "2025-Q5",
        "2025-S3",
        "yesterday",
        "0000",
        "9999",
        "2025?x=1",
        "2024-",
    ] {
        let mut q = Query::new("WS_CBPOL", "D.JP");
        q.start_period = Some(start.into());
        assert!(q.url().is_err(), "accepted {start}");
    }
    let mut q = Query::new("WS_CBPOL", "D.JP");
    q.start_period = Some("2025".into());
    q.end_period = Some("2024".into());
    assert!(q.url().is_err());
    q.start_period = None;
    q.end_period = None;
    q.first_n_observations = Some(0);
    assert!(q.url().is_err());
}

#[test]
fn path_and_query_injection_are_rejected() {
    for (flow, key) in [
        ("../data", "D.JP"),
        ("WS_CBPOL", "D.JP?format=json"),
        ("WS_CBPOL", "../all"),
        ("BIS,,1.0", "D.JP"),
        ("", "all"),
        ("WS_CBPOL", ""),
    ] {
        assert!(Query::new(flow, key).url().is_err());
    }
}

#[test]
fn additional_reporting_periods_and_wildcards_are_supported() {
    for period in [
        "2024-W01",
        "2020-W53",
        "2024-D366",
        "2024-M12",
        "2024-T3",
        "2024-A1",
        "2024Q1",
    ] {
        let mut q = Query::new("WS_EER", "M.N+R.B.");
        q.start_period = Some(period.into());
        assert!(q.url().is_ok(), "{period}");
    }
    for period in ["2021-W53", "2023-D366", "2024-D000", "2024-T4", "2024-M00"] {
        let mut q = Query::new("WS_EER", "all");
        q.start_period = Some(period.into());
        assert!(q.url().is_err(), "{period}");
    }
}

#[test]
fn malformed_key_alternatives_are_rejected() {
    for key in ["M.N++R.B.CH", "M.+N.B.CH", "M.N+.B.CH"] {
        assert!(Query::new("WS_EER", key).url().is_err(), "{key}");
    }
}
