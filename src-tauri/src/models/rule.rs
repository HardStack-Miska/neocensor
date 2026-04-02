use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteMode {
    Proxy,
    Direct,
    Block,
    Auto,
}

impl Default for RouteMode {
    fn default() -> Self {
        Self::Direct
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRoute {
    pub process_name: String,
    pub display_name: String,
    pub mode: RouteMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_path: Option<String>,
    /// Full path to the executable (for WFP filtering).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe_path: Option<String>,
    pub category: AppCategory,
}

impl AppRoute {
    pub fn new(process_name: impl Into<String>, mode: RouteMode) -> Self {
        let name = process_name.into();
        Self {
            display_name: name.replace(".exe", ""),
            process_name: name,
            mode,
            icon_path: None,
            exe_path: None,
            category: AppCategory::Other,
        }
    }

    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = name.into();
        self
    }

    pub fn with_category(mut self, category: AppCategory) -> Self {
        self.category = category;
        self
    }

    pub fn with_exe_path(mut self, path: impl Into<String>) -> Self {
        self.exe_path = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppCategory {
    Browser,
    Communication,
    Gaming,
    Streaming,
    Development,
    System,
    Other,
}

impl Default for AppCategory {
    fn default() -> Self {
        Self::Other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub routes: Vec<AppRoute>,
    pub default_mode: RouteMode,
}

impl Profile {
    pub fn default_profiles() -> Vec<Self> {
        vec![
            Self {
                id: "gaming".into(),
                name: "Gaming".into(),
                icon: "gamepad".into(),
                routes: vec![
                    AppRoute::new("Discord.exe", RouteMode::Proxy)
                        .with_display_name("Discord")
                        .with_category(AppCategory::Communication),
                    AppRoute::new("chrome.exe", RouteMode::Auto)
                        .with_display_name("Chrome")
                        .with_category(AppCategory::Browser),
                    AppRoute::new("dota2.exe", RouteMode::Direct)
                        .with_display_name("Dota 2")
                        .with_category(AppCategory::Gaming),
                    AppRoute::new("cs2.exe", RouteMode::Direct)
                        .with_display_name("Counter-Strike 2")
                        .with_category(AppCategory::Gaming),
                    AppRoute::new("steam.exe", RouteMode::Direct)
                        .with_display_name("Steam")
                        .with_category(AppCategory::Gaming),
                ],
                default_mode: RouteMode::Direct,
            },
            Self {
                id: "work".into(),
                name: "Work".into(),
                icon: "briefcase".into(),
                routes: vec![
                    AppRoute::new("chrome.exe", RouteMode::Proxy)
                        .with_display_name("Chrome")
                        .with_category(AppCategory::Browser),
                    AppRoute::new("firefox.exe", RouteMode::Proxy)
                        .with_display_name("Firefox")
                        .with_category(AppCategory::Browser),
                    AppRoute::new("Discord.exe", RouteMode::Proxy)
                        .with_display_name("Discord")
                        .with_category(AppCategory::Communication),
                    AppRoute::new("Telegram.exe", RouteMode::Proxy)
                        .with_display_name("Telegram")
                        .with_category(AppCategory::Communication),
                    AppRoute::new("Code.exe", RouteMode::Proxy)
                        .with_display_name("VS Code")
                        .with_category(AppCategory::Development),
                ],
                default_mode: RouteMode::Proxy,
            },
            Self {
                id: "smart".into(),
                name: "Smart (Geo)".into(),
                icon: "globe".into(),
                routes: vec![],
                default_mode: RouteMode::Auto,
            },
            Self {
                id: "full_vpn".into(),
                name: "Full VPN".into(),
                icon: "shield".into(),
                routes: vec![],
                default_mode: RouteMode::Proxy,
            },
        ]
    }
}
