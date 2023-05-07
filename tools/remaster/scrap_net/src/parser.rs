use std::collections::HashMap;
use std::error::Error;

use crate::hex_ii::hex_ii_dump;
use crate::ServerFlags;
use binrw::BinReaderExt;
use binrw::{binread, BinRead, NullString};

/*
00000000: 7f 4c 00 00 06 ba ce 01 01 06 63 61 63 6f 74 61 | .L........cacota
00000010: 10 5b 42 4a 5a 5d 20 45 61 72 74 68 6e 75 6b 65 | .[BJZ].Earthnuke
00000020: 72 06 53 50 6f 6c 69 31 37 00 08 50 5f 50 6f 6c | r.SPoli17..P_Pol
00000030: 69 63 65 06 4d 50 4f 4c 49 31 00 00 00 0d 30 2c | ice.MPOLI1....0,
00000040: 30 2c 30 2c 31 2c 30 2c 30 2c 31 00 00 00 00    | 0,0,1,0,0,1....

00000000: 7f 49 00 00 06 ba ce 01 01 06 63 61 63 6f 74 61 | .I........cacota
00000010: 0e 55 6e 6e 61 6d 65 64 20 50 6c 61 79 65 72 07 | .Unnamed.Player.
00000020: 53 42 65 74 74 79 31 50 00 07 50 5f 42 65 74 74 | SBetty1P..P_Bett
00000030: 79 07 4d 42 65 74 74 79 31 00 00 00 0b 31 2c 31 | y.MBetty1....1,1
00000040: 2c 30 2c 31 2c 33 2c 30 00 00 00 00             | ,0,1,3,0....
*/

#[derive(Debug, Clone, BinRead)]
#[br(big)]
#[br(magic = b"\xba\xce")]
struct ServerInfoJoin {
    #[br(map = |v: (u8,u8)| format!("{}.{}",v.0,v.1))]
    version: String,
}

struct Data {
    player_id: u32,
    num_vals: u32,
    pos: [f32; 3],
    player_index: u32,
    rtt: u32,
}

#[binread]
#[br(big)]
#[derive(Debug, Clone)]
enum PacketData {
    #[br(magic = b"\x7f")]
    PlayerJoin {
        data_len: u8,
        _1: u8,
        cur_players: u8,
        max_players: u8,
        info: ServerInfoJoin,
        #[br(temp)]
        pw_len: u8,
        #[br(count = pw_len, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned())]
        password: String,
        #[br(temp)]
        player_name_len: u8,
        #[br(count = player_name_len, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned())]
        player_name: String,
        #[br(temp)]
        ship_model_len: u8,
        #[br(count = ship_model_len, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned())]
        ship_model: String,
        #[br(little)]
        max_health: u16,
        #[br(temp)]
        pilot_model_len: u8,
        #[br(count = pilot_model_len, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned())]
        pilot_model: String,
        #[br(temp)]
        engine_model_r_len: u8,
        #[br(count = engine_model_r_len, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned())]
        engine_model_r: String,
        #[br(temp)]
        engine_model_l_len: u8,
        #[br(count = engine_model_r_len, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned())]
        engine_model_l: String,
        _2: u16,
        #[br(temp)]
        loadout_len: u8,
        #[br(count = loadout_len, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned())]
        loadout: String,
        team_number: u16,
        padding: [u8; 2],
    },
    #[br(magic = b"\x80\x15")]
    MapInfo {
        #[br(temp)]
        map_len: u32,
        #[br(count = map_len, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned())]
        map: String,
        #[br(temp)]
        mode_len: u8,
        #[br(count = mode_len, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned())]
        mode: String,
        _2: u16,
        item_count: u8,
        // _3: u32,
        // #[br(count = item_count)]
        // items: Vec<[u8;0x11]>
    },
    #[br(magic = b"\xba\xce")]
    ServerInfo {
        #[br(map = |v: (u8,u8)| format!("{}.{}",v.1,v.0))]
        version: String,
        port: u16,
        max_players: u16,
        cur_players: u16,
        #[br(map = u8::into)]
        flags: ServerFlags,
        #[br(pad_size_to(0x20), map=|s: NullString| s.to_string())]
        name: String,
        #[br(pad_size_to(0x10), map=|s: NullString| s.to_string())]
        mode: String,
        #[br(pad_size_to(0x20), map=|s: NullString| s.to_string())]
        map: String,
        _pad: u8,
    },
}

fn parse(data: &[u8]) -> Result<(PacketData, Vec<u8>), Box<dyn Error>> {
    use std::io::Cursor;
    let mut rdr = Cursor::new(data);
    let pkt: PacketData = rdr.read_le()?;
    let rest = data[rdr.position() as usize..].to_vec();
    println!("{}", rhexdump::hexdump(data));
    Ok((pkt, rest))
}

#[test]
fn test_parser() {
    let log = include_str!("../test_.log").lines();
    let mut hm = HashMap::new();
    for line in log {
        let data = line.split_ascii_whitespace().nth(1).unwrap();
        let data = hex::decode(data).unwrap();
        *hm.entry(data[0..1].to_vec()).or_insert(0usize) += 1;
        match parse(&data) {
            Ok((pkt, rest)) => {
                println!("{:#x?}", pkt);
            }
            Err(e) => (),
        }
    }
    let mut hm: Vec<(_, _)> = hm.iter().collect();
    hm.sort_by_key(|(_, v)| *v);
    for (k, v) in hm {
        let k = k.iter().map(|v| format!("{:02x}", v)).collect::<String>();
        println!("{} {}", k, v);
    }
    // println!("{:#x?}",parse("8015000000094c6576656c732f465a08466c616748756e7400000100000000000000000000000000000000000004105feb0006003e1125f3bc1300000019007e9dfa0404d5f9003f00000000000000000000"));
    // println!("{:#x?}",parse("8015000000094c6576656c732f465a08466c616748756e7400002000000000000000000000000000000000000004105feb0006003e1125f3bc1300000019007e9dfa0404d5f9003f000000000000000000001f020b0376a8e2475b6e5b467c1e99461e020903982d14c5ec79cb45b2ee96471d020e03b29dbc46caa433464a28a0c71c020603aa80514658b8ab458db025c71b020803ce492f4658b8ab4514d320c71a02070344532f4658b8ab4587cf16c7190205031b3a0d4658b8ab459eaf25c7180206030ac34c4669e1fd469891ca47170208032e8c2a4669e1fd465500cd4716020703a4952a4669e1fd461b02d247150205037b7c084669e1fd460f92ca4714020603da6b7ec714aa3746b77c5a4713020803c87c83c714aa3746305a5f47120207039a7b83c714aa3746bd5d694711020503bfbe87c714aa3746a67d5a4710020803c5c719474ad5d445a7b3d2c60f0206037c5522474ad5d4459a6edcc60e02070323ca19474ad5d4458dacbec60d020503d84311474ad5d445bb6cdcc60c020603a9b16b47d52d974602dd15470b020803f2236347d52d97467bba1a470a02070350266347d52d974608be24470902050305a05a47d52d9746f1dd1547080206031f4066c6384b9c46955bd345070208037e3b84c6384b9c466147fa4506020703c33684c6384b9c46e431254605020503574395c6384b9c461063d34504020603ba349bc77a60294640f387c103020803957b9fc77a602946658f994402020703677a9fc77a60294680006d45010205038cbda3c77a602946807880c1"));
}
