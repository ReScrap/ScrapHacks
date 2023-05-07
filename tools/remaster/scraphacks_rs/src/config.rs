enum FilePatch {

}

pub struct Config {
    file_patches: FxHashMap<PathBuf,FilePatch>
}