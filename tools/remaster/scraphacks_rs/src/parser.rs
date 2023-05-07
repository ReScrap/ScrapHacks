// use crate::{cdbg, ceprintln, cprint, cprintln};
use std::path::PathBuf;
use std::str::FromStr;
use anyhow::{anyhow, Result};
use nom::branch::alt;
use nom::bytes::complete::{take_till, take_while1};
use nom::character::complete::{digit1, hex_digit1};
use nom::character::streaming::char;
use nom::combinator::{eof, opt, rest};
use nom::sequence::{separated_pair, tuple};
use nom::{IResult, Parser};
use nom_locate::LocatedSpan;
use nom_supreme::error::ErrorTree;
use nom_supreme::final_parser::final_parser;
use nom_supreme::tag::complete::{tag, tag_no_case};
use nom_supreme::ParserExt;
use pelite::pattern::{self, Atom};

type Span<'a> = LocatedSpan<&'a str>;

type ParseResult<'a, 'b, T> = IResult<Span<'a>, T, ErrorTree<Span<'b>>>;

#[derive(Debug, Clone)]
pub enum Cmd {
    Imports,
    Read(u32, usize),
    ReadPE(u32, usize),
    Write(u32, Vec<u8>),
    Disams(u32, usize),
    Info(Option<u32>),
    Script(PathBuf),
    Unload,
    ScanModule(Vec<Atom>, Option<String>),
    Lua(String),
}

impl FromStr for Cmd {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match parse(s) {
            Ok(cmd) => Ok(cmd),
            Err(err) => Err(anyhow!("{}", err)),
        }
    }
}

fn ws(input: Span) -> ParseResult<()> {
    take_while1(|c: char| c.is_whitespace())
        .value(())
        .context("Whitepace")
        .parse(input)
}

//

/// Test

fn hex_bytes(input: Span) -> ParseResult<Vec<u8>> {
    hex_digit1
        .map_res_cut(hex::decode)
        .context("Hex string")
        .parse(input)
}

fn num(input: Span) -> ParseResult<usize> {
    digit1
        .map_res_cut(|n: Span| parse_int::parse(&n))
        .context("Number")
        .parse(input)
}

fn address(input: Span) -> ParseResult<u32> {
    tag_no_case("0x")
        .precedes(hex_digit1)
        .recognize()
        .map_res_cut(|addr: Span| parse_int::parse::<u32>(&addr))
        .context("Memory address")
        .parse(input)
}

fn parse_read_pe(input: Span) -> ParseResult<Cmd> {
    tag("read_pe")
        .precedes(ws)
        .precedes(separated_pair(address, ws, num.opt()))
        .map(|(addr, size)| Cmd::ReadPE(addr, size.unwrap_or(0x100)))
        .parse(input)
}

fn parse_read(input: Span) -> ParseResult<Cmd> {
    tag("read")
        .precedes(ws)
        .precedes(separated_pair(address, ws, num.opt()))
        .map(|(addr, size)| Cmd::Read(addr, size.unwrap_or(0x100)))
        .parse(input)
}

fn parse_disasm(input: Span) -> ParseResult<Cmd> {
    tag("disasm")
        .precedes(ws)
        .precedes(separated_pair(address, ws, num.opt()))
        .map(|(addr, size)| Cmd::Disams(addr, size.unwrap_or(50)))
        .parse(input)
}

fn parse_write(input: Span) -> ParseResult<Cmd> {
    tag("write")
        .precedes(ws)
        .precedes(separated_pair(address, ws, hex_bytes))
        .map(|(addr, data)| Cmd::Write(addr, data))
        .parse(input)
}

fn parse_info(input: Span) -> ParseResult<Cmd> {
    tag("info")
        .precedes(eof)
        .value(Cmd::Info(None))
        .or(tag("info")
            .precedes(ws)
            .precedes(address)
            .map(|addr| Cmd::Info(Some(addr))))
        .parse(input)
}

fn parse_scan(input: Span) -> ParseResult<Cmd> {
    let (input, _) = tag("scan").parse(input)?;
    let (input, module) =
        opt(tuple((char(':'), take_till(|c: char| c.is_whitespace())))).parse(input)?;
    let module = module.map(|(_, module)| module.fragment().to_string());
    let (input, _) = ws.parse(input)?;
    let (input, pattern) = rest
        .map_res(|pat: Span| pattern::parse(&pat))
        .parse(input)?;
    Ok((input, Cmd::ScanModule(pattern, module)))
}

fn parse_unload(input: Span) -> ParseResult<Cmd> {
    tag("unload").value(Cmd::Unload).parse(input)
}

fn parse_imports(input: Span) -> ParseResult<Cmd> {
    tag("imports").value(Cmd::Imports).parse(input)
}

fn parse_lua(input: Span) -> ParseResult<Cmd> {
    tag("lua")
        .precedes(ws)
        .precedes(rest)
        .map(|s| Cmd::Lua(s.fragment().to_string()))
        .parse(input)
}

fn parse_script(input: Span) -> ParseResult<Cmd> {
    tag("script")
        .precedes(ws)
        .precedes(rest)
        .map(|s| Cmd::Script(PathBuf::from(s.fragment())))
        .parse(input)
}

fn parse(input: &str) -> Result<Cmd, ErrorTree<Span<'_>>> {
    final_parser(
        alt((
            parse_imports,
            parse_unload,
            parse_scan,
            parse_info,
            parse_write,
            parse_read,
            parse_read_pe,
            parse_script,
            parse_disasm,
            parse_lua,
        ))
        .context("command"),
    )(Span::new(input))
}
