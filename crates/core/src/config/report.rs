use super::SettingView;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ConfigReport {
    pub server_id: String,
    pub saved_revision: i64,
    pub applied_revision: Option<i64>,
    pub items: Vec<ConfigItem>,
}

#[derive(Debug, Serialize)]
pub struct ConfigItem {
    #[serde(flatten)]
    pub setting: SettingView,
    pub application_state: &'static str,
}
