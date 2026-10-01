// Custom / Plugin
// to implement
#[derive(Debug, Clone)]
pub enum CustomEvent {
    PluginEvent { plugin_name: String, data: Vec<u8> },
    ThemeChanged,
    SettingsChanged,
}
