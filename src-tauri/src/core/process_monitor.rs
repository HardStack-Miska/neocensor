use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

use crate::models::AppCategory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningProcess {
    pub pid: u32,
    pub name: String,
    pub exe_path: String,
    pub category: AppCategory,
}

pub struct ProcessMonitor {
    system: System,
    /// Cache of known app categories by exe name
    known_apps: HashMap<String, (&'static str, AppCategory)>,
}

impl ProcessMonitor {
    pub fn new() -> Self {
        let mut known_apps = HashMap::new();

        // Browsers
        for (exe, display) in [
            ("chrome.exe", "Google Chrome"),
            ("firefox.exe", "Firefox"),
            ("msedge.exe", "Microsoft Edge"),
            ("opera.exe", "Opera"),
            ("brave.exe", "Brave"),
            ("vivaldi.exe", "Vivaldi"),
            ("yandex.exe", "Yandex Browser"),
        ] {
            known_apps.insert(exe.to_lowercase(), (display, AppCategory::Browser));
        }

        // Communication
        for (exe, display) in [
            ("Discord.exe", "Discord"),
            ("Telegram.exe", "Telegram"),
            ("Teams.exe", "Microsoft Teams"),
            ("Skype.exe", "Skype"),
            ("Slack.exe", "Slack"),
            ("Zoom.exe", "Zoom"),
        ] {
            known_apps.insert(exe.to_lowercase(), (display, AppCategory::Communication));
        }

        // Gaming
        for (exe, display) in [
            ("steam.exe", "Steam"),
            ("steamwebhelper.exe", "Steam WebHelper"),
            ("EpicGamesLauncher.exe", "Epic Games"),
            ("dota2.exe", "Dota 2"),
            ("cs2.exe", "Counter-Strike 2"),
            ("VALORANT.exe", "Valorant"),
            ("GenshinImpact.exe", "Genshin Impact"),
            ("LeagueClient.exe", "League of Legends"),
            ("RiotClientServices.exe", "Riot Client"),
        ] {
            known_apps.insert(exe.to_lowercase(), (display, AppCategory::Gaming));
        }

        // Streaming
        for (exe, display) in [
            ("Spotify.exe", "Spotify"),
        ] {
            known_apps.insert(exe.to_lowercase(), (display, AppCategory::Streaming));
        }

        // Development
        for (exe, display) in [
            ("Code.exe", "VS Code"),
            ("idea64.exe", "IntelliJ IDEA"),
            ("webstorm64.exe", "WebStorm"),
            ("pycharm64.exe", "PyCharm"),
            ("goland64.exe", "GoLand"),
            ("clion64.exe", "CLion"),
        ] {
            known_apps.insert(exe.to_lowercase(), (display, AppCategory::Development));
        }

        Self {
            system: System::new_with_specifics(
                RefreshKind::nothing().with_processes(
                    ProcessRefreshKind::nothing()
                        .with_exe(UpdateKind::OnlyIfNotSet),
                ),
            ),
            known_apps,
        }
    }

    /// Refresh the process list and return GUI-like applications.
    pub fn list_processes(&mut self) -> Vec<RunningProcess> {
        self.system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_exe(UpdateKind::OnlyIfNotSet),
        );

        let mut seen = HashMap::new();
        let mut result = Vec::new();

        for (pid, process) in self.system.processes() {
            let name = process.name().to_string_lossy().to_string();

            // Skip system processes
            if is_system_process(&name) {
                continue;
            }

            // Deduplicate by name (keep first instance)
            if seen.contains_key(&name.to_lowercase()) {
                continue;
            }
            seen.insert(name.to_lowercase(), true);

            let exe_path = process
                .exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let category = self
                .known_apps
                .get(&name.to_lowercase())
                .map(|(_, cat)| *cat)
                .unwrap_or(AppCategory::Other);

            result.push(RunningProcess {
                pid: pid.as_u32(),
                name,
                exe_path,
                category,
            });
        }

        // Sort: known apps first, then alphabetically
        result.sort_by(|a, b| {
            let a_known = a.category != AppCategory::Other;
            let b_known = b.category != AppCategory::Other;
            b_known
                .cmp(&a_known)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        result
    }

    /// Get display name for a process exe name.
    pub fn display_name(&self, exe_name: &str) -> String {
        self.known_apps
            .get(&exe_name.to_lowercase())
            .map(|(display, _)| display.to_string())
            .unwrap_or_else(|| exe_name.replace(".exe", ""))
    }

    /// Get category for a process exe name.
    pub fn category(&self, exe_name: &str) -> AppCategory {
        self.known_apps
            .get(&exe_name.to_lowercase())
            .map(|(_, cat)| *cat)
            .unwrap_or(AppCategory::Other)
    }

    /// Scan processes once and return a map of process_name (lowercase) -> exe_path.
    /// Reuse this map to avoid repeated full scans.
    pub fn build_exe_map(&mut self) -> HashMap<String, String> {
        self.system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_exe(UpdateKind::OnlyIfNotSet),
        );

        let mut map = HashMap::new();
        for (_pid, process) in self.system.processes() {
            let name = process.name().to_string_lossy().to_lowercase();
            if map.contains_key(&name) {
                continue;
            }
            if let Some(path) = process.exe() {
                let path_str = path.to_string_lossy().to_string();
                if !path_str.is_empty() {
                    map.insert(name, path_str);
                }
            }
        }
        map
    }
}

static SYSTEM_PROCS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    [
        "svchost.exe",
        "csrss.exe",
        "wininit.exe",
        "winlogon.exe",
        "lsass.exe",
        "services.exe",
        "smss.exe",
        "dwm.exe",
        "fontdrvhost.exe",
        "conhost.exe",
        "runtimebroker.exe",
        "searchhost.exe",
        "shellexperiencehost.exe",
        "sihost.exe",
        "taskhostw.exe",
        "ctfmon.exe",
        "dllhost.exe",
        "wmiprvse.exe",
        "spoolsv.exe",
        "audiodg.exe",
        "system",
        "idle",
        "registry",
        "memory compression",
        "wuauserv.exe",
        "securityhealthservice.exe",
        "msmpeng.exe",
        "nissrv.exe",
        "sgrmbroker.exe",
        "systemsettingsbroker.exe",
        "backgroundtaskhost.exe",
        "textinputhost.exe",
        "widgetservice.exe",
        "explorer.exe",
        "searchindexer.exe",
        "startmenuexperiencehost.exe",
    ]
    .into_iter()
    .map(String::from)
    .collect()
});

fn is_system_process(name: &str) -> bool {
    SYSTEM_PROCS.contains(&name.to_lowercase())
}
