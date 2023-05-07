#![feature(abi_thiscall)]
#![feature(c_variadic)]
mod discord;
mod lua;
mod mem;
mod parser;
mod scrap;
use std::ffi::{c_char, c_void, CString};
use anyhow::Result;
use crate::mem::search;
use crate::scrap::SCRAP;
use shadow_rs::shadow;
use winsafe::{co::{MB, CS, WS}, prelude::*, HWND, WNDCLASSEX, RegisterClassEx, WString};

shadow!(build);

custom_print::define_macros!({cprint, cprintln, cdbg}, fmt, |value: &str| {crate::scrap::SCRAP.print(value)});
custom_print::define_macros!({ceprint, ceprintln}, fmt, |value: &str| {crate::scrap::SCRAP.print_c(0x800000,value)});

#[allow(clippy::single_component_path_imports)]
pub(crate) use {cdbg, cprint, cprintln};
#[warn(clippy::single_component_path_imports)]
pub(crate) use {ceprint, ceprintln};

#[repr(C)]
#[derive(Debug)]
struct PyMethodDef {
    name: *const c_char,
    func: *const (*const c_void, *const c_void),
    ml_flags: i32,
    doc: *const c_char,
}

#[repr(C)]
#[derive(Debug)]
struct PyModuleDef {
    name: *const c_char,
    methods: *const PyMethodDef,
}

fn init_py_mod() {
    let py_init_module: fn(
        *const c_char,      // name
        *const PyMethodDef, // methods
        *const c_char,      // doc
        *const (),          // passthrough
        i32,                // module_api_version
    ) -> *const () =
        unsafe { std::mem::transmute(search("68 *{\"Scrap\" 00} e8 ${'}", 1, None).unwrap_or_default()) };
    let name = CString::new("ScrapHack").unwrap_or_default();
    let desc = CString::new("ScrapHack Rust version").unwrap_or_default();
    let methods: &[PyMethodDef] = &[PyMethodDef {
        name: 0 as _,
        func: 0 as _,
        ml_flags: 0,
        doc: 0 as _,
    }];
    assert!(
        !py_init_module(name.as_ptr(), methods.as_ptr(), desc.as_ptr(), 0 as _, 1007).is_null()
    );
}

#[no_mangle]
pub extern "system" fn initScrapHack() {
    #[cfg(feature = "console")]
    unsafe {
        AllocConsole();
    }
    std::panic::set_hook(Box::new(|info| {
        ceprintln!("ScrapHacks: {info}");
        HWND::DESKTOP
            .MessageBox(&format!("{info}"), "ScrapHacks error", MB::ICONERROR)
            .unwrap();
        std::process::exit(1);
    }));
    init_py_mod();
    print_version_info();
    cprintln!("{SCRAP:#x?}");
}

#[no_mangle]
pub extern "system" fn DllMain(_inst: isize, _reason: u32, _: *const u8) -> u32 {
    1
}

fn print_version_info() {
    cprintln!(
        "{} v{} ({} {}), built for {} by {}.",
        build::PROJECT_NAME,
        build::PKG_VERSION,
        build::SHORT_COMMIT,
        build::BUILD_TIME,
        build::BUILD_TARGET,
        build::RUST_VERSION
    );
}
