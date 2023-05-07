#![allow(clippy::upper_case_acronyms, non_camel_case_types)]
use anyhow::{anyhow, bail, Result};
use binrw::args;
use binrw::prelude::*;
use binrw::until_exclusive;
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use clap::Subcommand;
use configparser::ini::Ini;
use flate2::write::GzEncoder;
use flate2::Compression;
use fs_err as fs;
use indexmap::IndexMap;
use modular_bitfield::bitfield;
use modular_bitfield::specifiers::B2;
use modular_bitfield::specifiers::B4;
use modular_bitfield::BitfieldSpecifier;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek};
use std::path::Path;
use std::path::PathBuf;
use walkdir::WalkDir;

mod find_scrap;

type IniData = IndexMap<String, IndexMap<String, Option<String>>>;

#[binread]
#[derive(Serialize, Debug)]
struct PackedFile{
    path: PascalString,
    size: u32,
    offset: u32
}

#[binread]
#[br(magic = b"BFPK")]
#[derive(Serialize, Debug)]
struct PackedHeader {
    #[br(temp,assert(version==0))]
    version: u32,
    #[br(temp)]
    num_files: u32,
    #[br(count=num_files)]
    files: Vec<PackedFile>,
}

#[binread]
#[derive(Serialize, Debug)]
#[br(import(msg: &'static str))]
struct Unparsed<const SIZE: u64> {
    #[br(count=SIZE, try_map=|data: Vec<u8>| Err(anyhow!("Unparsed data: {}\n{}", msg, rhexdump::hexdump(&data))))]
    data: (),
}

#[binread]
#[derive(Serialize, Debug)]
struct RawTable<const SIZE: u32> {
    num_entries: u32,
    #[br(assert(entry_size==SIZE))]
    entry_size: u32,
    #[br(count=num_entries, args {inner: args!{count: entry_size.try_into().unwrap()}})]
    data: Vec<Vec<u8>>,
}

#[binread]
#[derive(Serialize, Debug)]
struct Table<T: for<'a> BinRead<Args<'a> = ()> + 'static> {
    num_entries: u32,
    entry_size: u32,
    #[br(count=num_entries)]
    data: Vec<T>,
}

// impl<T: for<'a> BinRead<Args<'a> = ()>> Serialize for Table<T> where T: Serialize {
//     fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
//     where
//         S: serde::Serializer {
//         self.data.serialize(serializer)
//     }
// }

#[binread]
struct Optional<T: for<'a> BinRead<Args<'a> = ()>> {
    #[br(temp)]
    has_value: u32,
    #[br(if(has_value!=0))]
    value: Option<T>,
}

impl<T: for<'a> BinRead<Args<'a> = ()> + Debug> Debug for Optional<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<T: for<'a> BinRead<Args<'a> = ()> + std::ops::Deref> std::ops::Deref for Optional<T> {
    type Target = Option<T>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: for<'a> BinRead<Args<'a> = ()> + Serialize> Serialize for Optional<T> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(serializer)
    }
}

#[binread]
#[derive(Serialize, Debug)]
struct Chunk {
    #[br(map=|c:[u8;4]| c.into_iter().map(|v| v as char).collect())]
    magic: Vec<char>,
    size: u32,
    #[br(temp,count=size)]
    data: Vec<u8>,
}

#[binread]
struct PascalString {
    #[br(temp)]
    length: u32,
    #[br(count=length, map=|bytes: Vec<u8>| {
        String::from_utf8_lossy(&bytes.iter().copied().take_while(|&v| v!=0).collect::<Vec<u8>>()).into_owned()
    })]
    string: String,
}

impl Serialize for PascalString {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.string.serialize(serializer)
    }
}

impl Debug for PascalString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.string.fmt(f)
    }
}

#[binread]
#[derive(Debug, Serialize)]
struct IniSection {
    #[br(temp)]
    num_lines: u32,
    #[br(count=num_lines)]
    sections: Vec<PascalString>,
}

#[binread]
#[br(magic = b"INI\0")]
#[derive(Debug)]
struct INI {
    #[br(temp)]
    size: u32,
    #[br(temp)]
    num_sections: u32,
    #[br(count=num_sections)]
    sections: Vec<IniSection>,
}

impl Serialize for INI {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::Error;
        let blocks: Vec<String> = self
            .sections
            .iter()
            .flat_map(|s| s.sections.iter())
            .map(|s| s.string.clone())
            .collect();
        Ini::new()
            .read(blocks.join("\n"))
            .map_err(Error::custom)?
            .serialize(serializer)
    }
}

#[binread]
#[derive(Debug, Serialize, Clone)]
struct RGBA {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[binread]
#[derive(Debug, Serialize, Clone)]
#[br(import(n_dims: usize))]
struct TexCoords(#[br(count=n_dims)] Vec<f32>);

#[binread]
#[derive(Debug, Serialize, Clone)]
#[br(import(vert_fmt: FVF))]
// https://github.com/elishacloud/dxwrapper/blob/23ffb74c4c93c4c760bb5f1de347a0b039897210/ddraw/IDirect3DDeviceX.cpp#L2642
struct Vertex {
    xyz: [f32; 3],
    // #[br(if(vert_fmt.pos()==Pos::XYZRHW))] // seems to be unused
    // rhw: Option<f32>,
    #[br(if(vert_fmt.normal()))]
    normal: Option<[f32; 3]>,
    #[br(if(vert_fmt.point_size()))]
    point_size: Option<[f32; 3]>,
    #[br(if(vert_fmt.diffuse()))]
    diffuse: Option<RGBA>,
    #[br(if(vert_fmt.specular()))]
    specular: Option<RGBA>,
    #[br(if(vert_fmt.tex_count()>=1), args (vert_fmt.tex_dims(0),))]
    tex_1: Option<TexCoords>,
    #[br(if(vert_fmt.tex_count()>=2), args (vert_fmt.tex_dims(1),))]
    tex_2: Option<TexCoords>,
    #[br(if(vert_fmt.tex_count()>=3), args (vert_fmt.tex_dims(2),))]
    tex_3: Option<TexCoords>,
    #[br(if(vert_fmt.tex_count()>=4), args (vert_fmt.tex_dims(3),))]
    tex_4: Option<TexCoords>,
    #[br(if(vert_fmt.tex_count()>=5), args (vert_fmt.tex_dims(4),))]
    tex_5: Option<TexCoords>,
    #[br(if(vert_fmt.tex_count()>=6), args (vert_fmt.tex_dims(5),))]
    tex_6: Option<TexCoords>,
    #[br(if(vert_fmt.tex_count()>=7), args (vert_fmt.tex_dims(6),))]
    tex_7: Option<TexCoords>,
    #[br(if(vert_fmt.tex_count()>=8), args (vert_fmt.tex_dims(7),))]
    tex_8: Option<TexCoords>,
}

#[derive(Debug, Serialize, PartialEq, Eq, BitfieldSpecifier)]
#[bits = 3]
enum Pos {
    XYZ,
    XYZRHW,
    XYZB1,
    XYZB2,
    XYZB3,
    XYZB4,
    XYZB5,
}

#[bitfield]
#[repr(u32)]
#[derive(Debug, Serialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FVF {
    reserved: bool,
    pos: Pos,
    normal: bool,
    point_size: bool,
    diffuse: bool,
    specular: bool,
    tex_count: B4,
    tex_1: B2,
    tex_2: B2,
    tex_3: B2,
    tex_4: B2,
    tex_5: B2,
    tex_6: B2,
    tex_7: B2,
    tex_8: B2,
    rest: B4,
}

impl FVF {
    fn tex_dims(&self, tex: u8) -> usize {
        let tex: u8 = match tex {
            0 => self.tex_1(),
            1 => self.tex_2(),
            2 => self.tex_3(),
            3 => self.tex_4(),
            4 => self.tex_5(),
            5 => self.tex_6(),
            6 => self.tex_7(),
            7 => self.tex_8(),
            _ => unreachable!(),
        };
        match tex {
            0 => 2,
            1 => 3,
            2 => 4,
            3 => 1,
            _ => unreachable!(),
        }
    }

    // fn num_w(&self) -> usize {
    //     use Pos::*;
    //     match self.pos() {
    //         XYZ | XYZRHW => 0,
    //         XYZB1 => 1,
    //         XYZB2 => 2,
    //         XYZB3 => 3,
    //         XYZB4 => 4,
    //         XYZB5 => 5,
    //     }
    // }
}

fn vertex_size_from_id(fmt_id: u32) -> Result<u32> {
    let fmt_size = match fmt_id {
        0 => 0x0,
        1 | 8 | 10 => 0x20,
        2 => 0x28,
        3 | 0xd => 0x1c,
        4 | 7 => 0x24,
        5 => 0x2c,
        6 => 0x34,
        0xb => 4,
        0xc => 0x18,
        0xe => 0x12,
        0xf | 0x10 => 0x16,
        0x11 => 0x1a,
        other => bail!("Invalid vertex format id: {other}"),
    };
    Ok(fmt_size)
}

fn vertex_format_from_id(fmt_id: u32, fmt: u32) -> Result<FVF> {
    let fvf = match fmt_id {
        0 => 0x0,
        1 => 0x112,
        2 => 0x212,
        3 => 0x1c2,
        4 => 0x116,
        5 => 0x252,
        6 => 0x352,
        7 => 0x152,
        8 => 0x1c4,
        10 => 0x242,
        other => bail!("Invalid vertex format id: {other}"),
    };
    if fvf != fmt {
        bail!("Vertex format mismatch: {fvf}!={fmt}");
    }
    Ok(FVF::from(fvf))
}

#[binread]
#[br(import(fmt_id: u32))]
#[derive(Debug, Serialize, Clone)]
struct LFVFInner {
    #[br(try_map=|v:  u32| vertex_format_from_id(fmt_id,v))]
    vert_fmt: FVF,
    #[br(assert(vert_size==vertex_size_from_id(fmt_id).unwrap()))]
    vert_size: u32,
    num_verts: u32,
    #[br(count=num_verts, args {inner: (vert_fmt,)})]
    data: Vec<Vertex>,
}

#[binread]
#[br(magic = b"LFVF")]
#[derive(Debug, Serialize)]
struct LFVF {
    size: u32,
    #[br(assert(version==1,"invalid LFVF version"))]
    version: u32,
    #[br(assert((0..=0x11).contains(&fmt_id),"invalid LFVF format_id"))]
    fmt_id: u32,
    #[br(if(fmt_id!=0),args(fmt_id))]
    inner: Option<LFVFInner>,
}

#[binread]
#[br(magic = b"MD3D")]
#[derive(Debug, Serialize)]
struct MD3D {
    // TODO: mesh
    size: u32,
    #[br(assert(version==1,"Invalid MD3D version"))]
    version: u32,
    name: PascalString,
    num_tris: u32,
    #[br(assert(tri_size==6,"Invalid MD3D tri size"))]
    tri_size: u32,
    #[br(count=num_tris)]
    tris: Vec<[u16; 3]>,
    mesh_data: LFVF,
    unk_table_1: RawTable<2>,
    rest: Unparsed<0x100>
    // TODO:
    // ==
    // unk_t1_count: u32,
    // #[br(assert(unk_t1_size==2))]
    // unk_t1_size: u32,
    // #[br(count=unk_t1_count)]
    // unk_t1_list: Vec<u16>,
    // // ==
    // unk_t2_count: u32,
    // #[br(assert(unk_t1_size==2))]
    // unk_t2_size: u32,
    // #[br(count=unk_t1_count)]
    // unk_t2_list: Vec<u16>,
}

#[binread]
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum NodeData {
    #[br(magic = 0x0u32)]
    Null,
    #[br(magic = 0xa1_00_00_01_u32)]
    TriangleMesh(Unparsed<0x10>), // TODO: Empty or unused?
    #[br(magic = 0xa1_00_00_02_u32)]
    Mesh(MD3D),
    #[br(magic = 0xa2_00_00_04_u32)]
    Camera(CAM),
    #[br(magic = 0xa3_00_00_08_u32)]
    Light(LUZ),
    #[br(magic = 0xa4_00_00_10_u32)]
    Ground(SUEL),
    #[br(magic = 0xa5_00_00_20_u32)]
    SistPart(Unparsed<0x10>), // TODO: Particles
    #[br(magic = 0xa6_00_00_40_u32)]
    Graphic3D(SPR3),
    #[br(magic = 0xa6_00_00_80_u32)]
    Flare(Unparsed<0x10>), // TODO: LensFlare?
    #[br(magic = 0xa7_00_01_00u32)]
    Portal(PORT),
}

#[binread]
#[br(magic = b"SPR3")]
#[derive(Debug, Serialize)]
struct SPR3 {
    size: u32,
    #[br(assert(version==1,"Invalid SPR3 version"))]
    version: u32,
    pos: [f32; 3],
    unk_1: [u8; 8],
    name_1: PascalString,
    name_2: PascalString,
    unk_2: u32,
}

#[binread]
#[br(magic = b"SUEL")]
#[derive(Debug, Serialize)]
struct SUEL {
    size: u32,
    #[br(assert(version==1,"Invalid SUEL version"))]
    version: u32,
    bbox: [[f32; 3]; 2],
    pos: [f32; 3],
    unk_3: [u8; 4],
    num_nodes: u32,
    unk_4: [u8; 4],
    bbox_2: [[f32; 3]; 2],
}

#[binread]
#[br(magic = b"CAM\0")]
#[derive(Debug, Serialize)]
struct CAM {
    size: u32,
    #[br(assert(version==1,"Invalid CAM version"))]
    version: u32,
    unk_1: [f32; 3],
    origin: [f32; 3],
    destination: [f32; 3],
    unk_4: [u8; 4],
    unk_5: [u8; 4],
    unk_6: [u8; 4],
    unk_7: [u8; 4],
    unk_8: [u8; 4],
    unk_9: [u8; 4],
    unk_10: [u8; 4],
    unk_11: [u8; 4],
}

#[binread]
#[br(magic = b"LUZ\0")]
#[derive(Debug, Serialize)]
struct LUZ {
    size: u32,
    #[br(assert(version==1,"Invalid LUZ version"))]
    version: u32,
    col: u32,
    brightness: u32,
    unk_3: u8,
    pos: [f32; 3],
    rot: [f32; 3],
    unk_6: [u8; 8],
    unk_7: [u8; 4],
    unk_8: [u8; 4],
    unk_9: [u8; 4],
    unk_10: [u8; 4],
    unk_11: [u8; 4],
    unk_12: [u8; 4],
    unk_13: u32,
}

#[binread]
#[br(magic = b"PORT")]
#[derive(Debug, Serialize)]
struct PORT {
    size: u32,
    #[br(assert(version==1,"Invalid PORT version"))]
    version: u32,
    width: u32,
    height: u32,
    sides: [u32; 2],
}

#[binread]
#[derive(Debug, Serialize)]
struct Node {
    unk_f17_0x44: u32,
    unk_f18_0x48: u32,
    unk_f19_0x4c: u32,
    flags: u32,
    unk_f20_0x50: u32,
    name: PascalString,
    parent: PascalString,
    unk_f7_0x1c: [f32; 3],       // 0xc
    unk_f10_0x28: [f32; 4],      // 0x10
    unk_f14_0x38: f32,           // 0x4
    unk_f23_0x5c: [[f32; 4]; 4], // 0x40 4x4 Matrix
    unk_f39_0x9c: [[f32; 4]; 4], // 0x40 4x4 Matrix
    unk_f55_0xdc: [f32; 4],      // 0x10 Vector?
    unk_f59_0xec: [f32; 3],      // 0xc Vector?
    node_info: Optional<INI>,
    content: Optional<NodeData>,
}

#[binread]
#[br(magic = b"MAP\0")]
#[derive(Debug, Serialize)]
struct MAP {
    size: u32,
    #[br(assert((2..=3).contains(&version),"invalid MAP version"))]
    version: u32,
    texture: PascalString,
    unk_1: [u8; 7],
    unk_bbox: [[f32; 2]; 2],
    unk_2: f32,
    #[br(if(version==3))]
    unk_3: Option<[u8; 0xc]>,
}

#[binread]
#[derive(Debug, Serialize)]
struct Textures {
    base: Optional<MAP>,
    metallic: Optional<MAP>,
    unk_1: Optional<MAP>,
    bump: Optional<MAP>,
    glow: Optional<MAP>
}

#[binread]
#[br(magic = b"MAT\0")]
#[derive(Debug, Serialize)]
struct MAT {
    size: u32,
    #[br(assert((1..=3).contains(&version),"invalid MAT version"))]
    version: u32,
    #[br(if(version>1))]
    name: Option<PascalString>,
    unk_f: [RGBA; 7],
    unk_data: [RGBA; 0x18 / 4],
    maps: Textures
}

#[binread]
#[br(magic = b"SCN\0")]
#[derive(Debug, Serialize)]
struct SCN {
    // 0x650220
    size: u32,
    #[br(temp,assert(version==1))]
    version: u32,
    model_name: PascalString,
    node_name: PascalString,
    node_props: Optional<INI>,
    unk_f_1: [f32; (8 + 8) / 4],
    unk_1: [f32; 0x18 / 4],
    unk_f_2: f32,
    user_props: Optional<INI>,
    num_materials: u32,
    #[br(count=num_materials)]
    mat: Vec<MAT>,
    #[br(temp,assert(unk_3==1))]
    unk_3: u32,
    num_nodes: u32,
    #[br(count = 1)] // 32
    nodes: Vec<Node>,
    // ani: Optional<ANI>, // TODO: ?
}

fn convert_timestamp(dt: u32) -> Result<DateTime<Utc>> {
    let Some(dt) = NaiveDateTime::from_timestamp_opt(dt.into(),0) else {
        bail!("Invalid timestamp");
    };
    Ok(DateTime::from_utc(dt, Utc))
}

#[binread]
#[derive(Debug, Serialize)]
struct VertexAnim {
    n_tr: u32,
    maybe_duration: f32,
    #[br(count=n_tr)]
    tris: Vec<[u8; 3]>,
}

#[binread]
#[br(magic = b"EVA\0")]
#[derive(Debug, Serialize)]
struct EVA {
    size: u32,
    #[br(assert(version==1,"Invalid EVA version"))]
    version: u32,
    num_verts: u32,
    #[br(count=num_verts)]
    verts: Vec<Optional<VertexAnim>>,
}

#[binread]
#[br(magic = b"NAM\0")]
#[derive(Debug, Serialize)]
struct NAM {
    size: u32,
    #[br(assert(version==1))]
    version: u32,
    primer_frames: u32,
    frames: u32,
    #[br(assert(flags&0xffffef60==0,"Invalid NAM flags"))]
    flags: u32,
    #[br(assert(opt_flags&0xfff8==0,"Invalid NAM opt_flags"))]
    opt_flags: u32,
    #[br(assert(stm_flags&0xfff8==0,"Invalid NAM stm_flags"))]
    stm_flags: u32,
    #[br(map=|_:()| flags&(opt_flags|0x8000)&stm_flags)]
    combined_flags: u32,
    #[br(if(combined_flags&0x1!=0))]
    unk_flags_1: Option<u32>,
    #[br(if(combined_flags&0x2!=0))]
    unk_flags_2: Option<u32>,
    #[br(if(combined_flags&0x4!=0))]
    unk_flags_3: Option<u32>,
    #[br(if(combined_flags&0x8!=0))]
    unk_flags_4: Option<u32>,
    #[br(if(combined_flags&0x10!=0))]
    unk_flags_5: Option<u32>,
    #[br(if(combined_flags&0x80!=0))]
    unk_flags_6: Option<u32>,
    #[br(if(flags&0x1000!=0))]
    eva: Option<EVA>,
}

#[binread]
#[br(magic = b"NABK")]
#[derive(Debug, Serialize)]
struct NABK {
    size: u32,
    #[br(temp,count=size)]
    data: Vec<u8>,
}

#[binread]
#[br(magic = b"ANI\0")]
#[derive(Debug, Serialize)]
struct ANI {
    size: u32,
    #[br(assert(version==2, "Invalid ANI version"))]
    version: u32,
    fps: f32,
    unk_1: u32,
    unk_2: u32,
    num_objects: u32,
    unk_flags: u32,
    num: u32,
    #[br(temp,count=num)]
    data: Vec<u8>,
    nabk: NABK,
    #[br(count=num_objects)]
    nam: Vec<NAM>,
}

#[binread]
#[br(magic = b"SM3\0")]
#[derive(Debug, Serialize)]
struct SM3 {
    size: u32,
    #[br(temp,assert(const_1==0x6515f8,"Invalid timestamp"))]
    const_1: u32,
    #[br(try_map=convert_timestamp)]
    time_1: DateTime<Utc>,
    #[br(try_map=convert_timestamp)]
    time_2: DateTime<Utc>,
    scene: SCN,
}

#[binread]
#[br(magic = b"CM3\0")]
#[derive(Debug, Serialize)]
struct CM3 {
    size: u32,
    #[br(temp,assert(const_1==0x6515f8,"Invalid timestamp"))]
    const_1: u32,
    #[br(try_map=convert_timestamp)]
    time_1: DateTime<Utc>,
    #[br(try_map=convert_timestamp)]
    time_2: DateTime<Utc>,
    scene: SCN,
}

#[binread]
#[derive(Debug, Serialize)]
struct Dummy {
    has_next: u32,
    name: PascalString,
    pos: [f32; 3],
    rot: [f32; 3],
    info: Optional<INI>,
}

#[binread]
#[br(magic = b"DUM\0")]
#[derive(Debug, Serialize)]
struct DUM {
    size: u32,
    #[br(assert(version==1, "Invalid DUM version"))]
    version: u32,
    num_dummies: u32,
    #[br(count=num_dummies)]
    dummies: Vec<Dummy>,
}

#[binread]
#[br(magic = b"QUAD")]
#[derive(Debug, Serialize)]
struct QUAD {
    size: u32,
    #[br(assert(version==1, "Invalid QUAD version"))]
    version: u32,
    mesh: u32,
    table: Table<u16>,
    f_4: [f32; 4],
    num_children: u32,
    #[br(count=num_children)]
    children: Vec<QUAD>,
}

#[binread]
#[br(magic = b"CMSH")]
#[derive(Debug, Serialize)]
struct CMSH {
    size: u32,
    #[br(assert(version==2, "Invalid CMSH version"))]
    version: u32,
    #[br(assert(collide_mesh_size==0x34, "Invalid collision mesh size"))]
    collide_mesh_size: u32,
    name: PascalString,
    unk_1: u16,
    sector: u16,
    unk_2: u16,
    index: u8,
    unk_4: u8,
    bbox_1: [[f32; 3]; 2],
    #[br(temp)]
    t_1: Table<[f32; 3]>,
    #[br(temp)]
    t_2: RawTable<0x1c>,
}

#[binread]
#[br(magic = b"AMC\0")]
#[derive(Debug, Serialize)]
struct AMC {
    size: u32,
    #[br(assert(version==100,"Invalid AMC version"))]
    version: u32,
    #[br(assert(version_code==0, "Invalid AMC version_code"))]
    version_code: u32,
    bbox_1: [[f32; 3]; 2],
    scale: f32,
    bbox_2: [[f32; 3]; 2],
    unk: [f32; 3],
    cmsh: [CMSH; 2],
    num_sectors: u32,
    #[br(count=num_sectors)]
    sector_col: Vec<[CMSH; 2]>,
    unk_num_1: u32,
    unk_num_2: u32,
    unk_f: [f32; 4],
    num_quads: u32,
    #[br(count=num_quads)]
    quads: Vec<QUAD>,
}

#[binread]
#[br(import(version: u32))]
#[derive(Debug, Serialize)]
struct TriV104 {
    #[br(if(version>=0x69))]
    name_2: Option<PascalString>,
    mat_key: u32,
    map_key: u32,
    num_tris: u32,
    #[br(count=num_tris)]
    tris: Vec<[u16; 3]>,
    verts_1: LFVF,
    verts_2: LFVF,
}

#[binread]
#[br(magic = b"TRI\0", import(version: u32))]
#[derive(Debug, Serialize)]
struct TRI {
    size: u32,
    unk_int: u32,
    name: PascalString,
    unk_int_2: u32, // if 0xffffffff sometimes TriV104 has no name_2 field
    #[br(args(version))]
    data: TriV104,
}

#[binread]
#[derive(Debug, Serialize)]
struct EMI_Textures {
    key: u32,
    #[br(if(key!=0))]
    data: Option<(PascalString, u32, PascalString)>,
}

#[binread]
#[br(magic = b"EMI\0")]
#[derive(Debug, Serialize)]
struct EMI {
    size: u32,
    #[br(assert((103..=105).contains(&version)))]
    version: u32,
    num_materials: u32,
    #[br(count=num_materials)]
    materials: Vec<(u32, MAT)>,
    #[br(parse_with = until_exclusive(|v: &EMI_Textures| v.key==0))]
    maps: Vec<EMI_Textures>,
    num_lists: u32,
    #[br(count=num_lists,args{inner: (version,)})]
    tri: Vec<TRI>,
}

#[binread]
#[derive(Debug, Serialize)]
enum Data {
    SM3(SM3),
    CM3(CM3),
    DUM(DUM),
    AMC(AMC),
    EMI(EMI),
}

fn parse_file(path: &PathBuf) -> Result<Data> {
    let mut rest_size = 0;
    let mut fh = BufReader::new(fs::File::open(path)?);
    let ret = fh.read_le()?;
    let pos = fh
        .stream_position()
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    eprintln!("Read {} bytes from {}", pos, path.display());
    let mut buffer = [0u8; 0x1000];
    if let Ok(n) = fh.read(&mut buffer) {
        if n != 0 {
            eprintln!("Rest:\n{}", rhexdump::hexdump_offset(&buffer[..n], pos));
        }
    };
    while let Ok(n) = fh.read(&mut buffer) {
        if n == 0 {
            break;
        }
        rest_size += n;
    }
    eprintln!("+{rest_size} unparsed bytes");
    Ok(ret)
}

fn load_ini(path: &PathBuf) -> IniData {
    Ini::new().load(path).unwrap_or_default()
}

#[derive(Serialize, Debug)]

struct Level {
    config: IniData,
    moredummies: IniData,
    emi: EMI,
    sm3: SM3,
    dummies: DUM,
    path: PathBuf,
    root: PathBuf,
}

impl Level {
    fn load(root: &Path, path: &Path) -> Result<Self> {
        let full_path = &root.join(path);
        let emi_path = full_path.join("map").join("map3d.emi");
        let sm3_path = emi_path.with_extension("sm3");
        let dum_path = emi_path.with_extension("dum");
        let config_file = emi_path.with_extension("ini");
        let moredummies = emi_path.with_file_name("moredummies").with_extension("ini");
        let config = load_ini(&config_file);
        let moredummies = load_ini(&moredummies);
        let Data::EMI(emi) = parse_file(&emi_path)? else {
            bail!("Failed to parse EMI at {emi_path}", emi_path=emi_path.display());
        };
        let Data::SM3(sm3) = parse_file(&sm3_path)? else {
            bail!("Failed to parse SM3 at {sm3_path}", sm3_path=sm3_path.display());
        };
        let Data::DUM(dummies) = parse_file(&dum_path)? else {
            bail!("Failed to parse DUM at {dum_path}", dum_path=dum_path.display());
        };
        Ok(Level {
            config,
            moredummies,
            emi,
            sm3,
            dummies,
            path: path.into(),
            root: root.into(),
        })
    }
}

#[derive(Subcommand, Debug)]
enum Commands {
    FindScrapland,
    ParsePacked {
        scrap_path: PathBuf,
    },
    ParseFile {
        #[clap(long)]
        /// Write to stdout
        stdout: bool,
        /// Scrapland root path
        root: PathBuf,
        /// Level to parse and convert
        level: PathBuf,
    },
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Args {
    #[arg(long,short)]
    /// Write data as JSON
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

fn cmd_parse_packed(root: &Path) -> Result<HashMap<PathBuf, Vec<PackedFile>>> {
    let mut packed_map = HashMap::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path
            .extension()
            .map(|e| e.to_str() == Some("packed"))
            .unwrap_or(false)
        {
            let path = entry.path().to_owned();
            let header: PackedHeader = BufReader::new(File::open(&path)?).read_le()?;
            packed_map.insert(path, header.files);
        }
    }
    Ok(packed_map)
}

fn to_bytes<T>(data: &T, json: bool) -> Result<Vec<u8>> where T: Serialize {
    if json {
        Ok(serde_json::to_vec_pretty(data)?)
    } else {
        Ok(serde_pickle::to_vec(data,Default::default())?)
    }
}

fn cmd_parse_file(stdout: bool, root: &Path, path: &Path, json: bool) -> Result<()> {
    let out_path = PathBuf::from(
        path
            .components()
            .last()
            .unwrap()
            .as_os_str()
            .to_string_lossy()
            .into_owned(),
    );
    let out_path = if json {
        out_path.with_extension("json.gz")
    } else {
        out_path.with_extension("pkl.gz")
    };
    let full_path = &root.join(path);
    let data = if full_path.is_dir() {
        let level = Level::load(root, path)?;
        to_bytes(&level,json)?
    } else {
        let data = parse_file(full_path)?;
        to_bytes(&data,json)?
    };
    let mut data = Cursor::new(data);
    if stdout {
        let mut stdout = std::io::stdout().lock();
        std::io::copy(&mut data, &mut stdout)?;
    } else {
        let mut fh = GzEncoder::new(File::create(&out_path)?, Compression::best());
        std::io::copy(&mut data, &mut fh)?;
        eprintln!("Wrote {path}", path = out_path.display());
    };
    Ok(())
}

fn emi_to_obj(emi: EMI) -> ! {
    // let mut obj_data = obj::ObjData::default();
    
    // for mesh in emi.tri {
    //     for vert in mesh.data.verts_1.inner.map(|d| d.data).unwrap_or_default() {
    //         obj_data.position.push(vert.xyz);
    //         obj_data.normal.push(vert.normal.unwrap_or_default());
    //         obj_data.texture.push(vert.tex_1.unwrap_or_default().0.try_into().unwrap());
    //     }
    //     for vert in mesh.data.verts_2.inner.map(|d| d.data).unwrap_or_default() {
    //         obj_data.position.push(vert.xyz);
    //         obj_data.normal.push(vert.normal.unwrap_or_default());
    //     }
    // }
    todo!("EMI to OBJ converter");
}

fn main() -> Result<()> {
    let args = Args::try_parse()?;
    match args.command {
        Commands::FindScrapland => {
            let data = to_bytes(&find_scrap::get_executable()?,args.json)?;
            let mut stdout = std::io::stdout().lock();
            std::io::copy(&mut &data[..], &mut stdout)?;
        }
        Commands::ParsePacked { scrap_path } => {
            let data = to_bytes(&cmd_parse_packed(&scrap_path)?,args.json)?;
            let mut stdout = std::io::stdout().lock();
            std::io::copy(&mut &data[..], &mut stdout)?;
        }
        Commands::ParseFile {
            stdout,
            root,
            level,
        } => {
            cmd_parse_file(stdout, &root, &level, args.json)?;
        }
    }
    Ok(())
}
