use std::str::FromStr;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Result;
use pelite::pattern::parse;
use pelite::pattern::save_len;
use pelite::pattern::Atom;
use pelite::pe32::{Pe, PeView};
use winsafe::co::TH32CS;
use winsafe::prelude::*;
use winsafe::HINSTANCE;
use winsafe::HPROCESSLIST;
pub(crate) struct Pattern(Vec<Atom>, usize);

impl Pattern {
    pub(crate) fn set_index(mut self, idx: usize) -> Self {
        self.1 = idx;
        self
    }
}

impl FromStr for Pattern {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(parse(s)?, 0))
    }
}

impl Pattern {
    pub(crate) fn scan(&self, module: Option<String>) -> Result<u32> {
        let pe = get_module(module)?;
        let scan = pe.scanner();
        let mut save = vec![0u32; save_len(&self.0)];
        if !scan.finds(&self.0, 0..u32::MAX, &mut save) {
            bail!("Pattern not found");
        }
        save.get(self.1)
            .ok_or_else(|| anyhow!("Result index out of range"))
            .and_then(|r| pe.rva_to_va(*r).map_err(|e| e.into()))
    }
}

pub(crate) fn get_modules() -> Result<Vec<PeView<'static>>> {
    let mut res = vec![];
    let pid = std::process::id();
    let mut h_snap = HPROCESSLIST::CreateToolhelp32Snapshot(TH32CS::SNAPMODULE, Some(pid))?;
    for module in h_snap.iter_modules() {
        res.push(unsafe { PeView::module(module?.hModule.as_ptr() as *const u8) });
    }
    Ok(res)
}

pub(crate) fn get_module(module: Option<String>) -> Result<PeView<'static>> {
    let hmodule = HINSTANCE::GetModuleHandle(module.as_deref())?;
    Ok(unsafe { PeView::module(hmodule.as_ptr() as *const u8) })
}

pub(crate) fn scan(pat: &[Atom], pe: &PeView) -> Result<Vec<Vec<u32>>> {
    let mut ret = vec![];
    let scan = pe.scanner();
    let mut m = scan.matches(pat, 0..u32::MAX);
    let mut save = vec![0u32; save_len(pat)];
    while m.next(&mut save) {
        ret.push(
            save.iter()
                .map(|rva| pe.rva_to_va(*rva).map_err(|e| e.into()))
                .collect::<Result<Vec<u32>>>()?,
        );
    }
    Ok(ret)
}

pub(crate) fn search(pat: &str, idx: usize, module: Option<String>) -> Result<u32> {
    pat.parse::<Pattern>()?.set_index(idx).scan(module)
}

fn addr_info(addr: u32) -> Result<()> {
    let pid = std::process::id();
    let mut h_snap = HPROCESSLIST::CreateToolhelp32Snapshot(TH32CS::SNAPMODULE, Some(pid))?;
    for module in h_snap.iter_modules() {
        let module = module?;
        let module_name = module.szModule();
        if module_name.to_lowercase() == "kernel32.dll" {
            continue;
        }
        let mod_range =
            unsafe { module.modBaseAddr..module.modBaseAddr.offset(module.modBaseSize as isize) };
        println!("{module_name}: {mod_range:?}");
        // let module = unsafe { PeView::module(module.modBaseAddr as *const u8) };
    }
    Ok(())
}
