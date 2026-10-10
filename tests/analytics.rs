//! Analytics queries, `analytics_pages` and the errors of `POST /analytics`, against the
//! responses in `tests/fixtures/analytics-*.json` (the same files as in the other SDKs).

mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use common::*;
use futures_core::Stream;
use lettermint::types::{
    AnalyticsComparison, AnalyticsGroupDimension, AnalyticsInterval, AnalyticsMetaComparison,
    AnalyticsMetric, AnalyticsMetricChange, AnalyticsMetricValues, AnalyticsPagination,
    AnalyticsQuery, AnalyticsRateBase, AnalyticsRateBases, AnalyticsResponse, AnalyticsSection,
};
use lettermint::{Error, Paginator};
use serde_json::{Value, json};

const PAGE_ONE: &str = include_str!("fixtures/analytics-page-1.json");
const PAGE_TWO: &str = include_str!("fixtures/analytics-page-2.json");
const SUMMARY_ONLY: &str = include_str!("fixtures/analytics-summary.json");

fn fixture(body: &str) -> Reply {
    Reply::text(200, "application/json", body)
}

fn query() -> AnalyticsQuery {
    AnalyticsQuery {
        metrics: vec![
            AnalyticsMetric::Delivered,
            AnalyticsMetric::Bounced,
            AnalyticsMetric::DeliveryRate,
            AnalyticsMetric::DeliveryLatencyP50Ms,
        ],
        include: Some(vec![
            AnalyticsSection::Summary,
            AnalyticsSection::TimeSeries,
            AnalyticsSection::Breakdown,
        ]),
        group_by: Some(vec![AnalyticsGroupDimension::RecipientDomain]),
        interval: Some(AnalyticsInterval::Hour),
        timezone: Some("Asia/Kolkata".into()),
        compare: Some(AnalyticsComparison::PreviousPeriod),
        limit: Some(2),
        ..Default::default()
    }
}

/// `query()` as the API receives it.
fn query_json() -> Value {
    json!({
        "metrics": ["delivered", "bounced", "delivery_rate", "delivery_latency_p50_ms"],
        "include": ["summary", "time_series", "breakdown"],
        "group_by": ["recipient_domain"],
        "interval": "hour",
        "timezone": "Asia/Kolkata",
        "compare": "previous_period",
        "limit": 2,
    })
}

fn with_cursor(mut body: Value, cursor: &str) -> Value {
    body["cursor"] = json!(cursor);
    body
}

async fn collect(mut pages: Paginator<AnalyticsResponse>) -> Vec<AnalyticsResponse> {
    let mut all = Vec::new();
    while let Some(page) = pages.next().await {
        all.push(page.unwrap());
    }
    all
}

// --- Responses --------------------------------------------------------------------------------

#[tokio::test]
async fn keeps_null_values_empty_rate_bases_and_both_timestamp_formats() {
    let transport = MockTransport::new();
    transport.reply(fixture(PAGE_ONE));
    let result = team_client(&transport).analytics(&query()).await.unwrap();

    let request = transport.last();
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(target(&request), "/analytics");
    assert_eq!(body_json(&request), query_json());

    // `Some(None)` is a metric the API sent as null; `None` is a metric it left out.
    let summary = result.data.summary.as_ref().unwrap();
    assert_eq!(
        summary.metrics,
        AnalyticsMetricValues {
            delivered: Some(Some(1200)),
            bounced: Some(Some(0)),
            delivery_rate: Some(Some(0.9836)),
            delivery_latency_p50_ms: Some(None),
            ..Default::default()
        }
    );
    assert_eq!(
        summary.rate_bases.delivery_rate,
        Some(AnalyticsRateBase {
            numerator: Some(1200),
            denominator: Some(1220)
        })
    );
    let previous = summary.previous.as_ref().unwrap();
    assert_eq!(previous.metrics.delivery_rate, Some(None));
    assert_eq!(previous.metrics.delivery_latency_p50_ms, Some(Some(812.5)));
    assert_eq!(
        previous.rate_bases.delivery_rate,
        Some(AnalyticsRateBase {
            numerator: None,
            denominator: None
        })
    );
    let change = summary.change.as_ref().unwrap();
    assert_eq!(
        change["delivery_rate"],
        AnalyticsMetricChange {
            absolute: Some(None),
            relative: Some(None),
            percentage_points: Some(None),
        }
    );
    assert_eq!(
        change["delivered"],
        AnalyticsMetricChange {
            absolute: Some(Some(100.0)),
            relative: Some(Some(0.0909)),
            percentage_points: None,
        }
    );

    // Timestamps are strings, exactly as the API sent them: bucket bounds carry the query's
    // UTC offset, the window and `generated_at` are UTC with microseconds.
    let series = result.data.time_series.as_ref().unwrap();
    let [complete, unavailable] = series.as_slice() else {
        panic!("{series:?}")
    };
    assert_eq!(complete.from, "2026-09-15T08:30:00+05:30");
    assert_eq!(complete.to, "2026-09-15T09:30:00+05:30");
    assert!(complete.available && !complete.partial);
    assert!(!unavailable.available && unavailable.partial);
    assert_eq!(unavailable.rate_bases, AnalyticsRateBases::default());
    assert_eq!(unavailable.metrics.delivered, Some(None));
    assert_eq!(
        serde_json::to_value(unavailable).unwrap(),
        json!({
            "from": "2026-09-15T09:30:00+05:30",
            "to": "2026-09-15T10:30:00+05:30",
            "available": false,
            "partial": true,
            "metrics": {"delivered": null, "bounced": null, "delivery_rate": null, "delivery_latency_p50_ms": null},
            "rate_bases": {},
        })
    );

    assert_eq!(result.meta.generated_at, "2026-09-15T04:12:30.482915Z");
    assert_eq!(result.meta.from, "2026-09-15T03:00:00.000000Z");
    assert_eq!(result.meta.last_ingested_at, None);
    assert_eq!(
        result.meta.comparison,
        Some(AnalyticsMetaComparison {
            from: "2026-09-15T01:00:00.000000Z".into(),
            to: "2026-09-15T03:00:00.000000Z".into(),
            partial: false,
        })
    );
    assert_eq!(
        result.pagination,
        AnalyticsPagination {
            total_groups: 3,
            returned_groups: 2,
            next_cursor: Some("cursor-page-2".into()),
            truncated: false,
        }
    );
}

#[tokio::test]
async fn leaves_out_the_sections_and_comparison_a_query_did_not_ask_for() {
    let transport = MockTransport::new();
    transport.reply(fixture(SUMMARY_ONLY));
    let result = team_client(&transport)
        .analytics(&AnalyticsQuery {
            metrics: vec![AnalyticsMetric::Delivered],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        body_json(&transport.last()),
        json!({"metrics": ["delivered"]})
    );

    let summary = result.data.summary.as_ref().unwrap();
    assert_eq!(summary.metrics.delivered, Some(Some(1200)));
    assert_eq!(summary.metrics.bounced, None);
    assert_eq!(summary.rate_bases, AnalyticsRateBases::default());
    assert_eq!(summary.previous, None);
    assert_eq!(summary.change, None);
    assert_eq!(result.data.time_series, None);
    assert_eq!(result.data.breakdown, None);
    assert_eq!(result.meta.comparison, None);
    assert_eq!(result.pagination.next_cursor, None);
    assert_eq!(
        serde_json::to_value(&result.data).unwrap(),
        json!({"summary": {"metrics": {"delivered": 1200}, "rate_bases": {}}})
    );
}

#[tokio::test]
async fn keeps_a_null_dimension_value_in_a_breakdown_row() {
    let transport = MockTransport::new();
    transport.reply(fixture(PAGE_TWO));
    let result = team_client(&transport)
        .analytics(&AnalyticsQuery {
            cursor: Some("cursor-page-2".into()),
            ..query()
        })
        .await
        .unwrap();
    let breakdown = result.data.breakdown.unwrap();
    assert_eq!(breakdown.len(), 1);
    assert_eq!(
        breakdown[0].dimensions,
        BTreeMap::from([("recipient_domain".to_owned(), None)])
    );
    assert_eq!(breakdown[0].metrics.bounced, Some(None));
    assert_eq!(breakdown[0].metrics.delivered, Some(Some(50)));
}

// --- analytics_pages --------------------------------------------------------------------------

#[tokio::test]
async fn analytics_pages_follows_next_cursor_and_yields_every_response() {
    let transport = MockTransport::new();
    transport.reply(fixture(PAGE_ONE));
    transport.reply(fixture(PAGE_TWO));
    let sent = query();
    let pages = collect(both_client(&transport).analytics_pages(&sent)).await;

    assert_eq!(pages.len(), 2);
    let requests = transport.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.method, http::Method::POST);
        assert_eq!(target(request), "/analytics");
        assert_eq!(
            header(request, "authorization"),
            Some(format!("Bearer {TEAM_TOKEN}").as_str())
        );
        assert_eq!(header(request, "x-lettermint-token"), None);
    }
    assert_eq!(body_json(&requests[0]), query_json());
    assert_eq!(
        body_json(&requests[1]),
        with_cursor(query_json(), "cursor-page-2")
    );
    assert_eq!(sent, query());

    let domains: Vec<Option<&str>> = pages
        .iter()
        .flat_map(|page| page.data.breakdown.iter().flatten())
        .map(|row| row.dimensions["recipient_domain"].as_deref())
        .collect();
    assert_eq!(domains, [Some("gmail.com"), Some("outlook.com"), None]);
    assert_eq!(pages[0].pagination.returned_groups, 2);
    assert_eq!(pages[1].pagination.returned_groups, 1);
    assert_eq!(pages[1].pagination.next_cursor, None);
}

#[tokio::test]
async fn analytics_pages_makes_one_request_for_a_query_without_more_pages() {
    let transport = MockTransport::new();
    transport.always(fixture(SUMMARY_ONLY));
    let pages = collect(team_client(&transport).analytics_pages(&AnalyticsQuery {
        metrics: vec![AnalyticsMetric::Delivered],
        ..Default::default()
    }))
    .await;
    assert_eq!(pages.len(), 1);
    assert_eq!(transport.count(), 1);
}

#[tokio::test]
async fn analytics_pages_requests_the_next_page_only_when_it_is_asked_for() {
    let transport = MockTransport::new();
    transport.always(fixture(PAGE_ONE));
    let mut pages = team_client(&transport).analytics_pages(&query());
    assert_eq!(transport.count(), 0);
    let page = pages.next().await.unwrap().unwrap();
    assert_eq!(
        page.pagination.next_cursor.as_deref(),
        Some("cursor-page-2")
    );
    assert_eq!(transport.count(), 1);
    drop(pages);
    assert_eq!(transport.count(), 1);
}

#[tokio::test]
async fn analytics_pages_stops_when_the_api_repeats_a_cursor() {
    let transport = MockTransport::new();
    transport.always(fixture(PAGE_ONE));
    // Read as a `Stream`, like the paginators of the `iterate` methods.
    let mut pages = team_client(&transport).analytics_pages(&query());
    let mut count = 0;
    while let Some(page) =
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut pages).poll_next(cx)).await
    {
        page.unwrap();
        count += 1;
    }
    assert_eq!(count, 2);
    assert_eq!(transport.count(), 2);
}

#[tokio::test]
async fn analytics_pages_stops_when_the_api_returns_the_cursor_the_query_started_from() {
    let transport = MockTransport::new();
    transport.always(fixture(PAGE_ONE));
    let pages = collect(team_client(&transport).analytics_pages(&AnalyticsQuery {
        cursor: Some("cursor-page-2".into()),
        ..query()
    }))
    .await;
    assert_eq!(pages.len(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        body_json(&requests[0]),
        with_cursor(query_json(), "cursor-page-2")
    );
}

#[tokio::test]
async fn analytics_pages_uses_the_client_timeout_for_every_page_and_surfaces_an_expired_cursor() {
    let message = "The analytics cursor is invalid or expired. Submit a new query.";
    let transport = MockTransport::new();
    transport.reply(fixture(PAGE_ONE));
    transport.reply(Reply::json(
        422,
        json!({"message": message, "errors": {"cursor": [message]}}),
    ));
    let client = team_client(&transport)
        .with_timeout(Duration::from_secs(7))
        .unwrap();
    let mut pages = client.analytics_pages(&query());
    assert!(pages.next().await.unwrap().is_ok());
    let error = pages.next().await.unwrap().unwrap_err();
    let Error::Validation(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(api.status(), 422);
    assert_eq!(api.message(), message);
    assert_eq!(
        api.errors(),
        Some(&BTreeMap::from([(
            "cursor".to_owned(),
            vec![message.to_owned()]
        )]))
    );
    // After an error the paginator ends.
    assert!(pages.next().await.is_none());

    let requests = transport.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.timeout, Duration::from_secs(7));
    }
}

#[tokio::test]
async fn analytics_pages_reports_a_missing_team_token_on_first_use() {
    let transport = MockTransport::new();
    let mut pages = sending_client(&transport).analytics_pages(&query());
    let error = pages.next().await.unwrap().unwrap_err();
    assert!(matches!(error, Error::Config(_)), "{error:?}");
    assert!(error.to_string().contains("`team_token`"), "{error}");
    assert!(pages.next().await.is_none());
    assert_eq!(transport.count(), 0);
}

// --- Errors -----------------------------------------------------------------------------------

#[tokio::test]
async fn reads_field_errors_from_a_422() {
    let message = "smtp_response_group can only be used in group_by.";
    let transport = MockTransport::new();
    transport.reply(Reply::json(
        422,
        json!({"message": message, "errors": {"filters": [message]}}),
    ));
    let error = team_client(&transport)
        .analytics(&query())
        .await
        .unwrap_err();
    let Error::Validation(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(error.status(), Some(422));
    assert_eq!(api.message(), message);
    assert_eq!(
        api.errors(),
        Some(&BTreeMap::from([(
            "filters".to_owned(),
            vec![message.to_owned()]
        )]))
    );
}

#[tokio::test]
async fn reads_retry_after_from_a_503() {
    let transport = MockTransport::new();
    transport.reply(
        Reply::json(
            503,
            json!({"error": {"code": "SERVICE_UNAVAILABLE", "message": "Try again shortly."}}),
        )
        .header("retry-after", "2"),
    );
    let error = team_client(&transport)
        .analytics(&query())
        .await
        .unwrap_err();
    let Error::Server(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(api.status(), 503);
    assert_eq!(api.code(), Some("SERVICE_UNAVAILABLE"));
    assert_eq!(api.message(), "Try again shortly.");
    assert_eq!(api.retry_after(), Some(Duration::from_secs(2)));
    assert!(error.is_retryable());
}

#[tokio::test]
async fn has_no_retry_after_for_a_503_or_504_without_the_header() {
    let transport = MockTransport::new();
    let client = team_client(&transport);
    transport.reply(Reply::json(
        503,
        json!({"message": "Analytics is unavailable."}),
    ));
    let error = client.analytics(&query()).await.unwrap_err();
    let Error::Server(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(api.status(), 503);
    assert_eq!(api.retry_after(), None);

    let message =
        "Analytics exceeded the query time limit. Retry with a shorter period or fewer dimensions.";
    transport.reply(Reply::json(504, json!({"message": message})));
    let error = client.analytics(&query()).await.unwrap_err();
    let Error::Server(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(api.status(), 504);
    assert_eq!(api.message(), message);
    assert_eq!(api.retry_after(), None);
}
