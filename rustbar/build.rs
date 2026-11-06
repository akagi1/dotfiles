//! Embeds assets/icon.ico as the exe's icon (resource id 1) without needing
//! rc.exe or windres: the .ico is turned into a COFF object holding a
//! `.rsrc` section, which both the GNU and MSVC linkers accept.

use std::env;
use std::fs;
use std::path::PathBuf;

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
const ICON_ID: u32 = 1;
const LANG_EN_US: u32 = 0x0409;
const SUBDIR: u32 = 0x8000_0000;
const IMAGE_REL_AMD64_ADDR32NB: u16 = 3;

struct Resource {
    kind: u32,
    id: u32,
    data: Vec<u8>,
}

fn put_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend(v.to_le_bytes());
}

fn put_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend(v.to_le_bytes());
}

/// IMAGE_RESOURCE_DIRECTORY header followed by its (id, offset) entries.
fn put_dir(buf: &mut Vec<u8>, entries: &[(u32, u32)]) {
    buf.extend([0u8; 12]); // Characteristics, TimeDateStamp, Major/MinorVersion
    put_u16(buf, 0); // NumberOfNamedEntries
    put_u16(buf, entries.len() as u16); // NumberOfIdEntries
    for &(id, offset) in entries {
        put_u32(buf, id);
        put_u32(buf, offset);
    }
}

/// Splits an .ico file into RT_ICON images plus the RT_GROUP_ICON directory.
fn icon_resources(ico: &[u8]) -> Vec<Resource> {
    let u16_at = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([ico[i], ico[i + 1], ico[i + 2], ico[i + 3]]);
    let count = u16_at(4) as u32;

    let mut resources = Vec::new();
    // Same layout as the .ico directory, but each entry ends with a
    // resource id instead of a file offset.
    let mut group = ico[0..6].to_vec();
    for n in 0..count {
        let entry = 6 + 16 * n as usize;
        let len = u32_at(entry + 8) as usize;
        let offset = u32_at(entry + 12) as usize;
        let image_id = n + 1;
        resources.push(Resource { kind: RT_ICON, id: image_id, data: ico[offset..offset + len].to_vec() });
        group.extend(&ico[entry..entry + 12]);
        put_u16(&mut group, image_id as u16);
    }
    resources.push(Resource { kind: RT_GROUP_ICON, id: ICON_ID, data: group });
    resources
}

/// Builds the .rsrc section contents (type -> id -> language tree) and the
/// offsets of the data-entry RVA fields that need relocations.
fn rsrc_section(resources: &[Resource]) -> (Vec<u8>, Vec<u32>) {
    let mut kinds: Vec<u32> = resources.iter().map(|r| r.kind).collect();
    kinds.dedup();
    let dir_size = |n: usize| 16 + 8 * n as u32;
    let of_kind = |k: u32| resources.iter().filter(move |r| r.kind == k);

    // Offsets of each part of the tree, laid out in order.
    let root = 0;
    let mut kind_dirs = Vec::new();
    let mut next = root + dir_size(kinds.len());
    for &k in &kinds {
        kind_dirs.push(next);
        next += dir_size(of_kind(k).count());
    }
    let lang_dirs = next;
    let data_entries = lang_dirs + dir_size(1) * resources.len() as u32;
    let mut data_offset = data_entries + 16 * resources.len() as u32;

    let mut buf = Vec::new();
    let kind_entries: Vec<_> = kinds.iter().zip(&kind_dirs).map(|(&k, &o)| (k, o | SUBDIR)).collect();
    put_dir(&mut buf, &kind_entries);
    // Resources are grouped by kind in `resources`, so their index matches
    // the order of the language directories.
    let mut index = 0;
    for &k in &kinds {
        let entries: Vec<_> = of_kind(k)
            .map(|r| {
                let entry = (r.id, (lang_dirs + dir_size(1) * index) | SUBDIR);
                index += 1;
                entry
            })
            .collect();
        put_dir(&mut buf, &entries);
    }
    for i in 0..resources.len() as u32 {
        put_dir(&mut buf, &[(LANG_EN_US, data_entries + 16 * i)]);
    }
    let mut relocations = Vec::new();
    for r in resources {
        relocations.push(buf.len() as u32);
        put_u32(&mut buf, data_offset); // OffsetToData (made an RVA by the linker)
        put_u32(&mut buf, r.data.len() as u32);
        put_u32(&mut buf, 0); // CodePage
        put_u32(&mut buf, 0); // Reserved
        data_offset += (r.data.len() as u32 + 7) & !7;
    }
    for r in resources {
        buf.extend(&r.data);
        buf.resize((buf.len() + 7) & !7, 0);
    }
    (buf, relocations)
}

/// Wraps the section in a minimal x86-64 COFF object file.
fn coff_object(section: &[u8], relocations: &[u32]) -> Vec<u8> {
    let raw_ptr = 20 + 40;
    let reloc_ptr = raw_ptr + section.len() as u32;
    let symtab_ptr = reloc_ptr + 10 * relocations.len() as u32;

    let mut obj = Vec::new();
    put_u16(&mut obj, 0x8664); // Machine: x86-64
    put_u16(&mut obj, 1); // NumberOfSections
    put_u32(&mut obj, 0); // TimeDateStamp
    put_u32(&mut obj, symtab_ptr);
    put_u32(&mut obj, 1); // NumberOfSymbols
    put_u16(&mut obj, 0); // SizeOfOptionalHeader
    put_u16(&mut obj, 0); // Characteristics

    obj.extend(b".rsrc\0\0\0");
    put_u32(&mut obj, 0); // VirtualSize
    put_u32(&mut obj, 0); // VirtualAddress
    put_u32(&mut obj, section.len() as u32);
    put_u32(&mut obj, raw_ptr);
    put_u32(&mut obj, reloc_ptr);
    put_u32(&mut obj, 0); // PointerToLinenumbers
    put_u16(&mut obj, relocations.len() as u16);
    put_u16(&mut obj, 0); // NumberOfLinenumbers
    put_u32(&mut obj, 0x4030_0040); // INITIALIZED_DATA | ALIGN_4BYTES | MEM_READ

    obj.extend(section);
    for &offset in relocations {
        put_u32(&mut obj, offset);
        put_u32(&mut obj, 0); // symbol 0: the section itself
        put_u16(&mut obj, IMAGE_REL_AMD64_ADDR32NB);
    }

    obj.extend(b".rsrc\0\0\0");
    put_u32(&mut obj, 0); // Value
    put_u16(&mut obj, 1); // SectionNumber
    put_u16(&mut obj, 0); // Type
    obj.push(3); // StorageClass: STATIC
    obj.push(0); // NumberOfAuxSymbols
    put_u32(&mut obj, 4); // empty string table
    obj
}

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("x86_64")
    {
        return;
    }
    let ico = fs::read("assets/icon.ico").expect("read assets/icon.ico");
    let (section, relocations) = rsrc_section(&icon_resources(&ico));
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("icon.o");
    fs::write(&out, coff_object(&section, &relocations)).expect("write icon.o");
    println!("cargo:rustc-link-arg-bins={}", out.display());
}
