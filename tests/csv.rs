use bis_client::parse_csv;

#[test]
fn official_daily_fixture_preserves_values_and_source_metadata() {
    let data = parse_csv(include_bytes!("fixtures/japan_daily.csv")).unwrap();
    assert_eq!(data.observations.len(), 4);
    let first = &data.observations[0];
    assert_eq!(first.period, "2025-01-23");
    assert_eq!(first.value, Some(0.25));
    assert_eq!(first.fields["REF_AREA"], "JP");
    assert_eq!(first.fields["UNIT_MEASURE"], "368");
    assert!(first.fields["SOURCE_REF"].contains("Bank of Japan"));
    assert!(first.fields["COMPILATION"].contains("27 Jan 2025"));
    assert_eq!(data.observations[2].value, Some(0.5));
}

#[test]
fn missing_zero_negative_and_quarterly_periods_remain_distinct() {
    let data = parse_csv(b"TIME_PERIOD,OBS_VALUE,OBS_STATUS,EXTRA\n2024-Q1,,M,unknown\n2024-Q2,0,A,\n2024-Q3,-0.1,A,\n").unwrap();
    assert_eq!(data.observations[0].value, None);
    assert_eq!(data.observations[0].fields["EXTRA"], "unknown");
    assert_eq!(data.observations[1].value, Some(0.0));
    assert_eq!(data.observations[2].value, Some(-0.1));
}

#[test]
fn quoted_multiline_fields_and_bom_are_supported() {
    let data = parse_csv(
        "\u{feff}OBS_VALUE,TIME_PERIOD,NOTE\r\n0.25,2025-01,\"a,b\nsecond line\"\r\n".as_bytes(),
    )
    .unwrap();
    assert_eq!(data.observations[0].fields["NOTE"], "a,b\nsecond line");
}

#[test]
fn malformed_data_is_rejected_instead_of_silently_dropped() {
    for input in [
        "",
        "<html>upstream error</html>",
        "DATE,OBS_VALUE\n2025,1\n",
        "TIME_PERIOD,OBS_VALUE,OBS_VALUE\n2025,1,2\n",
        "TIME_PERIOD,OBS_VALUE\n,1\n",
        "TIME_PERIOD,OBS_VALUE\n2025,oops\n",
        "TIME_PERIOD,OBS_VALUE\n2025,NaN\n",
        "TIME_PERIOD,OBS_VALUE\n2025,inf\n",
        "TIME_PERIOD,OBS_VALUE\n2025,1,extra\n",
        "TIME_PERIOD,OBS_VALUE,\n2025,1,extra\n",
    ] {
        assert!(parse_csv(input.as_bytes()).is_err(), "accepted {input:?}");
    }
}

#[test]
fn a_header_only_response_is_an_empty_dataset() {
    assert!(
        parse_csv(b"TIME_PERIOD,OBS_VALUE\n")
            .unwrap()
            .observations
            .is_empty()
    );
}

#[test]
fn official_monthly_fixture_keeps_month_labels() {
    let data = parse_csv(include_bytes!("fixtures/japan_monthly.csv")).unwrap();
    assert_eq!(
        data.observations
            .iter()
            .map(|r| (r.period.as_str(), r.value))
            .collect::<Vec<_>>(),
        vec![
            ("2025-01", Some(0.5)),
            ("2025-02", Some(0.5)),
            ("2025-03", Some(0.5))
        ]
    );
    assert_eq!(data.observations[0].fields["FREQ"], "M");
}

#[test]
fn non_utf8_data_is_rejected() {
    assert!(parse_csv(b"TIME_PERIOD,OBS_VALUE\n2025,\xff\n").is_err());
}

#[test]
fn table_parser_handles_key_only_csv_and_rejects_invalid_headers() {
    let table = bis_client::parse_table(b"REF_AREA,FREQ\nCH,M\n").unwrap();
    assert_eq!(table.rows[0]["REF_AREA"], "CH");
    for bytes in [
        b"".as_slice(),
        b"FREQ,FREQ\nM,Q\n",
        b"FREQ,\nM,x\n",
        b"FREQ\nM,extra\n",
    ] {
        assert!(bis_client::parse_table(bytes).is_err());
    }
}
