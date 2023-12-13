use std::path::PathBuf;
use steamlocate::SteamDir;
const APP_ID: u32 = 897610;

pub(crate) fn get_path() -> Option<PathBuf> {
    let Some(mut steam) = SteamDir::locate() else {
        return None;
    };
    let Some(app) = steam.app(&APP_ID) else {
        return None;
    };
    Some(app.path.clone())
}
