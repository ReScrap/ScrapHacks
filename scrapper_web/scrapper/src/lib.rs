use binrw::{binread, BinReaderExt};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use wasm_bindgen::prelude::*;
use wasm_bindgen_file_reader::WebSysFile;
use web_sys::{Blob, File};

type JsResult<T> = Result<T,JsValue>;

#[binread]
#[derive(Serialize, Debug)]
struct ScrapFile {
    #[br(temp)]
    name_len: u32,
    #[br(count = name_len)]
    #[br(map = |s: Vec<u8>| String::from_utf8_lossy(&s).to_string())]
    path: String,
    size: u32,
    offset: u32,
}

#[binread]
#[br(magic = b"BFPK", little)]
#[derive(Serialize, Debug)]
struct PackedHeader {
    version: u32,
    #[br(temp)]
    num_files: u32,
    #[br(count= num_files)]
    files: Vec<ScrapFile>,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum DirectoryTree {
    File {
        size: u32,
        offset: u32,
        file_index: u8,
    },
    Directory {
        entries: BTreeMap<String, DirectoryTree>,
    },
}

#[wasm_bindgen(inspectable)]
pub struct MultiPack {
    files: Vec<(String,WebSysFile)>,
    tree: DirectoryTree,
}

fn blob_url(buffer: &[u8]) -> JsResult<String> {
    let uint8arr =
        js_sys::Uint8Array::new(&unsafe { js_sys::Uint8Array::view(buffer) }.into());
    let array = js_sys::Array::new();
    array.push(&uint8arr.buffer());
    let blob = Blob::new_with_u8_array_sequence_and_options(
        &array,
        web_sys::BlobPropertyBag::new().type_("application/octet-stream"),
    )
    .unwrap();
    web_sys::Url::create_object_url_with_blob(&blob)
}

#[wasm_bindgen]
impl MultiPack {
    #[wasm_bindgen(constructor)]
    pub fn parse(files: Vec<File>) -> Self {
        let mut tree = DirectoryTree::default();
        let mut web_files = vec![];
        for (file_index, file) in files.into_iter().enumerate() {
            let file_name = file.name();
            let mut fh = WebSysFile::new(file);
            let header = fh.read_le::<PackedHeader>().unwrap();
            tree.merge(&header.files, file_index.try_into().unwrap());
            web_files.push((file_name,fh));
        }
        Self {
            tree,
            files: web_files,
        }
    }

    #[wasm_bindgen]
    pub fn tree(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.tree).unwrap()
    }

    #[wasm_bindgen]
    pub fn download(
        &mut self,
        file_index: u8,
        offset: u32,
        size: u32,
    ) -> Result<JsValue, JsValue> {
        let Some((_,file)) = self.files.get_mut(file_index as usize) else {
            return Err("File not found".into());
        };
        let mut buffer = vec![0u8; size as usize];
        file.seek(SeekFrom::Start(offset as u64))
            .map_err(|e| format!("Failed to seek file: {e}"))?;
        file.read(&mut buffer)
            .map_err(|e| format!("Failed to read from file: {e}"))?;
        Ok(blob_url(&buffer)?.into())
    }
}

impl Default for DirectoryTree {
    fn default() -> Self {
        Self::Directory {
            entries: Default::default(),
        }
    }
}

impl DirectoryTree {
    fn add_child(&mut self, name: &str, node: Self) -> &mut Self {
        match self {
            Self::File { .. } => panic!("Can't add child to file!"),
            Self::Directory {
                entries
            } => entries.entry(name.to_owned()).or_insert(node),
        }
    }

    fn merge(&mut self, files: &[ScrapFile], file_index: u8) {
        for file in files {
            let mut folder = &mut *self;
            let path: Vec<_> = file.path.split('/').collect();
            if let Some((filename, path)) = path.as_slice().split_last() {
                for part in path {
                    let DirectoryTree::Directory { entries } = folder else {
                            unreachable!();
                        };
                    folder = entries.entry(part.to_string()).or_default();
                }
                folder.add_child(
                    filename,
                    DirectoryTree::File {
                        size: file.size,
                        offset: file.offset,
                        file_index,
                    },
                );
            }
        }
    }
}

#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    Ok(())
}
