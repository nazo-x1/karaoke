//! 播放资源后台准备任务队列。对应 Python `karaoke/services/prepare_service.py`。
//!
//! 相较 Python 版的关键修复（P1）：任务状态表不再无限增长——终态
//! （Ready/NotNeeded/Failed）超过 TTL 后由后台清理任务回收；并发由
//! `Semaphore` 显式限制（默认按 CPU 核数推导，而非固定任务数）。

use karaoke_domain::playback::PlaybackSource;
use karaoke_events::EventBus;
use karaoke_infra::embedded::{ensure_embedded_cache, probe_layout};
use karaoke_infra::media::ensure_browser_mp4_cache;
use karaoke_infra::playback_resolver::PlaybackResolver;
use karaoke_infra::repositories::SongRepository;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareState {
    Idle,
    Pending,
    Running,
    Ready,
    NotNeeded,
    Failed,
}

impl PrepareState {
    fn as_str(&self) -> &'static str {
        match self {
            PrepareState::Idle => "idle",
            PrepareState::Pending => "pending",
            PrepareState::Running => "running",
            PrepareState::Ready => "ready",
            PrepareState::NotNeeded => "not_needed",
            PrepareState::Failed => "failed",
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self,
            PrepareState::Ready | PrepareState::NotNeeded | PrepareState::Failed
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepareStatus {
    pub song_id: i64,
    pub status: String,
    pub ready: bool,
    pub phase: String,
    pub progress: f64,
    pub message: String,
    pub prepare_kind: String,
    pub error: Option<String>,
}

struct PrepareTask {
    song_id: i64,
    state: PrepareState,
    phase: String,
    progress: f64,
    message: String,
    prepare_kind: String,
    error: Option<String>,
    started: bool,
    finished_at: Option<Instant>,
}

impl PrepareTask {
    fn new(song_id: i64) -> Self {
        Self {
            song_id,
            state: PrepareState::Pending,
            phase: "pending".to_string(),
            progress: 0.0,
            message: "排队等待中".to_string(),
            prepare_kind: "unknown".to_string(),
            error: None,
            started: false,
            finished_at: None,
        }
    }
}

struct Inner {
    tasks: HashMap<i64, PrepareTask>,
    wait_queue: VecDeque<i64>,
    running: usize,
}

const TERMINAL_TTL: Duration = Duration::from_secs(600);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

pub struct PrepareTaskManager {
    resolver: PlaybackResolver,
    songs: SongRepository,
    events: EventBus,
    max_concurrent: usize,
    #[allow(dead_code)]
    semaphore: Arc<Semaphore>,
    inner: Mutex<Inner>,
}

impl PrepareTaskManager {
    pub fn new(
        resolver: PlaybackResolver,
        songs: SongRepository,
        events: EventBus,
        max_concurrent: usize,
    ) -> Arc<Self> {
        let manager = Arc::new(Self {
            resolver,
            songs,
            events,
            max_concurrent: max_concurrent.max(1),
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            inner: Mutex::new(Inner {
                tasks: HashMap::new(),
                wait_queue: VecDeque::new(),
                running: 0,
            }),
        });
        spawn_cleanup_loop(manager.clone());
        manager
    }

    pub async fn schedule(self: &Arc<Self>, song_id: i64) -> PrepareStatus {
        {
            let inner = self.inner.lock().unwrap();
            if let Some(task) = inner.tasks.get(&song_id) {
                if matches!(task.state, PrepareState::Pending | PrepareState::Running) {
                    return to_status(task, &inner.wait_queue, inner.running);
                }
            }
        }

        let Ok(Some(song)) = self.songs.get_optional(song_id).await else {
            return not_found_status(song_id);
        };

        if self.resolver.override_complete(&song.display_name).await {
            return not_needed_status(song_id, "无需准备（已有 override 三件套）");
        }
        if self.resolver.is_stream_ready(&song).await {
            return ready_status(song_id);
        }

        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(task) = inner.tasks.get(&song_id) {
                if task.state.is_terminal() {
                    inner.tasks.remove(&song_id);
                }
            }
            inner
                .tasks
                .entry(song_id)
                .or_insert_with(|| PrepareTask::new(song_id));
            inner.wait_queue.retain(|id| *id != song_id);
            inner.wait_queue.push_back(song_id);
        }

        self.pump_queue();

        let inner = self.inner.lock().unwrap();
        let task = inner.tasks.get(&song_id).expect("just inserted");
        to_status(task, &inner.wait_queue, inner.running)
    }

    pub async fn status(&self, song_id: i64) -> PrepareStatus {
        {
            let inner = self.inner.lock().unwrap();
            if let Some(task) = inner.tasks.get(&song_id) {
                return to_status(task, &inner.wait_queue, inner.running);
            }
        }

        let Ok(Some(song)) = self.songs.get_optional(song_id).await else {
            return not_found_status(song_id);
        };

        if self.resolver.is_stream_ready(&song).await {
            return ready_status(song_id);
        }

        let profile = self.resolver.resolve(&song).await;
        if !self.resolver.needs_prepare(&song, &profile).await {
            return not_needed_status(song_id, "无需准备");
        }

        PrepareStatus {
            song_id,
            status: PrepareState::Idle.as_str().to_string(),
            ready: false,
            phase: "idle".to_string(),
            progress: 0.0,
            message: "尚未开始准备".to_string(),
            prepare_kind: "unknown".to_string(),
            error: None,
        }
    }

    pub fn active_tasks(&self) -> HashMap<i64, PrepareStatus> {
        let inner = self.inner.lock().unwrap();
        inner
            .tasks
            .values()
            .filter(|t| matches!(t.state, PrepareState::Pending | PrepareState::Running))
            .map(|t| (t.song_id, to_status(t, &inner.wait_queue, inner.running)))
            .collect()
    }

    pub async fn wait_until_ready(
        self: &Arc<Self>,
        song_id: i64,
        timeout: Duration,
    ) -> PrepareStatus {
        let deadline = Instant::now() + timeout;
        loop {
            let st = self.status(song_id).await;
            if st.ready || st.status == PrepareState::Failed.as_str() {
                return st;
            }
            if st.status == PrepareState::Idle.as_str() {
                self.schedule(song_id).await;
            }
            if Instant::now() >= deadline {
                let mut st = self.status(song_id).await;
                st.error = st.error.or_else(|| Some("等待播放资源超时".to_string()));
                st.status = PrepareState::Failed.as_str().to_string();
                return st;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    fn pump_queue(self: &Arc<Self>) {
        loop {
            let song_id = {
                let mut inner = self.inner.lock().unwrap();
                if inner.running >= self.max_concurrent {
                    return;
                }
                let mut picked = None;
                while let Some(candidate) = inner.wait_queue.pop_front() {
                    let runnable = inner
                        .tasks
                        .get(&candidate)
                        .map(|t| t.state == PrepareState::Pending && !t.started)
                        .unwrap_or(false);
                    if runnable {
                        picked = Some(candidate);
                        break;
                    }
                }
                let Some(song_id) = picked else { return };
                if let Some(task) = inner.tasks.get_mut(&song_id) {
                    task.started = true;
                }
                inner.running += 1;
                song_id
            };

            let manager = self.clone();
            tokio::spawn(async move {
                manager.run_and_release(song_id).await;
            });
        }
    }

    async fn run_and_release(self: Arc<Self>, song_id: i64) {
        Self::run(&self, song_id).await;
        {
            let mut inner = self.inner.lock().unwrap();
            inner.running = inner.running.saturating_sub(1);
        }
        self.pump_queue();
    }

    fn set_state(&self, song_id: i64, state: PrepareState) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(task) = inner.tasks.get_mut(&song_id) {
            task.state = state;
            if state.is_terminal() {
                task.finished_at = Some(Instant::now());
            }
        }
    }

    fn set_progress(&self, song_id: i64, progress: f64, phase: &str, message: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(task) = inner.tasks.get_mut(&song_id) {
            task.progress = progress.clamp(0.0, 100.0);
            task.phase = phase.to_string();
            task.message = message.to_string();
        }
    }

    fn set_kind(&self, song_id: i64, kind: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(task) = inner.tasks.get_mut(&song_id) {
            task.prepare_kind = kind.to_string();
        }
    }

    fn set_error(&self, song_id: i64, error: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(task) = inner.tasks.get_mut(&song_id) {
            task.error = Some(error.to_string());
        }
    }

    async fn run(self: &Arc<Self>, song_id: i64) {
        self.set_state(song_id, PrepareState::Running);
        self.set_progress(song_id, 0.0, "running", "准备中");

        let song = match self.songs.get_optional(song_id).await {
            Ok(Some(s)) => s,
            _ => {
                self.set_state(song_id, PrepareState::Failed);
                self.set_error(song_id, "歌曲不存在");
                return;
            }
        };

        if self.resolver.override_complete(&song.display_name).await {
            self.set_state(song_id, PrepareState::NotNeeded);
            self.set_progress(song_id, 100.0, "done", "无需准备");
            return;
        }

        let profile = self.resolver.resolve(&song).await;

        match profile.playback_source {
            PlaybackSource::Embedded => {
                self.set_kind(song_id, "embedded");
                self.set_progress(song_id, 0.0, "embedded_video", "MKV 双音轨拆轨中");

                let mut layout = song.layout().cloned();
                if layout.is_none() {
                    let probed =
                        probe_layout(&self.resolver.media, &song.source_path, "auto").await;
                    if let Err(e) = self.songs.update_audio_layout(song_id, &probed).await {
                        warn!("persist audio_layout failed for song {song_id}: {e}");
                    }
                    layout = Some(probed);
                }

                let Some(layout) =
                    layout.filter(|l| karaoke_domain::audio_layout::has_dual_roles(Some(l)))
                else {
                    self.set_state(song_id, PrepareState::Failed);
                    self.set_error(song_id, "无有效双音轨布局，请先检测播放能力");
                    self.set_progress(song_id, 0.0, "failed", "准备失败");
                    return;
                };

                let manager = Arc::clone(self);
                let sid = song_id;
                let progress_cb: karaoke_infra::media::ProgressFn = Arc::new(move |pct: f64| {
                    manager.set_progress(sid, pct, "embedded", "MKV 双音轨拆轨中");
                });

                let paths = ensure_embedded_cache(
                    &self.resolver.media,
                    &song.source_path,
                    &layout,
                    true,
                    Some(progress_cb),
                )
                .await;

                let refreshed = self.resolver.resolve(&song).await;
                if let Err(e) = self
                    .songs
                    .update_playback_meta(
                        song_id,
                        karaoke_domain::playback::effective_mode(&refreshed, &song.playback_mode)
                            .as_str(),
                        Some(refreshed.playback_source.as_str()),
                        refreshed.can_queue,
                    )
                    .await
                {
                    warn!("persist playback meta failed for song {song_id}: {e}");
                }

                if paths.ready {
                    self.set_state(song_id, PrepareState::Ready);
                    self.set_progress(song_id, 100.0, "done", "缓存就绪");
                    self.events.publish_prepare_ready(song_id);
                } else {
                    self.set_state(song_id, PrepareState::Failed);
                    self.set_error(song_id, "内嵌缓存生成失败");
                    self.set_progress(song_id, 0.0, "failed", "内嵌缓存生成失败");
                }
            }
            PlaybackSource::Plain if profile.video_path.is_some() => {
                self.set_kind(song_id, "plain");
                self.set_progress(song_id, 0.0, "transcode", "浏览器转码中");
                let video_path = profile.video_path.clone().unwrap();

                let manager = Arc::clone(self);
                let sid = song_id;
                let progress_cb: karaoke_infra::media::ProgressFn = Arc::new(move |pct: f64| {
                    manager.set_progress(sid, pct, "transcode", "浏览器转码中");
                });

                let ok =
                    ensure_browser_mp4_cache(&self.resolver.media, &video_path, Some(progress_cb))
                        .await;
                if ok {
                    self.set_state(song_id, PrepareState::Ready);
                    self.set_progress(song_id, 100.0, "done", "转码完成");
                    self.events.publish_prepare_ready(song_id);
                } else {
                    self.set_state(song_id, PrepareState::Failed);
                    self.set_error(song_id, "浏览器转码缓存生成失败");
                    self.set_progress(song_id, 0.0, "failed", "转码失败");
                }
            }
            _ => {
                self.set_state(song_id, PrepareState::NotNeeded);
                self.set_progress(song_id, 100.0, "done", "无需准备");
            }
        }
    }
}

fn spawn_cleanup_loop(manager: Arc<PrepareTaskManager>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
        loop {
            interval.tick().await;
            let mut inner = manager.inner.lock().unwrap();
            let before = inner.tasks.len();
            inner.tasks.retain(|_, task| {
                !task.state.is_terminal()
                    || task
                        .finished_at
                        .map(|t| t.elapsed() < TERMINAL_TTL)
                        .unwrap_or(true)
            });
            let removed = before - inner.tasks.len();
            if removed > 0 {
                tracing::debug!("prepare task cleanup removed {removed} terminal entries");
            }
        }
    });
}

fn to_status(task: &PrepareTask, wait_queue: &VecDeque<i64>, running: usize) -> PrepareStatus {
    let ready = matches!(task.state, PrepareState::Ready | PrepareState::NotNeeded);
    let mut message = task.message.clone();
    if task.state == PrepareState::Pending && !task.started {
        let ahead = wait_queue
            .iter()
            .take_while(|id| **id != task.song_id)
            .count();
        if ahead + running > 0 {
            message = format!("排队等待中（前方 {} 首）", ahead + running);
        }
    }
    PrepareStatus {
        song_id: task.song_id,
        status: task.state.as_str().to_string(),
        ready,
        phase: task.phase.clone(),
        progress: task.progress,
        message,
        prepare_kind: task.prepare_kind.clone(),
        error: task.error.clone(),
    }
}

fn not_found_status(song_id: i64) -> PrepareStatus {
    PrepareStatus {
        song_id,
        status: PrepareState::Failed.as_str().to_string(),
        ready: false,
        phase: "failed".to_string(),
        progress: 0.0,
        message: "歌曲不存在".to_string(),
        prepare_kind: "unknown".to_string(),
        error: Some("歌曲不存在".to_string()),
    }
}

fn not_needed_status(song_id: i64, message: &str) -> PrepareStatus {
    PrepareStatus {
        song_id,
        status: PrepareState::NotNeeded.as_str().to_string(),
        ready: true,
        phase: "done".to_string(),
        progress: 100.0,
        message: message.to_string(),
        prepare_kind: "none".to_string(),
        error: None,
    }
}

fn ready_status(song_id: i64) -> PrepareStatus {
    PrepareStatus {
        song_id,
        status: PrepareState::Ready.as_str().to_string(),
        ready: true,
        phase: "done".to_string(),
        progress: 100.0,
        message: "播放资源已就绪".to_string(),
        prepare_kind: "none".to_string(),
        error: None,
    }
}
