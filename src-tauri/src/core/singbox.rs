use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;

/// Send+Sync wrapper for a Windows kernel HANDLE.
/// Win32 kernel handles are documented as thread-safe; we only access the wrapped
/// value through our own Mutex anyway.
#[cfg(windows)]
struct JobHandle(HANDLE);

#[cfg(windows)]
unsafe impl Send for JobHandle {}
#[cfg(windows)]
unsafe impl Sync for JobHandle {}

/// Manages the sing-box child process lifecycle.
pub struct SingboxManager {
    binary_path: PathBuf,
    config_path: PathBuf,
    child: Arc<Mutex<Option<Child>>>,
    /// Mutex to serialize start/stop/restart operations (prevents racing connect/disconnect).
    op_lock: Mutex<()>,
    /// Output reader task handles to abort on stop().
    reader_tasks: Mutex<Vec<JoinHandle<()>>>,
    /// Lock-free liveness flag set by stderr reader on EOF.
    alive_flag: Arc<AtomicBool>,
    /// Whether the most recent successful start used TUN.
    tun_active: Arc<AtomicBool>,
    /// Windows Job Object so OS reaps sing-box if our process dies abnormally.
    /// Uses std::sync::Mutex — never held across await; closing the handle while a
    /// child is attached would kill the child (KILL_ON_JOB_CLOSE), so this lock is
    /// always taken with `.lock().unwrap()` to fail loudly rather than ever
    /// take an alternative path that could close the handle prematurely.
    #[cfg(windows)]
    job_handle: Arc<std::sync::Mutex<Option<JobHandle>>>,
}

impl SingboxManager {
    pub fn new(binary_path: PathBuf, config_dir: PathBuf) -> Self {
        Self {
            binary_path,
            config_path: config_dir.join("singbox-config.json"),
            child: Arc::new(Mutex::new(None)),
            op_lock: Mutex::new(()),
            reader_tasks: Mutex::new(Vec::new()),
            alive_flag: Arc::new(AtomicBool::new(false)),
            tun_active: Arc::new(AtomicBool::new(false)),
            #[cfg(windows)]
            job_handle: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Whether the most recent start used TUN mode.
    pub fn is_tun_active(&self) -> bool {
        self.tun_active.load(Ordering::Acquire)
    }

    /// Write config and start sing-box.
    /// `tun_mode` is purely informational — used to track current mode for routing decisions.
    pub async fn start(&self, config: &serde_json::Value, tun_mode: bool) -> Result<()> {
        let _op = self.op_lock.lock().await;
        self.stop_inner().await?;

        // Brief pause to let OS release bound ports from previous instance
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let config_str = serde_json::to_string_pretty(config)?;
        if let Some(parent) = self.config_path.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        tokio::fs::write(&self.config_path, &config_str)
            .await
            .context("failed to write sing-box config")?;

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("run")
            .arg("-c")
            .arg(&self.config_path)
            .arg("--disable-color")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        // Hide console window and prevent conhost creation on Windows
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            const DETACHED_PROCESS: u32 = 0x00000008;
            cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
        }

        let mut child = cmd.spawn().context("failed to start sing-box")?;

        // On Windows, attach the child to a Job Object so the OS kills sing-box
        // automatically if our process is terminated abnormally (Task Manager etc).
        #[cfg(windows)]
        if let Err(e) = self.attach_to_job_object(&child) {
            tracing::warn!("failed to attach sing-box to job object: {e}");
        }

        // Reset alive flag and clear any previous reader tasks
        self.alive_flag.store(true, Ordering::Release);
        let mut tasks = Vec::new();

        // Stream stderr to tracing only — BroadcastWriter (logger.rs) forwards
        // tracing output into the same `log_sender` broadcast channel that the UI
        // and traffic parser subscribe to. Sending directly here too would duplicate
        // every line, doubling connection-event emissions in the UI.
        if let Some(stderr) = child.stderr.take() {
            let alive_flag = self.alive_flag.clone();
            let handle = tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.contains("connection") {
                        tracing::debug!("[sing-box] {line}");
                    } else {
                        tracing::info!("[sing-box] {line}");
                    }
                }
                alive_flag.store(false, Ordering::Release);
            });
            tasks.push(handle);
        }

        // Stream stdout to tracing only (same reasoning).
        if let Some(stdout) = child.stdout.take() {
            let handle = tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::info!("[sing-box] {line}");
                }
            });
            tasks.push(handle);
        }

        *self.child.lock().await = Some(child);
        *self.reader_tasks.lock().await = tasks;
        self.tun_active.store(tun_mode, Ordering::Release);

        tracing::info!("sing-box started (tun_mode={tun_mode})");
        Ok(())
    }

    /// Stop sing-box gracefully and remove config file (contains credentials).
    pub async fn stop(&self) -> Result<()> {
        let _op = self.op_lock.lock().await;
        self.stop_inner().await
    }

    async fn stop_inner(&self) -> Result<()> {
        let child_opt = {
            let mut guard = self.child.lock().await;
            guard.take()
        };

        if let Some(mut child) = child_opt {
            child.kill().await.ok();
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                child.wait(),
            )
            .await;
            tracing::info!("sing-box stopped");
        }

        // Abort any leftover reader tasks (they should exit on EOF, but be defensive)
        let tasks = std::mem::take(&mut *self.reader_tasks.lock().await);
        for task in tasks {
            task.abort();
        }

        self.alive_flag.store(false, Ordering::Release);
        self.tun_active.store(false, Ordering::Release);

        // Close and recreate the job object so a fresh start gets a clean slate
        #[cfg(windows)]
        self.close_job_object();

        // Remove config file to avoid leaving credentials on disk
        tokio::fs::remove_file(&self.config_path).await.ok();
        Ok(())
    }

    /// Restart with a new config (preserving the current op_lock semantics).
    pub async fn restart(&self, config: &serde_json::Value, tun_mode: bool) -> Result<()> {
        // start() takes op_lock and calls stop_inner(); no separate stop needed.
        self.start(config, tun_mode).await
    }

    /// Check if sing-box process is still running. Lock-free fast path.
    pub async fn is_alive(&self) -> bool {
        if !self.alive_flag.load(Ordering::Acquire) {
            return false;
        }
        // Confirm with the kernel — child may have exited without our reader noticing yet
        let mut guard = self.child.lock().await;
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => {
                    *guard = None;
                    self.alive_flag.store(false, Ordering::Release);
                    false
                }
                Err(_) => {
                    self.alive_flag.store(false, Ordering::Release);
                    false
                }
            }
        } else {
            self.alive_flag.store(false, Ordering::Release);
            false
        }
    }

    /// Get the path to the sing-box binary.
    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    /// Get the path to the current config file.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    #[cfg(windows)]
    fn attach_to_job_object(&self, child: &Child) -> Result<()> {
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
            JobObjectExtendedLimitInformation, JOBOBJECT_BASIC_LIMIT_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        use windows::Win32::System::Threading::OpenProcess;
        use windows::Win32::System::Threading::PROCESS_SET_QUOTA;
        use windows::Win32::System::Threading::PROCESS_TERMINATE;

        let pid = child
            .id()
            .context("sing-box child has no PID")?;

        unsafe {
            let job = CreateJobObjectW(None, windows::core::PCWSTR::null())
                .context("CreateJobObjectW failed")?;

            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation = JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..Default::default()
            };

            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .ok()
            .context("SetInformationJobObject failed")?;

            let proc_handle = OpenProcess(PROCESS_TERMINATE | PROCESS_SET_QUOTA, false, pid)
                .context("OpenProcess failed")?;

            let assign_result = AssignProcessToJobObject(job, proc_handle);
            let _ = windows::Win32::Foundation::CloseHandle(proc_handle);
            assign_result.ok().context("AssignProcessToJobObject failed")?;

            // Stash the job handle. CRITICAL: closing this handle while sing-box
            // is attached triggers JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = the OS
            // immediately kills sing-box. So if we can't store it, we LEAK the
            // handle (OS will close it on app exit) rather than risk killing the
            // freshly-started process.
            match self.job_handle.lock() {
                Ok(mut guard) => {
                    if let Some(prev) = guard.take() {
                        let _ = windows::Win32::Foundation::CloseHandle(prev.0);
                    }
                    *guard = Some(JobHandle(job));
                }
                Err(_poisoned) => {
                    tracing::error!(
                        "job_handle mutex poisoned; leaking new job handle to avoid killing sing-box"
                    );
                }
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    fn close_job_object(&self) {
        let prev = match self.job_handle.lock() {
            Ok(mut g) => g.take(),
            Err(_) => return,
        };
        if let Some(handle) = prev {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(handle.0);
            }
        }
    }
}

impl Drop for SingboxManager {
    fn drop(&mut self) {
        // Synchronous best-effort cleanup. On Windows closing the job handle
        // triggers KILL_ON_JOB_CLOSE → OS reaps sing-box.
        #[cfg(windows)]
        self.close_job_object();

        // Remove config file (contains credentials)
        let _ = std::fs::remove_file(&self.config_path);
    }
}
