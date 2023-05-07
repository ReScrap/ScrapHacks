use std::{path::PathBuf, sync::Arc};

use crate::{
    cprintln,
    mem::{get_module, get_modules},
    parser::Cmd,
};
use anyhow::{anyhow, bail, Result};
use detour3::GenericDetour;
use mlua::{prelude::*, Variadic};
use pelite::pattern;
use pelite::pe32::{Pe, PeObject};
use rustc_hash::FxHashMap;
use winsafe::{prelude::*, HINSTANCE};

struct Ptr(*const ());

impl LuaUserData for Ptr {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_meta_method(LuaMetaMethod::ToString, |_, this, _: ()| {
            Ok(format!("{:p}", this.0))
        });
        methods.add_method("read", |_, this, (size,): (usize,)| {
            let addr = this.0 as u32;
            let ptr = this.0 as *const u8;
            let info = region::query(ptr).map_err(mlua::Error::external)?;
            let end = info.as_ptr_range::<()>().end as u32;
            let size = ((end - addr) as usize).min(size);
            if !info.is_readable() {
                return Err(LuaError::external(anyhow!("No read permission on page")));
            }
            let data = unsafe { std::slice::from_raw_parts(ptr, size) };
            Ok(data.to_vec())
        });
        methods.add_method("write", |_, this, data: Vec<u8>| {
            let data = data.as_slice();
            let addr = this.0 as *const u8;
            unsafe {
                let handle = region::protect_with_handle(
                    addr,
                    data.len(),
                    region::Protection::READ_WRITE_EXECUTE,
                )
                .map_err(mlua::Error::external)?;
                std::ptr::copy(data.as_ptr(), addr as *mut u8, data.len());
                drop(handle);
            };
            Ok(())
        });
        // methods.add_method("hook", |_, this, func: LuaFunction| -> LuaResult<()> {
        //     let addr = this.0;
        //     cprintln!("Hook: {func:?} @ {addr:p}");
        //     let dt = unsafe { GenericDetour::<extern "thiscall" fn(*const (), (u32,u32,u32)) -> u32>::new(std::mem::transmute(addr), hook_func) }.unwrap();
        //     Err(LuaError::external(anyhow!("TODO: hook")))
        // });
    }
}

// extern "thiscall" fn hook_func(this: *const (), args: (u32,u32,u32)) -> u32 {
//     return 0;
// }

pub(crate) fn init() -> Result<Lua> {
    let lua = unsafe { Lua::unsafe_new() };
    {
        let globals = lua.globals();
        globals.set("scan", lua.create_function(lua_scan)?)?;
        globals.set("print", lua.create_function(lua_print)?)?;
        globals.set("hook", lua.create_function(lua_hook)?)?;
        globals.set("imports", lua.create_function(lua_imports)?)?;
        globals.set(
            "ptr",
            lua.create_function(|_, addr: u32| Ok(Ptr(addr as _)))?,
        )?;
        globals.set(
            "ptr",
            lua.create_function(lua_alloc)?,
        )?;
    }
    Ok(lua)
}

fn lua_val_to_string(val: &LuaValue) -> LuaResult<String> {
    Ok(match val {
        LuaNil => "Nil".to_owned(),
        LuaValue::Boolean(b) => format!("{b}"),
        LuaValue::LightUserData(u) => format!("{u:?}"),
        LuaValue::Integer(i) => format!("{i}"),
        LuaValue::Number(n) => format!("{n}"),
        LuaValue::String(s) => (s.to_str()?).to_string(),
        LuaValue::Table(t) => {
            let mut vals = vec![];
            for res in t.clone().pairs() {
                let (k, v): (LuaValue, LuaValue) = res?;
                vals.push(format!(
                    "{k} = {v}",
                    k = lua_val_to_string(&k)?,
                    v = lua_val_to_string(&v)?
                ));
            }
            format!("{{{vals}}}", vals = vals.join(", "))
        }
        LuaValue::Function(f) => format!("{f:?}"),
        LuaValue::Thread(t) => format!("{t:?}"),
        LuaValue::UserData(u) => format!("{u:?}"),
        LuaValue::Error(e) => format!("{e:?}"),
    })
}

fn lua_alloc(lua: &Lua, size: usize) -> LuaResult<Ptr> {
    let data = vec![0u8;size].into_boxed_slice();
    Ok(Ptr(Box::leak(data).as_ptr() as _))
}

fn lua_hook(lua: &Lua, (addr, func): (u32, LuaFunction)) -> LuaResult<()> {
    cprintln!("Hook: {func:?} @ {addr:08x}");
    Err(LuaError::external(anyhow!("TODO: hook")))
}

fn lua_imports(lua: &Lua, (): ()) -> LuaResult<()> {
    Err(LuaError::external(anyhow!("TODO: imports")))
}

fn lua_print(lua: &Lua, args: Variadic<LuaValue>) -> LuaResult<()> {
    let msg: Vec<String> = args
        .into_iter()
        .map(|v| lua_val_to_string(&v))
        .collect::<LuaResult<Vec<String>>>()?;
    cprintln!("{}", msg.join(" "));
    Ok(())
}

#[derive(Debug)]
enum ScanMode {
    MainModule,
    Modules(Vec<String>),
    All,
}

impl FromLua<'_> for ScanMode {
    fn from_lua<'lua>(lua_value: LuaValue<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
        match &lua_value {
            LuaValue::Nil => Ok(ScanMode::MainModule),
            LuaValue::Boolean(true) => Ok(ScanMode::All),
            LuaValue::Table(t) => Ok(ScanMode::Modules(FromLua::from_lua(lua_value, lua)?)),
            _ => Err(LuaError::FromLuaConversionError {
                from: lua_value.type_name(),
                to: "scan_mode",
                message: None,
            }),
        }
    }
}

fn lua_scan(lua: &Lua, (pattern, scan_mode): (String, ScanMode)) -> LuaResult<LuaTable> {
    let pat = pattern::parse(&pattern).map_err(mlua::Error::external)?;
    let mut ret = FxHashMap::default();
    let modules = match scan_mode {
        ScanMode::MainModule => vec![get_module(None).map_err(mlua::Error::external)?],
        ScanMode::Modules(modules) => modules
            .into_iter()
            .map(|m| get_module(Some(m)))
            .collect::<Result<_>>()
            .map_err(mlua::Error::external)?,
        ScanMode::All => get_modules().map_err(mlua::Error::external)?,
    };
    'outer: for module in modules {
        let regions = region::query_range(module.image().as_ptr(), module.image().len())
            .map_err(mlua::Error::external)?;
        for region in regions {
            let Ok(region)=region else {
                continue 'outer;
            };
            if !region.is_readable() {
                continue 'outer;
            }
        }
        let h_module = unsafe { HINSTANCE::from_ptr(module.image().as_ptr() as _) };
        let module_name = PathBuf::from(
            h_module
                .GetModuleFileName()
                .map_err(mlua::Error::external)?,
        )
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
        if let Ok(res) = crate::mem::scan(&pat, &module) {
            if !res.is_empty() {
                let res: Vec<Vec<Ptr>> = res
                    .into_iter()
                    .map(|res| res.into_iter().map(|a| Ptr(a as _)).collect())
                    .collect();
                ret.insert(module_name, res);
            }
        };
    }
    lua.create_table_from(ret.into_iter())
}

pub(crate) fn exec(chunk: &str) -> Result<()> {
    Ok(init()?.load(chunk).set_name("ScrapLua")?.exec()?)
}
