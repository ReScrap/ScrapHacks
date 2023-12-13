#![allow(dead_code)]
// https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx9-graphics-reference-asm-ps-1-x
// https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx9-graphics-reference-asm-ps-registers-modifiers-source
// http://archive.gamedev.net/archive/columns/hardcore/dxshader3/page4.html
// r_bias -> r-0.5
// r_x2 -> r*2.0
// r_bx2 -> (r-0.5)*2.0
// [inst]_x(v) -> res*=v
// [inst]_d(v) -> res/=v
// [inst]_sat -> res=clamp(res,0,1)

use std::{convert::Infallible, str::FromStr};

use anyhow::Result;
use fs_err as fs;

#[derive(Debug)]
struct Arg {
    name: String,
    modifiers: Vec<String>,
}

#[derive(Debug)]
struct Cmd {
    name: String,
    modifiers: Vec<String>,
    args: Vec<Arg>,
}

impl FromStr for Cmd {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let line: Vec<&str> = s
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .map(|c| c.trim_end_matches(',').trim_start_matches('+'))
            .filter(|w| !w.is_empty())
            .collect();
        let (cmd, args) = match line.split_first() {
            Some((&cmd, args)) => (cmd, args),
            None => unreachable!(),
        };
        println!("{line:?} -> {cmd}{args:?}");
        Ok(Cmd {
            name: String::new(),
            modifiers: vec![],
            args: vec![],
        })
    }
}

fn parse(path: &str) -> Result<()> {
    let data = fs::read_to_string(path)?;
    for line in data.lines() {
        let mut line = line.trim().split("//");
        let line = line.next().unwrap_or_default();
        if line.is_empty() || line.starts_with("ps.") {
            continue;
        }

        let cmd: Cmd = line.parse()?;
        dbg!(cmd);
    }
    todo!()
}

#[cfg(test)]
mod test {
    #[test]
    fn test() {
        super::parse(r"E:\Games\Steam\steamapps\common\Scrapland\ext\bmp\glowmapmaskenvbump.psh")
            .unwrap();
        // super::parse(r"E:\Games\Steam\steamapps\common\Scrapland\ext\bmp\hologram.psh").unwrap();
    }
}
