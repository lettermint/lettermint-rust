//! Query string encoding, identical to the Node SDK's `serializeQuery`.
//!
//! Nested values use bracket syntax (`filter[tags][0][name]`), lists of scalars are joined with
//! commas, lists of objects are indexed, booleans are `1`/`0`, and unset values are left out. The
//! result is `application/x-www-form-urlencoded`, like `URLSearchParams`.

use crate::generated::support::QueryValue;

/// Encodes `(wire name, value)` pairs. Returns an empty string when nothing is set.
pub(crate) fn encode(params: &[(String, QueryValue)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (name, value) in params {
        append(&mut serializer, name, value);
    }
    serializer.finish()
}

fn append(serializer: &mut form_urlencoded::Serializer<'_, String>, key: &str, value: &QueryValue) {
    match value {
        QueryValue::Scalar(value) => {
            serializer.append_pair(key, value);
        }
        QueryValue::List(items) => {
            if !items.is_empty() {
                serializer.append_pair(key, &items.join(","));
            }
        }
        QueryValue::Indexed(items) => {
            for (index, item) in items.iter().enumerate() {
                append(serializer, &format!("{key}[{index}]"), item);
            }
        }
        QueryValue::Object(fields) => {
            for (name, item) in fields {
                append(serializer, &format!("{key}[{name}]"), item);
            }
        }
    }
}

/// Sets the parameter `name` (a wire name such as `page[cursor]` or `cursor`) to `value`,
/// replacing an existing one.
pub(crate) fn set_param(params: &mut Vec<(String, QueryValue)>, name: &str, value: &str) {
    let value = QueryValue::Scalar(value.to_owned());
    match params.iter_mut().find(|(key, _)| key == name) {
        Some(existing) => existing.1 = value,
        None => params.push((name.to_owned(), value)),
    }
}

/// Encodes a path segment like JavaScript's `encodeURIComponent`.
pub(crate) fn encode_path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::support::QueryParams;
    use crate::types::{
        DomainStatus, GetStatsQuery, ListDomainsQuery, ListDomainsQuerySortItem, ListMessagesQuery,
        ListMessagesQueryFilter, ListMessagesQueryFilterTagsItem, ListRoutesQuery,
        ListWebhooksQuery, MessageStatus,
    };

    fn decoded(query: &str) -> Vec<(String, String)> {
        form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect()
    }

    #[test]
    fn encodes_brackets_lists_and_booleans_like_node() {
        let query = ListDomainsQuery {
            page_size: Some(10),
            sort: Some(vec![
                ListDomainsQuerySortItem::CreatedAtDesc,
                ListDomainsQuerySortItem::Domain,
            ]),
            filter_status: Some(DomainStatus::Verified),
            ..Default::default()
        };
        let encoded = encode(&query.query_params());
        assert_eq!(
            encoded,
            "page%5Bsize%5D=10&sort=-created_at%2Cdomain&filter%5Bstatus%5D=verified"
        );
        assert_eq!(
            decoded(&encoded),
            [
                ("page[size]".into(), "10".into()),
                ("sort".into(), "-created_at,domain".into()),
                ("filter[status]".into(), "verified".into()),
            ]
        );
    }

    #[test]
    fn booleans_are_one_and_zero() {
        let query = ListWebhooksQuery {
            filter_enabled: Some(true),
            ..Default::default()
        };
        assert_eq!(encode(&query.query_params()), "filter%5Benabled%5D=1");
        let query = ListRoutesQuery {
            filter_is_default: Some(false),
            ..Default::default()
        };
        assert_eq!(encode(&query.query_params()), "filter%5Bis_default%5D=0");
    }

    #[test]
    fn object_lists_are_indexed() {
        let query = ListMessagesQuery {
            filter: Some(ListMessagesQueryFilter {
                tags: Some(vec![
                    ListMessagesQueryFilterTagsItem {
                        name: Some("campaign".into()),
                        value: Some("welcome".into()),
                    },
                    ListMessagesQueryFilterTagsItem {
                        name: Some("tier".into()),
                        value: Some("gold".into()),
                    },
                ]),
            }),
            filter_status: Some(MessageStatus::HardBounced),
            ..Default::default()
        };
        assert_eq!(
            decoded(&encode(&query.query_params())),
            [
                ("filter[tags][0][name]".into(), "campaign".into()),
                ("filter[tags][0][value]".into(), "welcome".into()),
                ("filter[tags][1][name]".into(), "tier".into()),
                ("filter[tags][1][value]".into(), "gold".into()),
                ("filter[status]".into(), "hard_bounced".into()),
            ]
        );
    }

    #[test]
    fn unset_values_and_empty_lists_are_left_out() {
        assert_eq!(encode(&ListDomainsQuery::default().query_params()), "");
        let query = ListDomainsQuery {
            sort: Some(Vec::new()),
            ..Default::default()
        };
        assert_eq!(encode(&query.query_params()), "");
    }

    #[test]
    fn unknown_enum_values_are_sent_as_given() {
        let query = ListDomainsQuery {
            filter_status: Some(DomainStatus::from("some_future_status")),
            ..Default::default()
        };
        assert_eq!(
            encode(&query.query_params()),
            "filter%5Bstatus%5D=some_future_status"
        );
    }

    #[test]
    fn required_parameters_and_spaces() {
        let query = GetStatsQuery {
            from: "2026-10-01".into(),
            to: "2026-10-31".into(),
            project_id: Some("a b&c".into()),
            include_machine: None,
        };
        assert_eq!(
            encode(&query.query_params()),
            "from=2026-10-01&to=2026-10-31&project_id=a+b%26c"
        );
    }

    #[test]
    fn set_param_replaces_or_appends() {
        let mut params = ListDomainsQuery {
            page_cursor: Some("old".into()),
            page_size: Some(5),
            ..Default::default()
        }
        .query_params();
        set_param(&mut params, "page[cursor]", "new");
        assert_eq!(
            decoded(&encode(&params))[1],
            ("page[cursor]".into(), "new".into())
        );
        let mut params = ListWebhooksQuery::default().query_params();
        set_param(&mut params, "cursor", "c1");
        assert_eq!(encode(&params), "cursor=c1");
    }

    #[test]
    fn path_segments_match_encode_uri_component() {
        assert_eq!(encode_path_segment("abc-123_.~"), "abc-123_.~");
        assert_eq!(encode_path_segment("a/b?c#d e"), "a%2Fb%3Fc%23d%20e");
        assert_eq!(encode_path_segment("!*'()"), "!*'()");
        assert_eq!(encode_path_segment("é"), "%C3%A9");
    }
}
