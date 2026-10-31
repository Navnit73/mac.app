//! Pure logic for the mymacstorage.com app API: heartbeat payloads, feature events and update
//! offers. No network here; the Tauri shell sends what this module builds.
//!
//! Privacy: events carry only names and flat props chosen by the app (counts, sizes, durations).
//! Never put paths, file names or emails in props.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

/// Event name rule enforced by the server: letters, digits, `.` `_` `:` `-`, at most 100 chars.
pub const MAX_NAME_LEN: usize = 100;
/// At most this many props per event.
pub const MAX_PROPS: usize = 20;
/// String prop values are cut to this many characters.
pub const MAX_PROP_STR: usize = 256;
/// Send as soon as the queue holds this many events.
pub const FLUSH_AT: usize = 100;
/// Events kept for retry after a failed send.
pub const MAX_QUEUED: usize = 500;

/// A newer build offered by the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppUpdate {
    pub version: String,
    #[ts(type = "number")]
    pub build: u64,
    pub channel: String,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    pub download_url: String,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    #[serde(default)]
    pub minimum_system_version: Option<String>,
    /// Cannot be skipped.
    #[serde(default)]
    pub critical: bool,
    /// This build is below the minimum: block use until the user updates.
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResponse {
    #[serde(default = "default_interval")]
    pub next_heartbeat_seconds: u64,
    #[serde(default)]
    pub update: Option<AppUpdate>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateResponse {
    #[serde(default)]
    pub update: Option<AppUpdate>,
}

fn default_interval() -> u64 {
    60
}

/// Seconds until the next heartbeat: the server's hint (at least 30 s), or 120 s after a failure.
pub fn next_wait(response: Option<&HeartbeatResponse>) -> u64 {
    response.map_or(120, |r| r.next_heartbeat_seconds.clamp(30, 3600))
}

/// Facts about this Mac sent with every heartbeat. No names, paths or addresses.
#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    pub os_version: String,
    pub model: String,
    pub arch: String,
    pub locale: String,
}

/// Body of `POST heartbeat`.
#[allow(clippy::too_many_arguments)]
pub fn heartbeat_body(
    device_id: &str,
    session_id: &str,
    app_version: &str,
    build: &str,
    channel: &str,
    event: &str,
    licensed: bool,
    info: &DeviceInfo,
) -> Value {
    serde_json::json!({
        "deviceId": device_id,
        "sessionId": session_id,
        "appVersion": app_version,
        "build": build,
        "channel": channel,
        "event": event,
        "licensed": licensed,
        "osVersion": info.os_version,
        "model": info.model,
        "arch": info.arch,
        "locale": info.locale,
    })
}

/// Body of `POST events`.
pub fn events_body(
    device_id: &str,
    session_id: &str,
    app_version: &str,
    events: &[Value],
) -> Value {
    serde_json::json!({
        "deviceId": device_id,
        "sessionId": session_id,
        "appVersion": app_version,
        "events": events,
    })
}

/// Whether the server would accept this event name.
pub fn valid_event_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

/// Keep only what the server accepts: a flat object of at most 20 keys whose values are
/// strings (cut to 256 chars), numbers, booleans or null. Nested values are dropped.
pub fn sanitize_props(props: Map<String, Value>) -> Map<String, Value> {
    props
        .into_iter()
        .filter_map(|(k, v)| match v {
            Value::String(s) => Some((k, Value::String(s.chars().take(MAX_PROP_STR).collect()))),
            Value::Number(_) | Value::Bool(_) | Value::Null => Some((k, v)),
            Value::Array(_) | Value::Object(_) => None,
        })
        .take(MAX_PROPS)
        .collect()
}

/// One queued event, or `None` when the name is invalid.
pub fn make_event(name: &str, props: Map<String, Value>, at_rfc3339: &str) -> Option<Value> {
    if !valid_event_name(name) {
        return None;
    }
    let mut event = Map::new();
    event.insert("name".into(), name.into());
    event.insert("at".into(), at_rfc3339.into());
    let props = sanitize_props(props);
    if !props.is_empty() {
        event.insert("props".into(), Value::Object(props));
    }
    Some(Value::Object(event))
}

/// In-memory event queue with bounded retry.
#[derive(Debug, Default)]
pub struct EventQueue {
    events: Vec<Value>,
}

impl EventQueue {
    /// Add an event. Returns true when the queue should be sent now.
    pub fn push(&mut self, event: Value) -> bool {
        self.events.push(event);
        if self.events.len() > MAX_QUEUED {
            let extra = self.events.len() - MAX_QUEUED;
            self.events.drain(..extra);
        }
        self.events.len() >= FLUSH_AT
    }

    pub fn take(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.events)
    }

    /// Put a failed batch back in front, keeping at most `MAX_QUEUED` events in total.
    pub fn requeue(&mut self, mut batch: Vec<Value>) {
        let room = MAX_QUEUED.saturating_sub(self.events.len());
        batch.truncate(room);
        batch.append(&mut self.events);
        self.events = batch;
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// UTC timestamp like `2026-10-31T12:00:00Z` for seconds since the Unix epoch.
pub fn rfc3339_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Build numbers must be whole numbers; the server compares them.
pub fn parse_build(s: &str) -> Option<u64> {
    s.trim().parse().ok()
}

/// Only HTTPS download links are ever opened.
pub fn is_safe_download_url(url: &str) -> bool {
    url.len() <= 2048
        && url
            .get(..8)
            .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
        && url.len() > 8
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Whether to show this update automatically. A skipped build stays hidden unless it is
/// critical or required; a manual check always shows it.
pub fn should_prompt(update: &AppUpdate, skipped_build: Option<u64>, user_initiated: bool) -> bool {
    user_initiated
        || update.critical
        || update.required
        || skipped_build.is_none_or(|s| update.build > s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn update(build: u64) -> AppUpdate {
        AppUpdate {
            version: "1.4.0".into(),
            build,
            channel: "stable".into(),
            title: "Version 1.4.0".into(),
            notes: String::new(),
            download_url: "https://example.com/a.dmg".into(),
            size: None,
            minimum_system_version: None,
            critical: false,
            required: false,
        }
    }

    #[test]
    fn event_names_follow_server_rule() {
        assert!(valid_event_name("scan.completed"));
        assert!(valid_event_name("a:b_c-d.e9"));
        assert!(!valid_event_name(""));
        assert!(!valid_event_name("has space"));
        assert!(!valid_event_name("slash/no"));
        assert!(!valid_event_name(&"a".repeat(101)));
        assert!(valid_event_name(&"a".repeat(100)));
    }

    #[test]
    fn props_are_flat_bounded_and_cut() {
        let mut m = Map::new();
        m.insert("s".into(), json!("x".repeat(300)));
        m.insert("n".into(), json!(3));
        m.insert("b".into(), json!(true));
        m.insert("z".into(), Value::Null);
        m.insert("o".into(), json!({"a": 1}));
        m.insert("a".into(), json!([1]));
        let out = sanitize_props(m);
        assert_eq!(out.len(), 4);
        assert_eq!(out["s"].as_str().map(|s| s.chars().count()), Some(256));
        assert!(!out.contains_key("o") && !out.contains_key("a"));

        let many: Map<String, Value> = (0..30).map(|i| (format!("k{i}"), json!(i))).collect();
        assert_eq!(sanitize_props(many).len(), MAX_PROPS);
    }

    #[test]
    fn make_event_drops_bad_names_and_empty_props() {
        assert!(make_event("bad name", Map::new(), "t").is_none());
        let e = make_event("scan.started", Map::new(), "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(
            e,
            json!({"name": "scan.started", "at": "2026-01-01T00:00:00Z"})
        );
    }

    #[test]
    fn queue_flushes_at_100_and_retries_bounded() {
        let mut q = EventQueue::default();
        for i in 0..99 {
            assert!(!q.push(json!(i)));
        }
        assert!(q.push(json!(99)));
        let batch = q.take();
        assert!(q.is_empty());
        q.push(json!("new"));
        q.requeue(batch);
        assert_eq!(q.len(), 101);
        assert_eq!(q.take()[0], json!(0), "failed batch goes back in front");

        q.requeue((0..600).map(|i| json!(i)).collect());
        assert_eq!(q.len(), MAX_QUEUED);
        for i in 0..10 {
            q.push(json!(i));
        }
        assert_eq!(q.len(), MAX_QUEUED);
    }

    #[test]
    fn waits_follow_server_with_floor_and_backoff() {
        let r = |n| HeartbeatResponse {
            next_heartbeat_seconds: n,
            update: None,
        };
        assert_eq!(next_wait(None), 120);
        assert_eq!(next_wait(Some(&r(5))), 30);
        assert_eq!(next_wait(Some(&r(60))), 60);
    }

    #[test]
    fn parses_server_update_json() {
        let r: HeartbeatResponse = serde_json::from_value(json!({
            "nextHeartbeatSeconds": 60,
            "update": {"version": "1.4.0", "build": 42, "channel": "stable",
              "title": "Version 1.4.0", "notes": "Faster scans",
              "downloadUrl": "https://x/MyMacStorage-1.4.0.dmg", "critical": false, "required": true}
        }))
        .unwrap();
        let u = r.update.unwrap();
        assert_eq!(u.build, 42);
        assert!(u.required);
        let none: UpdateResponse = serde_json::from_value(json!({"update": null})).unwrap();
        assert!(none.update.is_none());
    }

    #[test]
    fn only_https_links_open() {
        assert!(is_safe_download_url("https://x.com/a.dmg"));
        assert!(is_safe_download_url("HTTPS://x.com/a.dmg"));
        assert!(!is_safe_download_url("http://x.com/a.dmg"));
        assert!(!is_safe_download_url("file:///etc/passwd"));
        assert!(!is_safe_download_url("https://"));
        assert!(!is_safe_download_url("https://x.com/a b"));
    }

    #[test]
    fn skipped_builds_hide_unless_critical_required_or_manual() {
        assert!(should_prompt(&update(42), None, false));
        assert!(!should_prompt(&update(42), Some(42), false));
        assert!(should_prompt(&update(43), Some(42), false));
        assert!(should_prompt(&update(42), Some(42), true));
        let mut c = update(42);
        c.critical = true;
        assert!(should_prompt(&c, Some(42), false));
    }

    #[test]
    fn formats_utc_timestamps() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(1_793_448_000), "2026-10-31T12:00:00Z");
    }

    #[test]
    fn build_must_be_whole_number() {
        assert_eq!(parse_build("42"), Some(42));
        assert_eq!(parse_build("1.4.0"), None);
    }
}
