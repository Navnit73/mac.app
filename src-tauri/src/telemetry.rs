//! Client for the mymacstorage.com app API (heartbeat, feature events, update checks).
//!
//! Network calls happen only here, and only when the build was given an API key
//! (`MMS_APP_API_KEY` at compile time). Without one, every call is a no-op and nothing leaves
//! the Mac. Payload rules and privacy limits live in `mymacstorage_core::telemetry`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mymacstorage_core::telemetry::{
    self as core_tm, AppUpdate, DeviceInfo, EventQueue, HeartbeatResponse, UpdateResponse,
};
use mymacstorage_core::{platform, AppError};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager};

const BASE_URL: &str = "https://mymacstorage.com/api/app/v1/";
/// Set when building a release: `MMS_APP_API_KEY=<APP_API_KEY> npm run tauri build`.
const APP_KEY: Option<&str> = option_env!("MMS_APP_API_KEY");
/// `stable` unless the build sets `MMS_UPDATE_CHANNEL=beta`.
const CHANNEL: Option<&str> = option_env!("MMS_UPDATE_CHANNEL");
const FLUSH_EVERY: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// The final send on quit must not hold the app open for long.
const QUIT_FLUSH_TIMEOUT: Duration = Duration::from_secs(3);

pub const UPDATE_EVENT: &str = "app://update-available";
pub const CHECK_UPDATES_EVENT: &str = "app://check-updates";

pub struct Telemetry {
    client: Option<reqwest::Client>,
    key: String,
    channel: String,
    device_id: String,
    session_id: String,
    app_version: String,
    build: String,
    info: DeviceInfo,
    share_usage: AtomicBool,
    queue: Mutex<EventQueue>,
    flush_now: tokio::sync::Notify,
    /// The newest update the server offered this run. Downloads open only its link.
    offered: Mutex<Option<AppUpdate>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    core_tm::rfc3339_utc(i64::try_from(secs).unwrap_or(0))
}

impl Telemetry {
    fn new(app: &AppHandle) -> Self {
        let key = APP_KEY.unwrap_or("").trim().to_string();
        let client = if key.is_empty() {
            tracing::info!(
                "app API key not set at build time: usage stats and update checks are off"
            );
            None
        } else {
            reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent(format!("MyMacStorage/{}", app.package_info().version))
                .build()
                .map_err(|e| tracing::warn!("could not create HTTP client: {e}"))
                .ok()
        };

        let mut settings = crate::settings::load();
        let device_id = match settings.device_id.clone() {
            Some(id) if !id.is_empty() => id,
            _ => {
                let id = uuid::Uuid::new_v4().to_string().to_uppercase();
                settings.device_id = Some(id.clone());
                if let Err(e) = crate::settings::save(&settings) {
                    tracing::warn!("could not save device id: {e}");
                }
                id
            }
        };

        let version = app.package_info().version.to_string();
        let build = app
            .config()
            .bundle
            .macos
            .bundle_version
            .clone()
            .unwrap_or_else(|| version.clone());
        if core_tm::parse_build(&build).is_none() {
            tracing::warn!(
                "bundle version {build:?} is not a whole number; the server will reject it"
            );
        }

        Self {
            client,
            key,
            channel: CHANNEL.unwrap_or("stable").to_string(),
            device_id,
            session_id: uuid::Uuid::new_v4().to_string().to_uppercase(),
            app_version: version,
            build,
            info: DeviceInfo {
                os_version: platform::sysctl_string("kern.osproductversion").unwrap_or_default(),
                model: platform::sysctl_string("hw.model").unwrap_or_default(),
                arch: platform::sysctl_string("hw.machine").unwrap_or_default(),
                locale: platform::locale_identifier(),
            },
            share_usage: AtomicBool::new(settings.share_usage.unwrap_or(true)),
            queue: Mutex::new(EventQueue::default()),
            flush_now: tokio::sync::Notify::new(),
            offered: Mutex::new(None),
        }
    }

    pub fn share_usage(&self) -> bool {
        self.share_usage.load(Ordering::Relaxed)
    }

    /// Queue a feature event. Cheap; sent in batches. Dropped when sharing is off or the name
    /// breaks the server's rule.
    pub fn track(&self, name: &str, props: Map<String, Value>) {
        if self.client.is_none() || !self.share_usage() {
            return;
        }
        let Some(event) = core_tm::make_event(name, props, &now_rfc3339()) else {
            tracing::warn!("dropped event with invalid name {name:?}");
            return;
        };
        if lock(&self.queue).push(event) {
            self.flush_now.notify_one();
        }
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Vec<u8>, String> {
        let client = self.client.as_ref().ok_or("disabled")?;
        let resp = client
            .post(format!("{BASE_URL}{path}"))
            .header("X-App-Key", &self.key)
            .json(body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status}"));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| e.to_string())
    }

    /// Send queued events. A failed batch is put back for the next flush (bounded).
    pub async fn flush(&self) {
        let batch = lock(&self.queue).take();
        if batch.is_empty() {
            return;
        }
        let body =
            core_tm::events_body(&self.device_id, &self.session_id, &self.app_version, &batch);
        if let Err(e) = self.post("events", &body).await {
            tracing::debug!("events not sent ({e}); will retry");
            lock(&self.queue).requeue(batch);
        }
    }

    async fn heartbeat(&self, event: &str) -> Option<HeartbeatResponse> {
        let body = core_tm::heartbeat_body(
            &self.device_id,
            &self.session_id,
            &self.app_version,
            &self.build,
            &self.channel,
            event,
            mymacstorage_core::license::is_active(),
            &self.info,
        );
        match self.post("heartbeat", &body).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| tracing::debug!("bad heartbeat response: {e}"))
                .ok(),
            Err(e) => {
                tracing::debug!("heartbeat failed: {e}");
                None
            }
        }
    }

    /// Manual "Check for Updates…". `Ok(None)` when up to date.
    pub async fn check_for_updates(&self) -> Result<Option<AppUpdate>, AppError> {
        let client = self.client.as_ref().ok_or_else(|| {
            AppError::Internal("Update checks are not set up in this build.".into())
        })?;
        let resp = client
            .get(format!("{BASE_URL}updates"))
            .query(&[
                ("build", self.build.as_str()),
                ("channel", self.channel.as_str()),
            ])
            .header("X-App-Key", &self.key)
            .send()
            .await
            .map_err(|e| AppError::Io(format!("Could not reach the update server: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::Io(format!(
                "The update server answered {}.",
                resp.status()
            )));
        }
        let body: UpdateResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Unexpected update response: {e}")))?;
        if let Some(u) = &body.update {
            *lock(&self.offered) = Some(u.clone());
        }
        Ok(body.update)
    }

    /// Remember an update from a heartbeat and tell the UI, once per build per run.
    fn offer(&self, app: &AppHandle, update: AppUpdate) {
        {
            let mut offered = lock(&self.offered);
            if offered.as_ref() == Some(&update) {
                return;
            }
            *offered = Some(update.clone());
        }
        let skipped = crate::settings::load().skipped_update_build;
        if core_tm::should_prompt(&update, skipped, false) {
            let _ = app.emit(UPDATE_EVENT, update);
        }
    }
}

/// Create the client and start the heartbeat and event loops. Call once from `setup`.
pub fn start(app: &AppHandle) {
    let telemetry = Telemetry::new(app);
    let enabled = telemetry.client.is_some();
    app.manage(telemetry);
    if !enabled {
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let t = handle.state::<Telemetry>();
        let mut event = "launch";
        loop {
            let resp = t.heartbeat(event).await;
            if let Some(update) = resp.as_ref().and_then(|r| r.update.clone()) {
                t.offer(&handle, update);
            }
            event = "ping";
            tokio::time::sleep(Duration::from_secs(core_tm::next_wait(resp.as_ref()))).await;
        }
    });

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let t = handle.state::<Telemetry>();
        loop {
            // Wake every minute, or early when the queue reaches 100 events.
            let _ = tokio::time::timeout(FLUSH_EVERY, t.flush_now.notified()).await;
            t.flush().await;
        }
    });
}

/// Queue an event from Rust code. `props` must be a JSON object of counts and sizes.
pub fn track(app: &AppHandle, name: &str, props: Value) {
    if let Some(t) = app.try_state::<Telemetry>() {
        let props = match props {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        t.track(name, props);
    }
}

/// Last chance to send queued events on quit. Blocks for at most a few seconds.
pub fn flush_on_quit(app: &AppHandle) {
    if let Some(t) = app.try_state::<Telemetry>() {
        let t = t.inner();
        if t.client.is_none() || lock(&t.queue).is_empty() {
            return;
        }
        tauri::async_runtime::block_on(async {
            let _ = tokio::time::timeout(QUIT_FLUSH_TIMEOUT, t.flush()).await;
        });
    }
}

/// Turn "Share Anonymous Usage Data" on or off. Off also drops events not yet sent.
pub fn set_share_usage(app: &AppHandle, on: bool) {
    if let Some(t) = app.try_state::<Telemetry>() {
        t.share_usage.store(on, Ordering::Relaxed);
        if !on {
            lock(&t.queue).take();
        }
    }
    let mut s = crate::settings::load();
    s.share_usage = Some(on);
    if let Err(e) = crate::settings::save(&s) {
        tracing::warn!("could not save usage sharing setting: {e}");
    }
}

// ---- Commands -------------------------------------------------------------------------------

/// Record a feature use from the UI. Props: a flat object of counts, sizes and flags only.
#[tauri::command]
pub fn track_event(
    telemetry: tauri::State<'_, Telemetry>,
    name: String,
    props: Option<Map<String, Value>>,
) -> Result<(), AppError> {
    if !core_tm::valid_event_name(&name) {
        return Err(AppError::InvalidInput("invalid event name".into()));
    }
    telemetry.track(&name, props.unwrap_or_default());
    Ok(())
}

/// An update a heartbeat already offered this run, if it should be shown. The UI asks on
/// start in case the offer arrived before it was listening.
#[tauri::command]
pub fn get_offered_update(telemetry: tauri::State<'_, Telemetry>) -> Option<AppUpdate> {
    let offered = lock(&telemetry.offered).clone()?;
    let skipped = crate::settings::load().skipped_update_build;
    core_tm::should_prompt(&offered, skipped, false).then_some(offered)
}

#[tauri::command]
pub async fn check_for_updates(
    telemetry: tauri::State<'_, Telemetry>,
) -> Result<Option<AppUpdate>, AppError> {
    telemetry.check_for_updates().await
}

/// Open the download link of the offered update with this build in the browser.
#[tauri::command]
pub fn open_update_download(
    telemetry: tauri::State<'_, Telemetry>,
    build: u64,
) -> Result<(), AppError> {
    let url = lock(&telemetry.offered)
        .as_ref()
        .filter(|u| u.build == build)
        .map(|u| u.download_url.clone())
        .ok_or_else(|| AppError::NotFound("That update is no longer offered.".into()))?;
    platform::open_https_url(&url)?;
    let mut props = Map::new();
    props.insert("build".into(), build.into());
    telemetry.track("update.download_clicked", props);
    Ok(())
}

/// "Skip This Version". Critical and required updates ignore this.
#[tauri::command]
pub fn skip_update(build: u64) -> Result<(), AppError> {
    let mut s = crate::settings::load();
    s.skipped_update_build = Some(build);
    crate::settings::save(&s)
}

/// Quit after a required update was declined or its download opened.
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
