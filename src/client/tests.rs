use super::*;
use crate::orchestration::retry_delay;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

fn response(status: &str, content_type: &str, body: &str, extra: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )
}

async fn server(responses: Vec<String>) -> (Client, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for response in responses {
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0 && request.len() < 65536);
                request.extend_from_slice(&buffer[..count]);
            }
            requests.push(String::from_utf8(request).unwrap());
            // A client may close early when Content-Length already exceeds its limit.
            let _ = stream.write_all(response.as_bytes()).await;
        }
        requests
    });
    let mut client =
        Client::with_http_client(reqwest::Client::builder().no_proxy().build().unwrap());
    client.base_url = format!("http://{address}/api/v1/");
    (client, task)
}

#[tokio::test]
async fn downloads_csv_with_metadata_and_exact_replay_bytes() {
    let body = include_str!("../../tests/fixtures/japan_daily.csv");
    let (client, task) = server(vec![response(
        "200 OK",
        "text/csv;charset=UTF-8",
        body,
        "ETag: example-tag\r\nLast-Modified: Mon, 27 Jan 2025 00:00:00 GMT\r\n",
    )])
    .await;
    let result = client.fetch(&Query::new("WS_CBPOL", "D.JP")).await.unwrap();
    assert_eq!(result.body, body.as_bytes());
    assert_eq!(result.data.observations.len(), 4);
    assert_eq!(result.status, 200);
    assert_eq!(result.etag.as_deref(), Some("example-tag"));
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/manifest.json")).unwrap();
    assert_eq!(result.sha256, manifest["sha256"].as_str().unwrap());
    let requests = task.await.unwrap();
    assert!(requests[0].contains("/api/v1/data/WS_CBPOL/D.JP/all?detail=full"));
    assert!(
        requests[0]
            .to_lowercase()
            .contains("accept: application/vnd.sdmx.data+csv;version=1.0.0")
    );
}

#[tokio::test]
async fn retries_rate_limit_then_returns_success() {
    let (client, task) = server(vec![
        response(
            "429 Too Many Requests",
            "text/plain",
            "slow down",
            "Retry-After: 0\r\n",
        ),
        response(
            "200 OK",
            "text/csv",
            "TIME_PERIOD,OBS_VALUE\n2025-01,0\n",
            "",
        ),
    ])
    .await;
    let result = client.fetch(&Query::new("WS_CBPOL", "M.JP")).await.unwrap();
    assert_eq!(result.data.observations[0].value, Some(0.0));
    assert_eq!(task.await.unwrap().len(), 2);
}

#[tokio::test]
async fn retries_are_bounded_and_long_retry_after_is_not_ignored() {
    let unavailable = response(
        "503 Unavailable",
        "text/plain",
        "maintenance",
        "Retry-After: 0\r\n",
    );
    let (client, task) = server(vec![unavailable; 3]).await;
    assert!(matches!(
        client.fetch(&Query::new("WS_CBPOL", "D.JP")).await,
        Err(Error::Orchestration(rustopus::Error::Exhausted {
            attempts: 3,
            ..
        }))
    ));
    assert_eq!(task.await.unwrap().len(), 3);
    let (client, task) = server(vec![response(
        "429 Limited",
        "text/plain",
        "later",
        "Retry-After: 3600\r\n",
    )])
    .await;
    assert!(matches!(
        client.fetch(&Query::new("WS_CBPOL", "D.JP")).await,
        Err(Error::Http { status: 429, .. })
    ));
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn not_found_stays_an_error_and_is_not_retried() {
    let (client, task) = server(vec![response(
        "404 Not Found",
        "text/plain",
        "no data or invalid selection",
        "",
    )])
    .await;
    let error = client
        .fetch(&Query::new("WS_CBPOL", "D.XX"))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Http { status: 404, .. }));
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn no_content_is_explicit_empty_success() {
    let (client, task) = server(vec![
        "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into(),
    ])
    .await;
    let result = client.fetch(&Query::new("WS_CBPOL", "D.JP")).await.unwrap();
    assert_eq!(result.status, 204);
    assert!(result.body.is_empty() && result.data.observations.is_empty());
    assert!(result.data.columns.is_empty());
    task.await.unwrap();
}

#[tokio::test]
async fn rejects_html_partial_and_malformed_responses() {
    for (status, content_type, body) in [
        ("200 OK", "text/html", "<html>error</html>"),
        (
            "206 Partial Content",
            "text/csv",
            "TIME_PERIOD,OBS_VALUE\n2025,1\n",
        ),
        ("200 OK", "text/csv", "TIME_PERIOD,OBS_VALUE\n2025,bad\n"),
    ] {
        let (client, task) = server(vec![response(status, content_type, body, "")]).await;
        assert!(client.fetch(&Query::new("WS_CBPOL", "D.JP")).await.is_err());
        assert_eq!(task.await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn response_limit_applies_to_announced_and_streamed_bodies() {
    let chunked = "HTTP/1.1 200 OK\r\nContent-Type: text/csv\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n10\r\n0123456789abcdef\r\n0\r\n\r\n";
    for reply in [
        response("200 OK", "text/csv", "TIME_PERIOD,OBS_VALUE\n2025,1\n", ""),
        chunked.into(),
    ] {
        let (client, task) = server(vec![reply]).await;
        let client = client.with_max_response_bytes(8);
        assert!(matches!(
            client.fetch(&Query::new("WS_CBPOL", "D.JP")).await,
            Err(Error::ResponseTooLarge { .. })
        ));
        task.await.unwrap();
    }
}

#[tokio::test]
async fn incomplete_body_is_discarded_before_retry() {
    let truncated = "HTTP/1.1 200 OK\r\nContent-Type: text/csv\r\nContent-Length: 500\r\nConnection: close\r\n\r\nTIME_PERIOD,OBS_VALUE\n2025,1";
    let complete = response("200 OK", "text/csv", "TIME_PERIOD,OBS_VALUE\n2025,2\n", "");
    let (client, task) = server(vec![truncated.into(), complete]).await;
    let result = client.fetch(&Query::new("WS_CBPOL", "D.JP")).await.unwrap();
    assert_eq!(result.data.observations.len(), 1);
    assert_eq!(result.data.observations[0].value, Some(2.0));
    assert_eq!(task.await.unwrap().len(), 2);
}

#[tokio::test]
async fn full_and_since_methods_do_not_confuse_periods_with_vintages() {
    let csv = response(
        "200 OK",
        "text/csv",
        "TIME_PERIOD,OBS_VALUE\n2025-01,1\n",
        "",
    );
    let (client, task) = server(vec![csv.clone(), csv]).await;
    client.fetch_full_history("WS_CBPOL", "M.JP").await.unwrap();
    client
        .fetch_since("WS_CBPOL", "M.JP", "2025-01")
        .await
        .unwrap();
    let requests = task.await.unwrap();
    assert!(!requests[0].contains("startPeriod") && !requests[0].contains("lastNObservations"));
    assert!(requests[1].contains("startPeriod=2025-01"));
    assert!(!requests[1].contains("updatedAfter"));
}

#[test]
fn retry_after_http_dates_and_invalid_headers_are_handled() {
    let mut headers = header::HeaderMap::new();
    let future = (Utc::now() + chrono::Duration::hours(1)).to_rfc2822();
    headers.insert(header::RETRY_AFTER, future.parse().unwrap());
    assert!(retry_delay(&headers, 0).is_none());
    headers.insert(
        header::RETRY_AFTER,
        "Mon, 27 Jan 2025 00:00:00 GMT".parse().unwrap(),
    );
    assert_eq!(retry_delay(&headers, 0), Some(std::time::Duration::ZERO));
    headers.insert(header::RETRY_AFTER, "invalid".parse().unwrap());
    assert_eq!(
        retry_delay(&headers, 1),
        Some(std::time::Duration::from_secs(2))
    );
}

#[tokio::test]
async fn total_deadline_includes_server_directed_waits() {
    let (mut client, task) = server(vec![response(
        "429 Limited",
        "text/plain",
        "later",
        "Retry-After: 5\r\n",
    )])
    .await;
    client.arm = client.make_arm(
        rustopus::Arm::builder()
            .policy(crate::BisPolicy)
            .total_timeout(Duration::from_millis(30)),
    );
    assert!(matches!(
        client.fetch(&Query::new("WS_CBPOL", "D.JP")).await,
        Err(Error::Orchestration(rustopus::Error::Deadline))
    ));
    task.await.unwrap();
}

#[tokio::test]
async fn discovers_dataflows_structures_codelists_and_availability() {
    let (client, task) = server(vec![
        response(
            "200 OK",
            "application/json;charset=UTF-8",
            include_str!("../../tests/fixtures/dataflow.json"),
            "",
        ),
        response(
            "200 OK",
            "application/json",
            include_str!("../../tests/fixtures/datastructure.json"),
            "",
        ),
        response(
            "200 OK",
            "application/json",
            include_str!("../../tests/fixtures/codelist.json"),
            "",
        ),
        response(
            "200 OK",
            "application/json",
            include_str!("../../tests/fixtures/availability.json"),
            "",
        ),
    ])
    .await;
    let flows = client.dataflows().await.unwrap();
    let structure_id = crate::StructureRef::from_urn(&flows[0].structure).unwrap();
    let structures = client.datastructure(&structure_id).await.unwrap();
    let dims = structures[0].dimensions();
    assert_eq!(
        dims.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        ["FREQ", "EER_TYPE", "EER_BASKET", "REF_AREA"]
    );
    let codes_id = crate::StructureRef::from_urn(
        dims[0]
            .local_representation
            .as_ref()
            .unwrap()
            .enumeration
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    let lists = client.codelist(&codes_id).await.unwrap();
    assert!(
        lists[0]
            .codes
            .iter()
            .any(|c| c.id == "M" && c.name == "Monthly")
    );
    let constraints = client
        .availability(
            &Query::new("WS_EER", "M.N.B.CH"),
            "all",
            crate::AvailabilityMode::Exact,
        )
        .await
        .unwrap();
    assert!(
        constraints[0].cube_regions[0]
            .key_values
            .iter()
            .any(|v| v.id == "REF_AREA")
    );
    let requests = task.await.unwrap();
    for (request, path) in requests.iter().zip([
        "dataflow/BIS/all/latest",
        "datastructure/BIS/BIS_EER/1.0",
        "codelist/BIS/CL_FREQ/1.0",
        "availableconstraint/WS_EER/M.N.B.CH/all/all",
    ]) {
        assert!(request.contains(path), "{request}");
        assert!(request.contains("accept: application/vnd.sdmx.structure+json;version=1.0.0"));
    }
}

#[tokio::test]
async fn key_only_detail_does_not_require_observation_columns() {
    let (client, task) = server(vec![response(
        "200 OK",
        "text/csv",
        "FREQ,REF_AREA\nM,CH\n",
        "",
    )])
    .await;
    let result = client
        .fetch_table(
            &Query::new("WS_EER", "M.N.B."),
            crate::DataDetail::SeriesKeysOnly,
        )
        .await
        .unwrap();
    assert_eq!(result.data.rows[0]["REF_AREA"], "CH");
    assert!(task.await.unwrap()[0].contains("detail=serieskeysonly"));
}

#[tokio::test]
async fn invalid_metadata_envelopes_are_errors_not_empty_catalogues() {
    for body in ["{}", "{\"data\":{}}", "{\"data\":{\"dataflows\":[{}]}}"] {
        let (client, task) = server(vec![response("200 OK", "application/json", body, "")]).await;
        assert!(client.dataflows().await.is_err());
        task.await.unwrap();
    }
}

#[tokio::test]
async fn final_redirect_url_and_header_spoofing_preserve_provenance() {
    let (client, task) = server(vec![
        response(
            "302 Found",
            "text/plain",
            "",
            "Location: /redirected.csv\r\n",
        ),
        response(
            "200 OK",
            "text/csv",
            "TIME_PERIOD,OBS_VALUE\n2024,1\n",
            "x-bis-client-internal-final-url: https://spoofed.invalid/\r\n",
        ),
    ])
    .await;
    let result = client
        .fetch(&Query::new("WS_EER", "M.N.B.CH"))
        .await
        .unwrap();
    assert_eq!(
        result.response_url,
        format!(
            "{}redirected.csv",
            client.base_url.trim_end_matches("api/v1/")
        )
    );
    assert_ne!(result.request_url, result.response_url);
    assert_eq!(task.await.unwrap().len(), 2);
}

#[tokio::test]
async fn clones_share_server_directed_pause() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buffer[..n]);
        }
        stream
            .write_all(
                response("429 Limited", "text/plain", "wait", "Retry-After: 5\r\n").as_bytes(),
            )
            .await
            .unwrap();
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err(),
            "clone sent a request during the shared pause"
        );
    });
    let mut client = Client::with_arm_builder(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        rustopus::Arm::builder()
            .policy(crate::BisPolicy)
            .total_timeout(Duration::from_millis(30)),
    );
    client.base_url = format!("http://{address}/api/v1/");
    let clone = client.clone();
    for client in [client, clone] {
        assert!(matches!(
            client.fetch(&Query::new("WS_EER", "all")).await,
            Err(Error::Orchestration(rustopus::Error::Deadline))
        ));
    }
    server.await.unwrap();
}
