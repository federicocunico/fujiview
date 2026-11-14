//! Camera worker thread and the Tauri commands built on it.
//!
//! One thread owns COM and the camera connection; commands talk to it over
//! a channel. While a job runs, status and presets are served from cache.

use super::diagnostics::{
    open_camera, run_diagnostics, Cam, CameraDiagnostics, CameraLog, CheckStatus, DiagHost,
    LogLine, SimFault, EVENT_DIAGNOSTICS, EVENT_LOG, STEP_TIMEOUT,
};
use super::plan::{build_plan, PresetDef, PresetKind};
use super::runner::{run_plan, RunOptions};
use super::types::*;
use crate::camera::banks::{apply_bank, film_simulation_name, folder_name};
use crate::camera::profile::Profile;
use crate::camera::ptp::{DeviceInfo, PtpError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

pub const EVENT_STATUS: &str = "camera-status";
pub const EVENT_PROGRESS: &str = "variant-progress";
pub const EVENT_FINISHED: &str = "variant-finished";

type Reply<T> = Sender<Result<T, String>>;

enum Request {
    Status(Sender<CameraStatus>),
    Presets(Reply<Vec<PresetDef>>),
    Run(VariantRequest),
    Diagnostics(Sender<CameraDiagnostics>),
}

/// What the worker is doing right now (name, since when).
type Activity = Arc<Mutex<Option<(&'static str, Instant)>>>;
/// Latest checklist snapshot of a running diagnostic and when it was written.
type DiagProgress = Arc<Mutex<Option<(CameraDiagnostics, Instant)>>>;

const STATUS_WAIT: Duration = Duration::from_secs(5);
const PRESETS_WAIT: Duration = Duration::from_secs(45);
/// Operations that legitimately take a while: not reported as a frozen camera.
const SLOW_OPS: [&str; 2] = ["diagnostica", "preset"];

pub struct VariantService {
    tx: Mutex<Sender<Request>>,
    status: Arc<Mutex<CameraStatus>>,
    presets: Arc<Mutex<Vec<PresetDef>>>,
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    last_request: Mutex<Option<VariantRequest>>,
    app: AppHandle,
    log: CameraLog,
    activity: Activity,
    diag_progress: DiagProgress,
    last_diag: Mutex<Option<CameraDiagnostics>>,
}

pub fn simulated_camera_enabled() -> bool {
    std::env::var("FUJIVIEW_FAKE_CAMERA").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

struct Connected {
    camera: Cam,
    info: DeviceInfo,
    simulated: bool,
}

fn status_of(connected: &Connected) -> CameraStatus {
    let missing = connected.info.missing_conversion_support();
    let (state, message) = if missing.is_empty() {
        (CameraState::Ready, None)
    } else {
        (
            CameraState::NotReady,
            Some("La camera non è in modalità conversione RAW: imposta USB RAW CONV./BACKUP RESTORE nel menu della camera.".to_string()),
        )
    };
    CameraStatus {
        state,
        model: Some(connected.info.model.clone()),
        firmware: Some(connected.info.device_version.clone()),
        message,
        simulated: connected.simulated,
    }
}

fn status_from_error(error: &PtpError) -> CameraStatus {
    match error {
        PtpError::Disconnected => CameraStatus::new(CameraState::Disconnected, None),
        PtpError::Busy => CameraStatus::new(
            CameraState::Busy,
            Some("Camera occupata: chiudi X RAW Studio.".to_string()),
        ),
        other => CameraStatus::new(CameraState::Error, Some(other.to_string())),
    }
}

fn connect() -> Result<Option<Connected>, PtpError> {
    let simulated = simulated_camera_enabled();
    let Some(mut camera) = open_camera(simulated, SimFault::from_env())? else {
        return Ok(None);
    };
    let info = camera.device_info()?;
    Ok(Some(Connected {
        camera,
        info,
        simulated,
    }))
}

fn preset_view(preset: &PresetDef) -> CameraPreset {
    match &preset.kind {
        PresetKind::AsShot => CameraPreset {
            id: preset.id.clone(),
            bank: None,
            name: "Come scattato".into(),
            folder_name: preset.base_folder_name(),
            film_simulation: None,
            summary: "Impostazioni registrate nel RAF".into(),
            unapplied_properties: Vec::new(),
        },
        PresetKind::Bank(bank) => {
            // Same rules the conversion uses, against a neutral base profile.
            let neutral = Profile::synthetic("", &[0; 29]);
            let unapplied = apply_bank(&neutral, bank)
                .map(|a| a.unapplied)
                .unwrap_or_default();
            let unapplied = unapplied
                .into_iter()
                .filter(|u| !u.starts_with("gamma dinamica limitata"))
                .collect();
            CameraPreset {
                id: preset.id.clone(),
                bank: Some(bank.index),
                name: if bank.name.is_empty() {
                    format!("C{}", bank.index)
                } else {
                    bank.name.clone()
                },
                folder_name: folder_name(bank.index, &bank.name),
                film_simulation: bank
                    .film_simulation()
                    .and_then(film_simulation_name)
                    .map(str::to_string),
                summary: bank.summary(),
                unapplied_properties: unapplied,
            }
        }
    }
}

struct Worker {
    app: AppHandle,
    status: Arc<Mutex<CameraStatus>>,
    presets: Arc<Mutex<Vec<PresetDef>>>,
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    connected: Option<Connected>,
    log: CameraLog,
    activity: Activity,
    diag_progress: DiagProgress,
}

fn status_after_failed_diagnostics(result: &CameraDiagnostics) -> CameraStatus {
    if result.unresponsive {
        return unresponsive_status(None);
    }
    if result
        .check("device")
        .is_some_and(|c| c.status == CheckStatus::Error)
    {
        return CameraStatus::new(CameraState::Disconnected, None);
    }
    let message = result
        .checks
        .iter()
        .find(|c| c.status == CheckStatus::Error)
        .and_then(|c| c.detail.clone());
    let mut status = CameraStatus::new(CameraState::NotReady, message);
    status.model = result.model.clone();
    status.firmware = result.firmware.clone();
    status.simulated = result.simulated;
    status
}

fn unresponsive_status(previous: Option<CameraStatus>) -> CameraStatus {
    let mut status = CameraStatus::new(
        CameraState::Unresponsive,
        Some("In attesa della camera… Se non risponde, spegni e riaccendi la camera.".to_string()),
    );
    if let Some(previous) = previous {
        status.model = previous.model;
        status.firmware = previous.firmware;
        status.simulated = previous.simulated;
    }
    status
}

impl Worker {
    fn publish(&self, status: CameraStatus) -> CameraStatus {
        let changed = *self.status.lock().unwrap() != status;
        *self.status.lock().unwrap() = status.clone();
        if changed {
            let detail = status.message.as_deref().unwrap_or("");
            let line = format!("Stato camera: {:?} {detail}", status.state);
            match status.state {
                CameraState::Error | CameraState::Unresponsive => self.log.error(line),
                CameraState::Ready => self.log.info(format!(
                    "Camera pronta: {} fw {}",
                    status.model.as_deref().unwrap_or("?"),
                    status.firmware.as_deref().unwrap_or("?")
                )),
                _ => self.log.warn(line),
            }
            let _ = self.app.emit(EVENT_STATUS, &status);
        }
        status
    }

    /// Make sure a live connection exists; returns the current status.
    fn refresh(&mut self) -> CameraStatus {
        if let Some(connected) = self.connected.as_mut() {
            match connected.camera.device_info() {
                Ok(info) => connected.info = info,
                Err(error) => {
                    self.log.warn(format!("Connessione persa: {error}"));
                    self.connected = None;
                }
            }
        }
        if self.connected.is_none() {
            match connect() {
                Ok(Some(connected)) => self.connected = Some(connected),
                Ok(None) => {
                    return self.publish(CameraStatus::new(CameraState::Disconnected, None))
                }
                Err(error) => return self.publish(status_from_error(&error)),
            }
        }
        let status = status_of(self.connected.as_ref().unwrap());
        self.publish(status)
    }

    fn read_presets(&mut self) -> Result<Vec<PresetDef>, String> {
        let status = self.refresh();
        if status.state != CameraState::Ready {
            return Err(status
                .message
                .unwrap_or_else(|| "Nessuna camera pronta per la conversione.".into()));
        }
        let connected = self.connected.as_mut().unwrap();
        let banks = connected.camera.read_banks().map_err(|e| {
            self.connected = None;
            format!("Lettura dei preset non riuscita: {e}")
        })?;
        let mut presets = vec![PresetDef::as_shot()];
        presets.extend(banks.into_iter().map(PresetDef::bank));
        *self.presets.lock().unwrap() = presets.clone();
        Ok(presets)
    }

    fn run(&mut self, request: VariantRequest) {
        let report = self
            .run_inner(&request)
            .unwrap_or_else(|reason| VariantReport {
                state: VariantJobState::Failed,
                output_dir: request.output_dir.clone().unwrap_or_default(),
                converted: 0,
                skipped: 0,
                failures: Vec::new(),
                paused_reason: Some(reason),
            });
        self.running.store(false, Ordering::SeqCst);
        let summary = format!(
            "Generazione {:?}: {} convertite, {} saltate, {} fallite",
            report.state,
            report.converted,
            report.skipped,
            report.failures.len()
        );
        match &report.paused_reason {
            Some(reason) => self.log.warn(format!("{summary}. Motivo: {reason}")),
            None => self.log.info(summary),
        }
        if matches!(
            report.state,
            VariantJobState::Paused | VariantJobState::Failed
        ) {
            // Force a reconnect before the next attempt.
            self.connected = None;
            self.refresh();
        }
        let _ = self.app.emit(EVENT_FINISHED, &report);
    }

    fn run_inner(&mut self, request: &VariantRequest) -> Result<VariantReport, String> {
        // Banks are read once at job start: this snapshot is what gets applied.
        let presets = self.read_presets()?;
        let plan = build_plan(request, &presets)?;
        let counts = plan.summary();
        self.log.info(format!(
            "Generazione avviata: {} conversioni da fare su {} RAF",
            counts.pending_conversions, counts.raf_count
        ));
        let connected = self.connected.as_mut().ok_or("Camera non collegata.")?;
        let app = self.app.clone();
        let cancel = self.cancel.clone();
        let log = self.log.clone();
        let mut last: Option<(VariantJobState, Option<String>)> = None;
        let mut emit = |progress: VariantProgress| {
            let key = (progress.state, progress.message.clone());
            if last.as_ref() != Some(&key) {
                match &progress.message {
                    Some(message) => {
                        log.warn(format!("Generazione {:?}: {message}", progress.state))
                    }
                    None => log.info(format!("Generazione: {:?}", progress.state)),
                }
                last = Some(key);
            }
            let _ = app.emit(EVENT_PROGRESS, &progress);
        };
        Ok(run_plan(
            &mut connected.camera,
            &plan,
            RunOptions::default(),
            &cancel,
            &mut emit,
        ))
    }

    fn diagnose(&mut self) -> CameraDiagnostics {
        self.log.info("Diagnostica camera avviata");
        let existing = self.connected.take();
        let simulated = existing
            .as_ref()
            .map(|c| c.simulated)
            .unwrap_or_else(simulated_camera_enabled);
        let mut host = DiagHost::new(existing.map(|c| c.camera), simulated, SimFault::from_env());
        let progress = self.diag_progress.clone();
        let app = self.app.clone();
        *progress.lock().unwrap() = Some((CameraDiagnostics::new(simulated), Instant::now()));
        let result = run_diagnostics(&mut host, &mut |snapshot| {
            *progress.lock().unwrap() = Some((snapshot.clone(), Instant::now()));
            let _ = app.emit(EVENT_DIAGNOSTICS, snapshot);
        });
        *progress.lock().unwrap() = None;
        for check in &result.checks {
            let detail = check.detail.as_deref().unwrap_or("");
            match check.status {
                CheckStatus::Error => self.log.error(format!("{}: {detail}", check.label)),
                CheckStatus::Warning => self.log.warn(format!("{}: {detail}", check.label)),
                _ => {}
            }
        }
        self.log.info(if result.ready {
            "Diagnostica completata: tutto in ordine"
        } else {
            "Diagnostica completata: problemi rilevati"
        });
        self.connected = host.into_connected().map(|(camera, info)| Connected {
            camera,
            info,
            simulated,
        });
        let status = match &self.connected {
            Some(connected) => status_of(connected),
            None => status_after_failed_diagnostics(&result),
        };
        self.publish(status);
        result
    }

    fn serve(mut self, rx: Receiver<Request>) {
        #[cfg(windows)]
        let _com = crate::camera::wpd::ComGuard::init();
        for request in rx {
            let what = match &request {
                Request::Status(_) => "stato",
                Request::Presets(_) => "preset",
                Request::Run(_) => "generazione",
                Request::Diagnostics(_) => "diagnostica",
            };
            *self.activity.lock().unwrap() = Some((what, Instant::now()));
            match request {
                Request::Status(reply) => {
                    let _ = reply.send(self.refresh());
                }
                Request::Presets(reply) => {
                    let _ = reply.send(self.read_presets());
                }
                Request::Run(request) => self.run(request),
                Request::Diagnostics(reply) => {
                    let _ = reply.send(self.diagnose());
                }
            }
            *self.activity.lock().unwrap() = None;
        }
    }
}

impl VariantService {
    pub fn start(app: AppHandle) -> Self {
        let (tx, rx) = channel();
        let status = Arc::new(Mutex::new(CameraStatus::new(
            CameraState::Disconnected,
            None,
        )));
        let presets = Arc::new(Mutex::new(Vec::new()));
        let cancel = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(false));
        let log = CameraLog::new(Some({
            let app = app.clone();
            Arc::new(move |line: &LogLine| {
                let _ = app.emit(EVENT_LOG, line);
            })
        }));
        let activity: Activity = Arc::new(Mutex::new(None));
        let diag_progress: DiagProgress = Arc::new(Mutex::new(None));
        let (s, p, c, r) = (
            status.clone(),
            presets.clone(),
            cancel.clone(),
            running.clone(),
        );
        let (worker_app, worker_log, worker_activity, worker_progress) = (
            app.clone(),
            log.clone(),
            activity.clone(),
            diag_progress.clone(),
        );
        // The connection is created on the worker thread and never leaves it.
        std::thread::Builder::new()
            .name("fujiview-camera".into())
            .spawn(move || {
                Worker {
                    app: worker_app,
                    status: s,
                    presets: p,
                    cancel: c,
                    running: r,
                    connected: None,
                    log: worker_log,
                    activity: worker_activity,
                    diag_progress: worker_progress,
                }
                .serve(rx)
            })
            .expect("camera worker thread");
        Self {
            tx: Mutex::new(tx),
            status,
            presets,
            cancel,
            running,
            last_request: Mutex::new(None),
            app,
            log,
            activity,
            diag_progress,
            last_diag: Mutex::new(None),
        }
    }

    /// Send a request to the worker and wait at most `wait` for the answer.
    fn ask<T: Send + 'static>(
        &self,
        wait: Duration,
        make: impl FnOnce(Sender<T>) -> Request,
    ) -> Result<T, String> {
        let (reply, rx) = channel();
        self.tx
            .lock()
            .unwrap()
            .send(make(reply))
            .map_err(|_| "Servizio camera non disponibile.".to_string())?;
        rx.recv_timeout(wait).map_err(|error| match error {
            RecvTimeoutError::Timeout => {
                "La camera non risponde. Spegni e riaccendi la camera, poi riprova.".to_string()
            }
            RecvTimeoutError::Disconnected => "Servizio camera non disponibile.".to_string(),
        })
    }

    /// How long the worker has been inside the current operation, if any.
    fn busy_for(&self) -> Option<(&'static str, Duration)> {
        self.activity
            .lock()
            .unwrap()
            .map(|(what, since)| (what, since.elapsed()))
    }

    fn cached_status(&self) -> CameraStatus {
        self.status.lock().unwrap().clone()
    }

    /// Cheap status check that never blocks on a hung camera call.
    fn status(&self) -> CameraStatus {
        if self.is_running() {
            return self.cached_status();
        }
        if let Some((what, busy)) = self.busy_for() {
            if busy > Duration::from_secs(3) {
                return if SLOW_OPS.contains(&what) && busy < PRESETS_WAIT {
                    self.cached_status()
                } else {
                    unresponsive_status(Some(self.cached_status()))
                };
            }
        }
        match self.ask(STATUS_WAIT, Request::Status) {
            Ok(status) => status,
            Err(_) => {
                self.log
                    .error("La camera non risponde alla richiesta di stato");
                unresponsive_status(Some(self.cached_status()))
            }
        }
    }

    fn finish_diagnostics(&self, result: CameraDiagnostics) -> CameraDiagnostics {
        let _ = self.app.emit(EVENT_DIAGNOSTICS, &result);
        *self.last_diag.lock().unwrap() = Some(result.clone());
        result
    }

    /// Full checklist. Never blocks longer than `STEP_TIMEOUT` past the last
    /// completed step, even when the camera hangs the worker thread.
    fn diagnostics(&self) -> Result<CameraDiagnostics, String> {
        if self.is_running() {
            return self.last_diag.lock().unwrap().clone().ok_or_else(|| {
                "Generazione in corso: la diagnostica completa non è disponibile.".to_string()
            });
        }
        let simulated = simulated_camera_enabled();
        if let Some((what, busy)) = self.busy_for() {
            if busy > STEP_TIMEOUT {
                let snapshot = self.diag_progress.lock().unwrap().clone();
                let result = match snapshot {
                    Some((partial, at)) if what == "diagnostica" => {
                        partial.into_unresponsive(at.elapsed())
                    }
                    _ => CameraDiagnostics::stuck(simulated, what, busy),
                };
                return Ok(self.finish_diagnostics(result));
            }
        }
        *self.diag_progress.lock().unwrap() = None;
        let (reply, rx) = channel();
        self.tx
            .lock()
            .unwrap()
            .send(Request::Diagnostics(reply))
            .map_err(|_| "Servizio camera non disponibile.".to_string())?;
        let started = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(result) => return Ok(self.finish_diagnostics(result)),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("Servizio camera non disponibile.".into())
                }
                Err(RecvTimeoutError::Timeout) => {
                    let snapshot = self.diag_progress.lock().unwrap().clone();
                    let (waited, partial) = match snapshot {
                        Some((partial, at)) => (at.elapsed(), Some(partial)),
                        None => (started.elapsed(), None),
                    };
                    if waited > STEP_TIMEOUT {
                        self.log.error(format!(
                            "La camera non risponde da {} s: diagnostica interrotta",
                            waited.as_secs()
                        ));
                        let result = match partial {
                            Some(partial) => partial.into_unresponsive(waited),
                            None => CameraDiagnostics::stuck(simulated, "in coda", waited),
                        };
                        return Ok(self.finish_diagnostics(result));
                    }
                }
            }
        }
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn start_job(&self, request: VariantRequest) -> Result<(), String> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err("Una generazione è già in corso.".into());
        }
        self.cancel.store(false, Ordering::SeqCst);
        *self.last_request.lock().unwrap() = Some(request.clone());
        self.tx
            .lock()
            .unwrap()
            .send(Request::Run(request))
            .map_err(|_| {
                self.running.store(false, Ordering::SeqCst);
                "Servizio camera non disponibile.".to_string()
            })
    }
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn camera_status(
    service: State<'_, Arc<VariantService>>,
) -> Result<CameraStatus, String> {
    let service = service.inner().clone();
    blocking(move || Ok(service.status())).await
}

/// Last known status without touching the camera.
#[tauri::command]
pub fn camera_status_cached(service: State<'_, Arc<VariantService>>) -> CameraStatus {
    service.cached_status()
}

#[tauri::command]
pub async fn camera_diagnostics(
    service: State<'_, Arc<VariantService>>,
) -> Result<CameraDiagnostics, String> {
    let service = service.inner().clone();
    blocking(move || service.diagnostics()).await
}

#[tauri::command]
pub fn camera_log(service: State<'_, Arc<VariantService>>) -> Vec<LogLine> {
    service.log.snapshot()
}

#[tauri::command]
pub async fn camera_presets(
    service: State<'_, Arc<VariantService>>,
) -> Result<Vec<CameraPreset>, String> {
    let service = service.inner().clone();
    blocking(move || {
        let presets = if service.is_running() {
            service.presets.lock().unwrap().clone()
        } else {
            service.ask(PRESETS_WAIT, Request::Presets)??
        };
        Ok(presets.iter().map(preset_view).collect())
    })
    .await
}

#[tauri::command]
pub async fn plan_variants(
    service: State<'_, Arc<VariantService>>,
    request: VariantRequest,
) -> Result<VariantPlan, String> {
    let service = service.inner().clone();
    blocking(move || {
        let presets = service.presets.lock().unwrap().clone();
        let presets = if presets.is_empty() {
            vec![PresetDef::as_shot()]
        } else {
            presets
        };
        Ok(build_plan(&request, &presets)?.summary())
    })
    .await
}

#[tauri::command]
pub async fn start_variants(
    service: State<'_, Arc<VariantService>>,
    request: VariantRequest,
) -> Result<(), String> {
    let service = service.inner().clone();
    blocking(move || {
        let presets = service.presets.lock().unwrap().clone();
        if !presets.is_empty() {
            let summary = build_plan(&request, &presets)?.summary();
            if !summary.enough_space {
                return Err(format!(
                    "Spazio insufficiente: servono {:.1} GB più 5 GB di margine, liberi {:.1} GB.",
                    summary.estimated_bytes as f64 / 1e9,
                    summary.free_bytes as f64 / 1e9
                ));
            }
        }
        service.start_job(request)
    })
    .await
}

#[tauri::command]
pub fn cancel_variants(service: State<'_, Arc<VariantService>>) {
    service.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn resume_variants(service: State<'_, Arc<VariantService>>) -> Result<(), String> {
    let service = service.inner().clone();
    blocking(move || {
        let request = service
            .last_request
            .lock()
            .unwrap()
            .clone()
            .ok_or("Nessuna generazione da riprendere.")?;
        service.start_job(request)
    })
    .await
}
