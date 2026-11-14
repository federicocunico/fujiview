//! Executes a [`Plan`] against a camera.
//!
//! Safety rules: RAFs are only ever read; nothing is written or deleted
//! outside the plan's preset folders; every JPEG lands via `.part` + rename.

use super::estimate::{self, Meter};
use super::plan::{Conversion, Plan, PresetKind, MANIFEST, PART_SUFFIX};
use super::types::{VariantFailure, VariantJobState, VariantProgress, VariantReport};
use crate::camera::banks::{apply_bank, as_shot};
use crate::camera::fuji::{is_raf_rejected, FujiCamera};
use crate::camera::profile::Profile;
use crate::camera::ptp::{rc, PtpError, PtpTransport};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Upload the RAF again before every conversion (fallback when the
    /// camera accepts only one conversion per upload). Switched on
    /// automatically when that behaviour is detected.
    pub upload_per_conversion: bool,
    pub busy_retries: usize,
    pub busy_backoff: Duration,
    pub disk_check_every: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            upload_per_conversion: false,
            busy_retries: 3,
            busy_backoff: Duration::from_millis(750),
            disk_check_every: estimate::DISK_CHECK_EVERY,
        }
    }
}

/// Canonical output root plus checks that every write stays inside it.
pub struct OutputGuard {
    root: PathBuf,
}

impl OutputGuard {
    pub fn new(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root)
            .map_err(|e| format!("Impossibile creare {}: {e}", root.display()))?;
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        Ok(Self { root })
    }

    /// Fails unless `path`'s parent directory resolves inside the root.
    pub fn check(&self, path: &Path) -> Result<(), String> {
        let name_ok = path.file_name().is_some_and(|n| n != ".." && n != ".");
        let parent = path.parent().ok_or("percorso senza cartella")?;
        let parent = parent
            .canonicalize()
            .map_err(|e| format!("{}: {e}", parent.display()))?;
        if name_ok && parent.starts_with(&self.root) && parent != self.root {
            Ok(())
        } else {
            Err(format!(
                "Scrittura rifiutata fuori dalla cartella di output: {}",
                path.display()
            ))
        }
    }
}

fn prepare_targets(plan: &Plan, guard: &OutputGuard) -> Result<(), String> {
    for target in &plan.targets {
        fs::create_dir_all(&target.dir)
            .map_err(|e| format!("Impossibile creare {}: {e}", target.dir.display()))?;
        let manifest = target.dir.join(MANIFEST);
        guard.check(&manifest)?;
        if !manifest.exists() {
            let text = serde_json::to_string_pretty(&target.preset.manifest())
                .map_err(|e| e.to_string())?;
            fs::write(&manifest, text).map_err(|e| e.to_string())?;
        }
        // Remove partial files left by an interrupted run (only `*.part`,
        // only directly inside our preset folders).
        for entry in fs::read_dir(&target.dir)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let path = entry.path();
            if path.is_file()
                && path.to_string_lossy().ends_with(PART_SUFFIX)
                && guard.check(&path).is_ok()
            {
                let _ = fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

pub fn write_atomic(guard: &OutputGuard, path: &Path, bytes: &[u8]) -> Result<(), String> {
    guard.check(path)?;
    let part = PathBuf::from(format!("{}{PART_SUFFIX}", path.display()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&part)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&part, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result.map_err(|e| format!("Scrittura di {} non riuscita: {e}", path.display()))
}

fn spawn_read(path: PathBuf) -> JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || fs::read(path))
}

enum Stop {
    Paused(String),
    Failed(String),
}

fn classify(error: &PtpError) -> Option<Stop> {
    match error {
        PtpError::Disconnected => Some(Stop::Paused(
            "Camera disconnessa: ricollegala e premi Riprendi.".into(),
        )),
        PtpError::Transport { .. } => Some(Stop::Paused(format!(
            "Comunicazione con la camera interrotta ({error}). Ricollegala e premi Riprendi."
        ))),
        PtpError::Busy => Some(Stop::Paused(
            "Camera occupata da un altro programma (chiudi X RAW Studio) e premi Riprendi.".into(),
        )),
        PtpError::Timeout(_) => Some(Stop::Failed(
            "La camera non risponde: spegnila e riaccendila, poi premi Riprendi.".into(),
        )),
        PtpError::Response {
            code: rc::DEVICE_BUSY,
            ..
        } => Some(Stop::Failed(
            "La camera resta occupata (0x2019): spegnila e riaccendila, poi premi Riprendi.".into(),
        )),
        _ => None,
    }
}

struct Job<'a, T: PtpTransport> {
    camera: &'a mut FujiCamera<T>,
    plan: &'a Plan,
    guard: OutputGuard,
    options: RunOptions,
    cancel: &'a AtomicBool,
    emit: &'a mut dyn FnMut(VariantProgress),
    meter: Meter,
    converted: usize,
    failures: Vec<VariantFailure>,
    since_disk_check: usize,
}

impl<T: PtpTransport> Job<'_, T> {
    fn name(path: &Path) -> String {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn progress(
        &mut self,
        state: VariantJobState,
        raf: Option<String>,
        preset: Option<String>,
        message: Option<String>,
    ) {
        let remaining = self
            .plan
            .pending
            .len()
            .saturating_sub(self.converted + self.failures.len());
        let state = if state == VariantJobState::Running && self.cancel.load(Ordering::SeqCst) {
            VariantJobState::Cancelling
        } else {
            state
        };
        let event = VariantProgress {
            state,
            total: self.plan.total(),
            completed: self.converted,
            skipped: self.plan.already_done,
            failed: self.failures.len(),
            current_raf: raf,
            current_preset: preset,
            eta_seconds: Some(self.meter.eta_seconds(remaining)),
            message,
        };
        (self.emit)(event);
    }

    fn fail(&mut self, raf: usize, preset: Option<usize>, reason: String) {
        self.failures.push(VariantFailure {
            raf: Self::name(&self.plan.rafs[raf]),
            preset: preset.map(|p| self.plan.targets[p].folder_name.clone()),
            reason,
        });
    }

    fn with_busy_retry<R>(
        &mut self,
        mut action: impl FnMut(&mut FujiCamera<T>) -> Result<R, PtpError>,
    ) -> Result<R, PtpError> {
        let mut attempt = 0;
        loop {
            match action(self.camera) {
                Err(PtpError::Response {
                    code: rc::DEVICE_BUSY,
                    ..
                }) if attempt < self.options.busy_retries => {
                    attempt += 1;
                    std::thread::sleep(self.options.busy_backoff * attempt as u32);
                }
                other => return other,
            }
        }
    }

    fn upload(&mut self, raf: &[u8]) -> Result<Profile, PtpError> {
        self.with_busy_retry(|camera| camera.upload_raf(raf))?;
        self.with_busy_retry(|camera| camera.read_profile())
    }

    fn profile_for(&self, base: &Profile, preset: usize) -> Result<Profile, PtpError> {
        match &self.plan.targets[preset].preset.kind {
            PresetKind::AsShot => as_shot(base),
            PresetKind::Bank(bank) => Ok(apply_bank(base, bank)?.profile),
        }
    }

    fn check_disk(&mut self, remaining: usize) -> Result<(), Stop> {
        let Some(free) = estimate::free_bytes(&self.plan.output_dir) else {
            return Ok(());
        };
        let need = estimate::required_bytes(remaining, self.meter.average_bytes());
        if estimate::has_room(free, need) {
            Ok(())
        } else {
            Err(Stop::Paused(format!(
                "Spazio su disco insufficiente: servono {:.1} GB più 5 GB di margine, liberi {:.1} GB. Libera spazio e premi Riprendi.",
                need as f64 / 1e9,
                free as f64 / 1e9
            )))
        }
    }

    /// Convert every pending preset of one RAF. `Err` stops the whole job.
    fn convert_raf(&mut self, raf: usize, presets: &[usize], bytes: &[u8]) -> Result<(), Stop> {
        let raf_name = Self::name(&self.plan.rafs[raf]);
        self.progress(
            VariantJobState::Running,
            Some(raf_name.clone()),
            None,
            Some("Caricamento del RAF nella camera…".into()),
        );
        let mut base = match self.upload(bytes) {
            Ok(base) => base,
            Err(error) if is_raf_rejected(&error) => {
                for p in presets {
                    self.fail(
                        raf,
                        Some(*p),
                        "RAF non scattato da questa camera o non convertibile".into(),
                    );
                }
                return Ok(());
            }
            Err(error) => {
                return Err(classify(&error).unwrap_or_else(|| Stop::Paused(error.to_string())))
            }
        };
        let mut fresh_upload = true;
        for &preset in presets {
            if self.cancel.load(Ordering::SeqCst) {
                return Ok(());
            }
            let folder = self.plan.targets[preset].folder_name.clone();
            self.progress(
                VariantJobState::Running,
                Some(raf_name.clone()),
                Some(folder.clone()),
                None,
            );
            if self.options.upload_per_conversion && !fresh_upload {
                base = self
                    .upload(bytes)
                    .map_err(|e| classify(&e).unwrap_or_else(|| Stop::Paused(e.to_string())))?;
            }
            let started = Instant::now();
            let mut outcome = self
                .profile_for(&base, preset)
                .and_then(|profile| self.with_busy_retry(|camera| camera.convert(&profile)));
            if let Err(PtpError::Response {
                code: rc::GENERAL_ERROR,
                ..
            }) = outcome
            {
                if !fresh_upload && !self.options.upload_per_conversion {
                    // The camera refuses a second conversion per upload: switch
                    // to one upload per conversion for the rest of the job.
                    self.options.upload_per_conversion = true;
                    outcome = self
                        .upload(bytes)
                        .and_then(|fresh| {
                            base = fresh;
                            self.profile_for(&base, preset)
                        })
                        .and_then(|profile| {
                            self.with_busy_retry(|camera| camera.convert(&profile))
                        });
                }
            }
            fresh_upload = false;
            match outcome {
                Ok(jpeg) => {
                    let path = self.plan.output_path(Conversion { raf, preset });
                    write_atomic(&self.guard, &path, &jpeg).map_err(Stop::Paused)?;
                    self.meter
                        .record(started.elapsed().as_secs_f64(), jpeg.len() as u64);
                    self.converted += 1;
                    self.since_disk_check += 1;
                }
                Err(error) => match classify(&error) {
                    Some(stop) => return Err(stop),
                    None => self.fail(raf, Some(preset), error.to_string()),
                },
            }
            if self.since_disk_check >= self.options.disk_check_every {
                self.since_disk_check = 0;
                let remaining = self
                    .plan
                    .pending
                    .len()
                    .saturating_sub(self.converted + self.failures.len());
                self.check_disk(remaining)?;
            }
        }
        self.progress(VariantJobState::Running, Some(raf_name), None, None);
        Ok(())
    }

    fn run(&mut self) -> Result<(), Stop> {
        self.check_disk(self.plan.pending.len())?;
        self.with_busy_retry(|camera| camera.clear_objects())
            .map_err(|e| classify(&e).unwrap_or_else(|| Stop::Paused(e.to_string())))?;
        // Group pending conversions by RAF, keeping plan order.
        let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
        for c in &self.plan.pending {
            match groups.last_mut() {
                Some((raf, presets)) if *raf == c.raf => presets.push(c.preset),
                _ => groups.push((c.raf, vec![c.preset])),
            }
        }
        let mut next = groups
            .first()
            .map(|(raf, _)| spawn_read(self.plan.rafs[*raf].clone()));
        for index in 0..groups.len() {
            if self.cancel.load(Ordering::SeqCst) {
                return Ok(());
            }
            let (raf, presets) = groups[index].clone();
            let bytes = next.take().map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(std::io::Error::other("lettura interrotta")))
            });
            next = groups
                .get(index + 1)
                .map(|(r, _)| spawn_read(self.plan.rafs[*r].clone()));
            match bytes {
                Some(Ok(bytes)) => self.convert_raf(raf, &presets, &bytes)?,
                Some(Err(error)) => {
                    for p in &presets {
                        self.fail(
                            raf,
                            Some(*p),
                            format!("Impossibile leggere il RAF: {error}"),
                        );
                    }
                }
                None => {}
            }
        }
        Ok(())
    }
}

pub fn run_plan<T: PtpTransport>(
    camera: &mut FujiCamera<T>,
    plan: &Plan,
    options: RunOptions,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(VariantProgress),
) -> VariantReport {
    let output_dir = plan.output_dir.to_string_lossy().into_owned();
    let finish = |state, converted, failures, paused_reason| VariantReport {
        state,
        output_dir: output_dir.clone(),
        converted,
        skipped: plan.already_done,
        failures,
        paused_reason,
    };
    let guard = match OutputGuard::new(&plan.output_dir)
        .and_then(|guard| prepare_targets(plan, &guard).map(|_| guard))
    {
        Ok(guard) => guard,
        Err(error) => return finish(VariantJobState::Failed, 0, Vec::new(), Some(error)),
    };
    let mut job = Job {
        camera,
        plan,
        guard,
        options,
        cancel,
        emit,
        meter: Meter::default(),
        converted: 0,
        failures: Vec::new(),
        since_disk_check: 0,
    };
    job.progress(
        VariantJobState::Running,
        None,
        None,
        Some("Preparazione…".into()),
    );
    let outcome = job.run();
    let (state, reason) = match outcome {
        Err(Stop::Paused(reason)) => (VariantJobState::Paused, Some(reason)),
        Err(Stop::Failed(reason)) => (VariantJobState::Failed, Some(reason)),
        Ok(()) if cancel.load(Ordering::SeqCst) => (VariantJobState::Cancelled, None),
        Ok(()) => (VariantJobState::Finished, None),
    };
    job.progress(state, None, None, reason.clone());
    let (converted, failures) = (job.converted, std::mem::take(&mut job.failures));
    finish(state, converted, failures, reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::sim::{synthetic_raf, SimulatedCamera};
    use crate::variants::plan::{build_plan, PresetDef};
    use crate::variants::types::VariantRequest;

    struct Fixture {
        dir: tempfile::TempDir,
        presets: Vec<PresetDef>,
    }

    fn fixture(rafs: usize, foreign: &[usize]) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..rafs {
            let model = if foreign.contains(&i) {
                "X100VI"
            } else {
                "X-T5"
            };
            fs::write(
                dir.path().join(format!("DSCF{:04}.RAF", i + 1)),
                synthetic_raf(model),
            )
            .unwrap();
        }
        let mut camera = FujiCamera::new(SimulatedCamera::new());
        let mut presets = vec![PresetDef::as_shot()];
        presets.extend(
            camera
                .read_banks()
                .unwrap()
                .into_iter()
                .take(2)
                .map(PresetDef::bank),
        );
        Fixture { dir, presets }
    }

    fn request(f: &Fixture) -> VariantRequest {
        VariantRequest {
            sources: vec![f.dir.path().to_string_lossy().into_owned()],
            output_dir: None,
            preset_ids: f.presets.iter().map(|p| p.id.clone()).collect(),
            overwrite: false,
        }
    }

    fn fast() -> RunOptions {
        RunOptions {
            busy_backoff: Duration::from_millis(1),
            ..RunOptions::default()
        }
    }

    fn run(
        camera: &mut FujiCamera<SimulatedCamera>,
        f: &Fixture,
    ) -> (VariantReport, Vec<VariantProgress>) {
        let plan = build_plan(&request(f), &f.presets).unwrap();
        let mut events = Vec::new();
        let report = run_plan(camera, &plan, fast(), &AtomicBool::new(false), &mut |e| {
            events.push(e)
        });
        (report, events)
    }

    fn jpgs(dir: &Path) -> usize {
        walk(dir)
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
            .count()
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn converts_everything_with_one_upload_per_raf() {
        let f = fixture(3, &[]);
        let originals: Vec<_> = (1..=3)
            .map(|i| fs::read(f.dir.path().join(format!("DSCF{i:04}.RAF"))).unwrap())
            .collect();
        let mut camera = FujiCamera::new(SimulatedCamera::new());
        let (report, events) = run(&mut camera, &f);
        assert_eq!(report.state, VariantJobState::Finished, "{report:?}");
        assert_eq!(report.converted, 9);
        assert_eq!(camera.transport_mut().uploads, 3);
        let out = f.dir.path().join("FujiView varianti");
        assert_eq!(jpgs(&out), 9);
        assert!(out.join("Come scattato").join("DSCF0001.jpg").exists());
        assert!(out.join("C1 Sim C1").join(MANIFEST).exists());
        assert!(walk(&out)
            .iter()
            .all(|p| !p.to_string_lossy().ends_with(PART_SUFFIX)));
        for (i, original) in originals.iter().enumerate() {
            assert_eq!(
                &fs::read(f.dir.path().join(format!("DSCF{:04}.RAF", i + 1))).unwrap(),
                original
            );
        }
        assert_eq!(events.last().unwrap().state, VariantJobState::Finished);
        // The scan used by the comparison view accepts the output as-is.
        let index = crate::scan_library_for_tests(&out).unwrap();
        assert_eq!(index.0, 3);
        assert_eq!(index.1, 3);
    }

    #[test]
    fn resume_after_disconnection_converts_only_missing() {
        let f = fixture(3, &[]);
        let mut sim = SimulatedCamera::new();
        sim.disconnect_after = Some(4);
        let mut camera = FujiCamera::new(sim);
        let (report, _) = run(&mut camera, &f);
        assert_eq!(report.state, VariantJobState::Paused);
        assert!(report.paused_reason.unwrap().contains("disconnessa"));
        assert_eq!(
            report.converted, 3,
            "the 4th JPEG was produced but not downloaded"
        );
        camera.transport_mut().reconnect();
        let (report, _) = run(&mut camera, &f);
        assert_eq!(report.state, VariantJobState::Finished);
        assert_eq!((report.converted, report.skipped), (6, 3));
        assert_eq!(jpgs(&f.dir.path().join("FujiView varianti")), 9);
    }

    #[test]
    fn foreign_raf_is_skipped_and_reported() {
        let f = fixture(3, &[1]);
        let mut camera = FujiCamera::new(SimulatedCamera::new());
        let (report, _) = run(&mut camera, &f);
        assert_eq!(report.state, VariantJobState::Finished);
        assert_eq!(report.converted, 6);
        assert_eq!(report.failures.len(), 3);
        assert!(
            report
                .failures
                .iter()
                .all(|x| x.raf == "DSCF0002.RAF"
                    && x.reason.contains("non scattato da questa camera"))
        );
    }

    #[test]
    fn cancel_stops_between_conversions() {
        let f = fixture(3, &[]);
        let plan = build_plan(&request(&f), &f.presets).unwrap();
        let cancel = AtomicBool::new(false);
        let mut camera = FujiCamera::new(SimulatedCamera::new());
        let mut seen = 0;
        let report = run_plan(&mut camera, &plan, fast(), &cancel, &mut |e| {
            if e.completed >= 2 {
                seen += 1;
                cancel.store(true, Ordering::SeqCst);
            }
        });
        assert_eq!(report.state, VariantJobState::Cancelled);
        assert!(report.converted < 9 && report.converted >= 2);
        assert!(seen > 0);
        let out = f.dir.path().join("FujiView varianti");
        assert!(walk(&out)
            .iter()
            .all(|p| !p.to_string_lossy().ends_with(PART_SUFFIX)));
    }

    #[test]
    fn busy_camera_is_retried() {
        let f = fixture(1, &[]);
        let mut sim = SimulatedCamera::new();
        sim.busy_starts = 2;
        let mut camera = FujiCamera::new(sim);
        let (report, _) = run(&mut camera, &f);
        assert_eq!(
            (report.state, report.converted),
            (VariantJobState::Finished, 3)
        );
    }

    #[test]
    fn falls_back_to_one_upload_per_conversion() {
        let f = fixture(2, &[]);
        let mut sim = SimulatedCamera::new();
        sim.single_conversion_per_upload = true;
        let mut camera = FujiCamera::new(sim);
        let (report, _) = run(&mut camera, &f);
        assert_eq!(
            (report.state, report.converted),
            (VariantJobState::Finished, 6),
            "{report:?}"
        );
        assert_eq!(camera.transport_mut().uploads, 6);
    }

    #[test]
    fn hung_camera_fails_the_job_with_power_cycle_hint() {
        let f = fixture(1, &[]);
        let mut sim = SimulatedCamera::new();
        sim.hang_conversions = true;
        let mut camera = FujiCamera::new(sim);
        camera.conversion_timeout = Duration::from_millis(20);
        camera.poll_interval = Duration::from_millis(2);
        let (report, _) = run(&mut camera, &f);
        assert_eq!(report.state, VariantJobState::Failed);
        assert!(report
            .paused_reason
            .unwrap()
            .contains("spegnila e riaccendila"));
    }

    #[test]
    fn leftover_part_files_are_removed() {
        let f = fixture(1, &[]);
        let leftover = f.dir.path().join("FujiView varianti").join("Come scattato");
        fs::create_dir_all(&leftover).unwrap();
        // State left by an interrupted run: our manifest plus a partial JPEG.
        fs::write(
            leftover.join(MANIFEST),
            serde_json::to_string(&PresetDef::as_shot().manifest()).unwrap(),
        )
        .unwrap();
        fs::write(leftover.join("DSCF0001.jpg.part"), b"half").unwrap();
        let outside = f.dir.path().join("keep.part");
        fs::write(&outside, b"not ours").unwrap();
        let mut camera = FujiCamera::new(SimulatedCamera::new());
        run(&mut camera, &f);
        assert!(!leftover.join("DSCF0001.jpg.part").exists());
        assert!(outside.exists());
    }

    #[test]
    fn guard_rejects_paths_outside_output() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("out");
        let guard = OutputGuard::new(&root).unwrap();
        fs::create_dir_all(root.join("C1")).unwrap();
        assert!(guard.check(&root.join("C1").join("a.jpg")).is_ok());
        assert!(
            guard.check(&root.join("a.jpg")).is_err(),
            "root itself is not a preset folder"
        );
        assert!(guard
            .check(&root.join("C1").join("..").join("..").join("evil.jpg"))
            .is_err());
        assert!(guard.check(&dir.path().join("evil.jpg")).is_err());
        assert!(write_atomic(&guard, &dir.path().join("evil.jpg"), b"x").is_err());
        assert!(!dir.path().join("evil.jpg").exists());
        // A preset folder that is a link to somewhere else is refused.
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&elsewhere, root.join("C2")).is_ok();
        #[cfg(not(windows))]
        let linked = std::os::unix::fs::symlink(&elsewhere, root.join("C2")).is_ok();
        if linked {
            assert!(guard.check(&root.join("C2").join("a.jpg")).is_err());
        }
    }
}
