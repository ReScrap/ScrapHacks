use std::path::PathBuf;

use steamlocate::SteamDir;
use anyhow::{bail,Result};
const APP_ID: u32 = 897610;

pub(crate) fn get_executable() -> Result<PathBuf> {
    let Some(mut steam) = SteamDir::locate() else {
        bail!("Failed to find steam folder");
    };
    let Some(app) = steam.app(&APP_ID) else {
        bail!("App {APP_ID} is not installed!");
    };
    Ok(app.path.clone())
}