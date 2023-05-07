use crate::{
    cdbg, ceprintln, cprint, cprintln, lua,
    mem::{get_module, scan, search},
    parser::Cmd, discord,
};
use anyhow::{bail, Result};
use derivative::Derivative;
use detour3::GenericDetour;
use futures::executor::block_on;
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, NasmFormatter};
use once_cell::sync::Lazy;
use pelite::{pe::PeView, pe32::Pe};
use std::{
    collections::HashMap,
    ffi::{c_char, CStr, CString},
    fmt::Debug,
    ptr,
    thread::JoinHandle,
    time::Duration,
};
use winsafe::HINSTANCE;
use winsafe::{co::TH32CS, prelude::*, HPROCESSLIST};

const POINTER_SIZE: usize = std::mem::size_of::<*const ()>();

#[repr(C)]
struct VirtualMethodTable(*const *const ());

impl Debug for VirtualMethodTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut methods = vec![];
        for idx in 0.. {
            let ptr = self.get::<()>(idx);
            if ptr.is_null()
                || !region::query(ptr)
                    .map(|r| r.is_executable())
                    .unwrap_or(false)
            {
                break;
            }
            methods.push(ptr);
        }
        f.debug_tuple("VMT").field(&methods).finish()
    }
}

impl VirtualMethodTable {
    fn get<T>(&self, offset: usize) -> *const T {
        unsafe { self.0.add(POINTER_SIZE * offset).read() as *const T }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Scrap {
    print: extern "C" fn(u32, *const c_char, u8),
    console_detour: GenericDetour<extern "C" fn(*const c_char)>,
    world: WorldPointer,
    discord_thread_handle: JoinHandle<Result<()>>,
}

#[repr(C)]
#[derive(Debug)]
struct Entity {
    vmt: VirtualMethodTable,
}

#[repr(C)]
#[derive(Debug)]
struct HashTableEntry<T> {
    data: *const T,
    name: *const c_char,
    next: *const Self,
}

#[repr(C)]
struct HashTable<T> {
    num_slots: u32,
    chains: *const *const HashTableEntry<T>,
}

fn try_read<T>(ptr: *const T) -> Option<T> {
    (!ptr.is_null()).then(|| unsafe { ptr.read() })
}

impl<T: std::fmt::Debug> std::fmt::Debug for HashTable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut entries: HashMap<String, Option<T>> = HashMap::default();
        for offset in 0..self.num_slots {
            let offset = offset as _;
            // let chain=vec![];
            let mut chain_ptr = unsafe { self.chains.offset(offset).read() };
            while !chain_ptr.is_null() {
                let entry = unsafe { chain_ptr.read() };
                let data = try_read(entry.data);
                let key = unsafe { CStr::from_ptr(entry.name) }
                    .to_str()
                    .unwrap()
                    .to_owned();
                chain_ptr = entry.next;
                entries.insert(key, data);
            }
        }
        f.debug_struct(&format!("HashTable @ {self:p} "))
            .field("num_slots", &self.num_slots)
            .field("entries", &entries)
            .finish()
    }
}

#[repr(C)]
struct World {
    vmt: VirtualMethodTable,
    entities: HashTable<Entity>,
}

impl Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("vmt", &self.vmt)
            .field("entities", &self.entities)
            .finish()
    }
}

struct WorldPointer(u32);

impl std::fmt::Debug for WorldPointer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ptr = self.ptr();
        let world = unsafe { ptr.read() };
        f.debug_tuple(&format!("WorldPointer @ {ptr:p} "))
            .field(&world)
            .finish()
    }
}

impl WorldPointer {
    fn ptr(&self) -> *const World {
        let ptr = self.0 as *const *const World;
        unsafe { ptr.read() }
    }
    
    fn get_hashtable(&self) {
        // let ents = unsafe { self.ptr().read().entities.read() };
        // cprintln!("Ents: {ents:?}");
    }
}

pub(crate) static SCRAP: Lazy<Scrap> =
    Lazy::new(|| Scrap::init().expect("Failed to initialize Scrap data structure"));

impl Scrap {
    const PRINT_PATTERN: &str = r#"6a0068 *{"Scrap engine"} 6a?e8 $'"#;
    const PY_EXEC: &str = r#"68 *{"import Viewer"} e8 $'"#;
    const WORLD_PATTERN: &str = r#"8b 0d *{'} 68 *"CTFFriends""#;
    fn init() -> Result<Self> {
        let scrap = unsafe {
            Self {
                world: WorldPointer(search(Self::WORLD_PATTERN, 1, None)? as _),
                print: std::mem::transmute(search(Self::PRINT_PATTERN, 1, None)?),
                console_detour: GenericDetour::<extern "C" fn(*const c_char)>::new(
                    std::mem::transmute(search(Self::PY_EXEC, 1, None)?),
                    Self::console_input,
                )?,
                discord_thread_handle: discord::Client::run()?,
            }
        };
        unsafe { scrap.console_detour.enable()? }
        Ok(scrap)
    }

    extern "C" fn console_input(orig_line: *const c_char) {
        let line = unsafe { CStr::from_ptr(orig_line) }.to_str();
        let Ok(line) = line else {
            return SCRAP.console_detour.call(orig_line);
        };
        if let Some(cmd) = line.strip_prefix('$') {
            let res = cmd.parse().and_then(|cmd: Cmd| cmd.exec());
            if let Err(err) = res {
                ceprintln!("Error: {err}");
            }
            return;
        };
        SCRAP.console_detour.call(orig_line)
    }

    pub fn println(&self, msg: &str) {
        self.println_c(0x008000, msg)
    }

    pub fn print(&self, msg: &str) {
        self.print_c(0x008000, msg)
    }

    pub fn print_c(&self, col: u32, msg: &str) {
        let col = (col & 0xffffff).swap_bytes() >> 8; // 0xRRGGBB -> 0xBBGGRR
        let msg = CString::new(msg.to_string()).unwrap();
        (self.print)(col, msg.as_ptr(), 0);
    }

    pub fn println_c(&self, col: u32, msg: &str) {
        let msg = msg.to_owned() + "\n";
        self.print_c(col, &msg)
    }
}

impl Cmd {
    pub(crate) fn exec(&self) -> Result<()> {
        let pe = get_module(None)?;
        match self {
            Cmd::Imports => {
                for import in pe.imports()? {
                    let iat = import.iat()?;
                    let int = import.int()?;
                    for (func, imp) in iat.zip(int) {
                        let imp = imp?;
                        cprintln!(
                            "{addr:p}: {name} {imp:?}",
                            name = import.dll_name()?,
                            addr = func
                        );
                    }
                }
            }
            Cmd::Read(addr, size) => {
                let ptr = *addr as *const u8;
                let info = region::query(ptr)?;
                let end = info.as_ptr_range::<()>().end as u32;
                let size = ((end - addr) as usize).min(*size);
                if !info.is_readable() {
                    bail!("No read permission on page");
                }
                let data = unsafe { std::slice::from_raw_parts(ptr, size) };
                cprintln!("{}", &rhexdump::hexdump_offset(data, *addr));
            }
            Cmd::Disams(addr, size) => {
                let ptr = *addr as *const u8;
                let info = region::query(ptr)?;
                let end = info.as_ptr_range::<()>().end as u32;
                let size = ((end - addr) as usize).min(*size);
                if !info.is_readable() {
                    bail!("No read permission on page");
                }
                let data = unsafe { std::slice::from_raw_parts(ptr, size) };
                let mut decoder = Decoder::with_ip(32, data, *addr as u64, DecoderOptions::NONE);
                let mut instruction = Instruction::default();
                let mut output = String::new();
                let mut formatter = NasmFormatter::new();
                while decoder.can_decode() {
                    decoder.decode_out(&mut instruction);
                    output.clear();
                    formatter.format(&instruction, &mut output);
                    cprint!("{:016X} ", instruction.ip());
                    let start_index = (instruction.ip() - (*addr as u64)) as usize;
                    let instr_bytes = &data[start_index..start_index + instruction.len()];
                    for b in instr_bytes.iter() {
                        cprint!("{:02X}", b);
                    }
                    cprintln!(" {}", output);
                }
            }
            Cmd::Write(addr, data) => {
                let data = data.as_slice();
                let addr = *addr as *const u8;
                unsafe {
                    let handle = region::protect_with_handle(
                        addr,
                        data.len(),
                        region::Protection::READ_WRITE_EXECUTE,
                    )?;
                    std::ptr::copy(data.as_ptr(), addr as *mut u8, data.len());
                    drop(handle);
                };
            }
            Cmd::ReadPE(addr, size) => {
                if !region::query(*addr as *const ())?.is_readable() {
                    bail!("No read permission for 0x{addr:x}");
                }
                let data = pe.read_bytes(*addr)?;
                cprintln!("{}", &rhexdump::hexdump_offset(&data[..*size], *addr));
            }
            Cmd::Info(None) => {
                let regions = region::query_range(ptr::null::<()>(), usize::MAX)?;
                for region in regions.flatten() {
                    cprintln!(
                        "{:?}: {}",
                        region.as_ptr_range::<*const ()>(),
                        region.protection()
                    );
                }
            }
            Cmd::Info(Some(addr)) => {
                let info = region::query(*addr as *const ())?;
                cprintln!(
                    "{:?}: {}",
                    info.as_ptr_range::<*const ()>(),
                    info.protection()
                );
            }
            Cmd::ScanModule(pat, module) => {
                cprintln!("{:?}", pat);
                let mut total_hits = 0;
                let mut modules = vec![];
                let is_wildcard = matches!(module.as_deref(), Some("*"));
                if is_wildcard {
                    let pid = std::process::id();
                    let mut h_snap =
                        HPROCESSLIST::CreateToolhelp32Snapshot(TH32CS::SNAPMODULE, Some(pid))?;
                    for module in h_snap.iter_modules() {
                        let module = module?;
                        let module_name = module.szModule();
                        let module_addr = module.hModule.as_ptr() as *const u8;
                        let module = region::query_range(module_addr, module.modBaseSize as usize)?
                            .all(|m| m.ok().map(|m| m.is_readable()).unwrap_or(false))
                            .then(|| unsafe { PeView::module(module_addr) });
                        if let Some(module) = module {
                            modules.push((module_name, module));
                        }
                    }
                } else {
                    let module = HINSTANCE::GetModuleHandle(module.as_deref())?;
                    let module_name = module.GetModuleFileName()?;
                    let module_addr = module.as_ptr() as *const u8;
                    let module = region::query(module_addr)
                        .map(|m| m.is_readable())
                        .unwrap_or(false)
                        .then(|| unsafe { PeView::module(module_addr) });
                    if let Some(module) = module {
                        modules.push((module_name, module));
                    };
                }
                for (module_name, pe) in modules {
                    let res = scan(pat, &pe)?;
                    if res.is_empty() {
                        continue;
                    }
                    total_hits += res.len();
                    cprintln!("Module: {module_name}");
                    let sections = pe.section_headers();
                    for hit in &res {
                        for (idx, addr) in hit.iter().enumerate() {
                            let mut section_name = String::from("<invalid address>");
                            if let Ok(section_rva) = pe.va_to_rva(*addr) {
                                if let Some(section) = sections.by_rva(section_rva) {
                                    section_name = match section.name() {
                                        Ok(name) => name.to_string(),
                                        Err(name_bytes) => format!("{name_bytes:?}"),
                                    };
                                } else {
                                    section_name = String::from("<invalid section>");
                                }
                            };
                            if let Ok(region) = region::query(addr) {
                                cprintln!(
                                    "\t{}: {:?} {} [{}] {:p}",
                                    idx,
                                    region.as_ptr_range::<()>(),
                                    region.protection(),
                                    section_name,
                                    addr
                                )
                            }
                        }
                    }
                }
                cprintln!("Results: {total_hits}");
            }
            Cmd::Lua(code) => {
                lua::exec(code)?;
            }
            Cmd::Script(path) => {
                for line in std::fs::read_to_string(path)?.lines() {
                    line.parse().and_then(|cmd: Cmd| cmd.exec())?;
                }
            }
            other => bail!("Not implemented: {other:?}"),
        }
        Ok(())
    }
}
