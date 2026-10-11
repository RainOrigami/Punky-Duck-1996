//! le2elf — convert a DOS/4GW "LE" (Linear Executable) into a static ELF32 i386 image
//! so that ordinary disassemblers (objdump, Ghidra) can load it.
//!
//! Layout references: Open Watcom `bld/watcom/h/exeflat.h` (`os2_flat_header`,
//! `object_record`, `le_map_entry`, `OSF_*` fixup flags) and the IBM LX spec rev. 11.
//! Every object is loaded at its LE relocation base address and all fixups are applied
//! against those bases, so addresses in the ELF equal the addresses the linker assigned
//! (DOS/4GW itself loads the objects elsewhere and relocates; that is irrelevant here).

use std::fmt::Write as _;
use std::process::ExitCode;

type Res<T> = Result<T, String>;

fn u16le(d: &[u8], o: usize) -> Res<u16> {
    d.get(o..o + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| format!("read u16 past end of file at {o:#x}"))
}
fn u32le(d: &[u8], o: usize) -> Res<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| format!("read u32 past end of file at {o:#x}"))
}
fn u8at(d: &[u8], o: usize) -> Res<u8> {
    d.get(o).copied().ok_or_else(|| format!("read u8 past end of file at {o:#x}"))
}

/// Locate the LE header. Handles a plain MZ stub with `e_lfanew` (WLINK output) and a
/// chain of bound MZ/BW images (DOS/4GW bound executables) in front of the LE module.
fn find_le(d: &[u8]) -> Res<usize> {
    let is_le = |o: usize| d.get(o..o + 4) == Some(b"LE\0\0");
    let mut pos = 0usize;
    for _ in 0..16 {
        let sig = d.get(pos..pos + 2).ok_or("ran off file end while looking for LE header")?;
        if is_le(pos) {
            return Ok(pos);
        }
        if sig != b"MZ" && sig != b"BW" {
            break;
        }
        if sig == b"MZ" && u16le(d, pos + 0x18)? >= 0x40 {
            let lfanew = pos + u32le(d, pos + 0x3c)? as usize;
            if lfanew != pos && is_le(lfanew) {
                return Ok(lfanew);
            }
        }
        // next image in the chain: e_cp pages of 512 bytes, e_cblp bytes in the last one
        let cblp = u16le(d, pos + 2)? as usize;
        let cp = u16le(d, pos + 4)? as usize;
        if cp == 0 {
            break;
        }
        pos += if cblp == 0 { cp * 512 } else { (cp - 1) * 512 + cblp };
    }
    Err("no LE header found (not an MZ+LE / DOS/4GW executable?)".into())
}

struct Object {
    vsize: u32,
    base: u32,
    flags: u32,
    map_idx: u32,
    map_cnt: u32,
    /// bytes backed by file pages (zero fill beyond, up to `vsize`)
    file_len: u32,
    image: Vec<u8>,
}

#[derive(Default)]
struct Stats {
    records: usize,
    sources: usize,
    cross_page_dups: usize,
    by_kind: std::collections::BTreeMap<(u8, u8), usize>,
}

struct Le {
    objects: Vec<Object>,
    entry: u32,
    stack: u32,
    page_size: u32,
    stats: Stats,
    /// (address of patched dword/word, value written)
    fixups: Vec<(u32, u32)>,
    info: String,
}

fn parse(d: &[u8]) -> Res<Le> {
    let le = find_le(d)?;
    let h = |o: usize| u32le(d, le + o);
    if u8at(d, le + 2)? != 0 || u8at(d, le + 3)? != 0 {
        return Err("big-endian LE not supported".into());
    }
    if h(0x04)? != 0 {
        return Err(format!("unsupported LE format level {}", h(4)?));
    }
    let num_pages = h(0x14)? as usize;
    let (eip_obj, eip, esp_obj, esp) = (h(0x18)?, h(0x1c)?, h(0x20)?, h(0x24)?);
    let page_size = h(0x28)?;
    let last_page = h(0x2c)?;
    let objtab = le + h(0x40)? as usize;
    let num_obj = h(0x44)? as usize;
    let objmap = le + h(0x48)? as usize;
    let fixpage = le + h(0x68)? as usize;
    let fixrec = le + h(0x6c)? as usize;
    let num_impmods = h(0x74)?;
    let data_pages = h(0x80)? as usize; // relative to start of file (not to LE header)
    if page_size == 0 || !page_size.is_power_of_two() {
        return Err(format!("bad page size {page_size:#x}"));
    }
    if h(0x4c)? != 0 {
        return Err("iterated-data pages present (idmap_off != 0): unsupported".into());
    }
    let ps = page_size as usize;
    let mut info = String::new();
    let _ = writeln!(
        info,
        "LE header at {le:#x}: cpu {} os {} flags {:#x} pages {num_pages} page_size {page_size:#x} last_page {last_page:#x}\n\
         data pages at file {data_pages:#x}, fixup page table {fixpage:#x}, fixup records {fixrec:#x}, import modules {num_impmods}",
        u16le(d, le + 8)?,
        u16le(d, le + 10)?,
        h(0x10)?
    );

    let mut objects = Vec::new();
    for i in 0..num_obj {
        let o = objtab + 24 * i;
        let (vsize, base, flags, map_idx, map_cnt) =
            (u32le(d, o)?, u32le(d, o + 4)?, u32le(d, o + 8)?, u32le(d, o + 12)?, u32le(d, o + 16)?);
        if base % page_size != 0 {
            return Err(format!("object {} base {base:#x} not page aligned", i + 1));
        }
        let mut image = vec![0u8; vsize as usize];
        let mut file_len = 0u32;
        for p in 0..map_cnt as usize {
            let logical = map_idx as usize - 1 + p; // 0-based logical page
            if logical >= num_pages {
                return Err(format!("object {} references page {} > num_pages", i + 1, logical + 1));
            }
            // le_map_entry: 3-byte big-endian physical page number (1-based), 1 flag byte
            let e = objmap + 4 * logical;
            let phys = ((u8at(d, e)? as usize) << 16) | ((u8at(d, e + 1)? as usize) << 8) | u8at(d, e + 2)? as usize;
            let pflags = u8at(d, e + 3)?;
            let dst = p * ps;
            if dst >= image.len() {
                break;
            }
            match pflags {
                0 => {} // legal physical page
                2 | 3 => continue, // invalid / zero-filled page
                f => return Err(format!("page {} has unsupported page flags {f:#x}", logical + 1)),
            }
            if phys == 0 {
                return Err(format!("page {} is flagged valid but has physical page number 0", logical + 1));
            }
            let src = data_pages + (phys - 1) * ps;
            // only the very last physical page of the module is short
            let avail = if phys == num_pages { last_page as usize } else { ps };
            let n = avail.min(image.len() - dst);
            let bytes = d
                .get(src..src + n)
                .ok_or_else(|| format!("page {} data {src:#x}+{n:#x} is past end of file", logical + 1))?;
            image[dst..dst + n].copy_from_slice(bytes);
            file_len = (dst + n) as u32;
        }
        let _ = writeln!(
            info,
            "object {}: base {base:#010x} vsize {vsize:#x} flags {flags:#06x} [{}{}{}{}] pages {map_idx}..{} file-backed {file_len:#x}",
            i + 1,
            if flags & 1 != 0 { 'R' } else { '-' },
            if flags & 2 != 0 { 'W' } else { '-' },
            if flags & 4 != 0 { 'X' } else { '-' },
            if flags & 0x2000 != 0 { " 32bit" } else { " 16bit" },
            map_idx + map_cnt.max(1) - 1
        );
        objects.push(Object { vsize, base, flags, map_idx, map_cnt, file_len, image });
    }

    // ---- fixups -------------------------------------------------------------------
    let mut stats = Stats::default();
    let mut fixups: Vec<(u32, u32)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let bases: Vec<(u32, u32)> = objects.iter().map(|o| (o.base, o.vsize)).collect();
    for oi in 0..objects.len() {
        for p in 0..objects[oi].map_cnt as usize {
            let logical = objects[oi].map_idx as usize - 1 + p;
            let mut o = fixrec + u32le(d, fixpage + 4 * logical)? as usize;
            let end = fixrec + u32le(d, fixpage + 4 * logical + 4)? as usize;
            let page_va = objects[oi].base as i64 + (p * ps) as i64;
            while o < end {
                let at = o;
                let src = u8at(d, o)?;
                let tf = u8at(d, o + 1)?;
                o += 2;
                let bad = |what: &str| -> String {
                    format!(
                        "UNSUPPORTED FIXUP at file {at:#x} (page {}, object {}): source {src:#04x} target-flags {tf:#04x}: {what}",
                        logical + 1,
                        oi + 1
                    )
                };
                let kind = src & 0x0f;
                if src & 0x10 != 0 {
                    return Err(bad("fixup-to-alias (16:16) source flag"));
                }
                let list = src & 0x20 != 0;
                if src & 0xc0 != 0 {
                    return Err(bad("unknown source flag bits"));
                }
                let mut srcoffs: Vec<i16> = Vec::new();
                let list_cnt = if list {
                    let c = u8at(d, o)?;
                    o += 1;
                    c as usize
                } else {
                    srcoffs.push(u16le(d, o)? as i16);
                    o += 2;
                    0
                };
                if tf & 0x03 != 0 {
                    return Err(bad("target is not an internal reference (imports/entry-table targets)"));
                }
                if tf & 0x88 != 0 {
                    return Err(bad("chained fixup / 8-bit ordinal flag"));
                }
                let tobj = if tf & 0x40 != 0 {
                    let v = u16le(d, o)? as usize;
                    o += 2;
                    v
                } else {
                    let v = u8at(d, o)? as usize;
                    o += 1;
                    v
                };
                // a 16-bit selector fixup carries no target offset
                let toff = if kind == 2 {
                    0
                } else if tf & 0x10 != 0 {
                    let v = u32le(d, o)?;
                    o += 4;
                    v
                } else {
                    let v = u16le(d, o)? as u32;
                    o += 2;
                    v
                };
                let addend = if tf & 0x04 != 0 {
                    if tf & 0x20 != 0 {
                        let v = u32le(d, o)?;
                        o += 4;
                        v
                    } else {
                        let v = u16le(d, o)? as u32;
                        o += 2;
                        v
                    }
                } else {
                    0
                };
                for _ in 0..list_cnt {
                    srcoffs.push(u16le(d, o)? as i16);
                    o += 2;
                }
                if tobj == 0 || tobj > bases.len() {
                    return Err(bad(&format!("target object {tobj} out of range")));
                }
                let (tbase, tsize) = bases[tobj - 1];
                if toff > tsize {
                    eprintln!("warning: fixup at file {at:#x} targets object {tobj}+{toff:#x}, beyond its virtual size {tsize:#x}");
                }
                let target = tbase.wrapping_add(toff).wrapping_add(addend);
                stats.records += 1;
                *stats.by_kind.entry((src, tf)).or_default() += 1;
                for so in srcoffs {
                    stats.sources += 1;
                    let va = page_va + so as i64;
                    let width: usize = match kind {
                        7 | 8 => 4,
                        5 => 2,
                        0 => return Err(bad("8-bit source")),
                        2 | 3 | 6 => return Err(bad("selector / far pointer source needs a runtime selector")),
                        _ => return Err(bad("unknown source type")),
                    };
                    let img = &mut objects[oi];
                    let rel = va - img.base as i64;
                    // A fixup that straddles a page boundary is recorded in both pages
                    // (negative source offset in the second one); apply it only once.
                    if !seen.insert(va) {
                        stats.cross_page_dups += 1;
                        continue;
                    }
                    if rel < 0 || rel as usize + width > img.image.len() {
                        return Err(bad(&format!("source address {va:#x} outside object")));
                    }
                    let rel = rel as usize;
                    let value = match kind {
                        7 => target,
                        8 => target.wrapping_sub(va as u32 + 4),
                        5 => target & 0xffff,
                        _ => unreachable!(),
                    };
                    img.image[rel..rel + width].copy_from_slice(&value.to_le_bytes()[..width]);
                    if rel + width > img.file_len as usize {
                        img.file_len = (rel + width) as u32;
                    }
                    fixups.push((va as u32, value));
                }
            }
            if o != end {
                return Err(format!("fixup records of page {} overrun their table slice ({o:#x} != {end:#x})", logical + 1));
            }
        }
    }
    // Cross-check: the fixup page table's final entry must equal the record table length.
    let total = u32le(d, fixpage + 4 * num_pages)? as usize;
    let declared = h(0x30)? as usize; // fixup_size = page table + record table (+ import tables)
    let _ = writeln!(
        info,
        "fixup record table: {total:#x} bytes in page table, header fixup_size {declared:#x} (page table {:#x} + records)",
        fixrec - fixpage
    );

    let obj_base = |n: u32, what: &str| -> Res<u32> {
        objects
            .get((n as usize).wrapping_sub(1))
            .map(|o| o.base)
            .ok_or_else(|| format!("{what} object {n} out of range"))
    };
    let entry = obj_base(eip_obj, "entry")?.wrapping_add(eip);
    let stack = if esp_obj == 0 { 0 } else { obj_base(esp_obj, "stack")?.wrapping_add(esp) };
    fixups.sort_unstable();
    Ok(Le { objects, entry, stack, page_size, stats, fixups, info })
}

// ---- ELF writer -------------------------------------------------------------------

fn put16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn write_elf(le: &Le) -> Vec<u8> {
    const EHSZ: u32 = 52;
    const PHSZ: u32 = 32;
    const SHSZ: u32 = 40;
    let align = le.page_size.max(0x1000);
    let n = le.objects.len() as u32;
    // file offsets of each object's file-backed bytes (offset == vaddr mod align)
    let mut off = (EHSZ + PHSZ * n).next_multiple_of(align);
    let mut offs = Vec::new();
    for o in &le.objects {
        offs.push(off);
        off = (off + o.file_len).next_multiple_of(align);
    }
    // sections: null, per object PROGBITS (+ NOBITS tail), .symtab, .strtab, .shstrtab
    let mut shstr = vec![0u8];
    let mut name = |s: &str| -> u32 {
        let at = shstr.len() as u32;
        shstr.extend_from_slice(s.as_bytes());
        shstr.push(0);
        at
    };
    struct Sh {
        name: u32,
        ty: u32,
        flags: u32,
        addr: u32,
        off: u32,
        size: u32,
        link: u32,
        info: u32,
        align: u32,
        entsize: u32,
    }
    let mut shs = vec![Sh { name: 0, ty: 0, flags: 0, addr: 0, off: 0, size: 0, link: 0, info: 0, align: 0, entsize: 0 }];
    let mut entry_shndx = 0u16;
    for (i, o) in le.objects.iter().enumerate() {
        let x = o.flags & 4 != 0;
        let w = o.flags & 2 != 0;
        let fl = 2 | if w { 1 } else { 0 } | if x { 4 } else { 0 }; // SHF_ALLOC|WRITE|EXECINSTR
        let base_name = if x { format!(".text{}", i + 1) } else { format!(".data{}", i + 1) };
        if le.entry >= o.base && le.entry < o.base + o.vsize {
            entry_shndx = shs.len() as u16;
        }
        if o.file_len > 0 {
            shs.push(Sh { name: name(&base_name), ty: 1, flags: fl, addr: o.base, off: offs[i], size: o.file_len, link: 0, info: 0, align: 16, entsize: 0 });
        }
        if o.vsize > o.file_len {
            shs.push(Sh {
                name: name(&format!(".bss{}", i + 1)),
                ty: 8,
                flags: fl & !4,
                addr: o.base + o.file_len,
                off: offs[i] + o.file_len,
                size: o.vsize - o.file_len,
                link: 0,
                info: 0,
                align: 1,
                entsize: 0,
            });
        }
    }
    // symbols: null, _start (entry), __le_stack_top
    let mut strtab = vec![0u8];
    let mut symtab = vec![0u8; 16];
    let mut sym = |nm: &str, value: u32, info: u8, shndx: u16| {
        let at = strtab.len() as u32;
        strtab.extend_from_slice(nm.as_bytes());
        strtab.push(0);
        put32(&mut symtab, at);
        put32(&mut symtab, value);
        put32(&mut symtab, 0);
        symtab.push(info);
        symtab.push(0);
        put16(&mut symtab, shndx);
    };
    sym("_start", le.entry, 0x12, entry_shndx); // GLOBAL FUNC
    if le.stack != 0 {
        sym("__le_stack_top", le.stack, 0x10, 0xfff1); // GLOBAL NOTYPE, SHN_ABS
    }
    let symtab_off = off;
    let strtab_off = symtab_off + symtab.len() as u32;
    let symtab_idx = shs.len() as u32;
    shs.push(Sh { name: name(".symtab"), ty: 2, flags: 0, addr: 0, off: symtab_off, size: symtab.len() as u32, link: symtab_idx + 1, info: 1, align: 4, entsize: 16 });
    shs.push(Sh { name: name(".strtab"), ty: 3, flags: 0, addr: 0, off: strtab_off, size: strtab.len() as u32, link: 0, info: 0, align: 1, entsize: 0 });
    let shstr_off = strtab_off + strtab.len() as u32;
    let shstr_name = name(".shstrtab");
    shs.push(Sh { name: shstr_name, ty: 3, flags: 0, addr: 0, off: shstr_off, size: shstr.len() as u32, link: 0, info: 0, align: 1, entsize: 0 });
    let shoff = (shstr_off + shstr.len() as u32).next_multiple_of(4);

    let mut out = Vec::new();
    out.extend_from_slice(&[0x7f, b'E', b'L', b'F', 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    put16(&mut out, 2); // ET_EXEC
    put16(&mut out, 3); // EM_386
    put32(&mut out, 1);
    put32(&mut out, le.entry);
    put32(&mut out, EHSZ); // e_phoff
    put32(&mut out, shoff);
    put32(&mut out, 0); // e_flags
    put16(&mut out, EHSZ as u16);
    put16(&mut out, PHSZ as u16);
    put16(&mut out, n as u16);
    put16(&mut out, SHSZ as u16);
    put16(&mut out, shs.len() as u16);
    put16(&mut out, shs.len() as u16 - 1);
    for (i, o) in le.objects.iter().enumerate() {
        put32(&mut out, 1); // PT_LOAD
        put32(&mut out, offs[i]);
        put32(&mut out, o.base);
        put32(&mut out, o.base);
        put32(&mut out, o.file_len);
        put32(&mut out, o.vsize);
        let f = (if o.flags & 1 != 0 { 4 } else { 0 }) | (if o.flags & 2 != 0 { 2 } else { 0 }) | (if o.flags & 4 != 0 { 1 } else { 0 });
        put32(&mut out, f);
        put32(&mut out, align);
    }
    for (i, o) in le.objects.iter().enumerate() {
        out.resize(offs[i] as usize, 0);
        out.extend_from_slice(&o.image[..o.file_len as usize]);
    }
    out.resize(symtab_off as usize, 0);
    out.extend_from_slice(&symtab);
    out.extend_from_slice(&strtab);
    out.extend_from_slice(&shstr);
    out.resize(shoff as usize, 0);
    for s in &shs {
        for v in [s.name, s.ty, s.flags, s.addr, s.off, s.size, s.link, s.info, s.align, s.entsize] {
            put32(&mut out, v);
        }
    }
    out
}

fn run() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos = Vec::new();
    let mut fixup_list = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fixups" => fixup_list = Some(it.next().ok_or("--fixups needs a path")?.clone()),
            "-h" | "--help" => pos.clear(),
            _ => pos.push(a.clone()),
        }
    }
    if pos.len() != 2 {
        return Err("usage: le2elf <input.exe> <output.elf> [--fixups <list.txt>]\n\
                    Converts a DOS/4GW LE executable to a static ELF32 i386 image.\n\
                    --fixups writes one line per applied fixup: <address> <value>"
            .into());
    }
    let data = std::fs::read(&pos[0]).map_err(|e| format!("{}: {e}", pos[0]))?;
    let le = parse(&data)?;
    print!("{}", le.info);
    println!(
        "fixups: {} records, {} source locations, {} applied, {} cross-page duplicates skipped",
        le.stats.records,
        le.stats.sources,
        le.fixups.len(),
        le.stats.cross_page_dups
    );
    for ((src, tf), n) in &le.stats.by_kind {
        println!("  source {src:#04x} target-flags {tf:#04x}: {n}");
    }
    if le.stats.sources != le.fixups.len() + le.stats.cross_page_dups {
        return Err("internal error: fixup accounting mismatch".into());
    }
    println!("entry {:#010x}, initial stack {:#010x}", le.entry, le.stack);
    let elf = write_elf(&le);
    std::fs::write(&pos[1], &elf).map_err(|e| format!("{}: {e}", pos[1]))?;
    println!("wrote {} ({} bytes)", pos[1], elf.len());
    if let Some(p) = fixup_list {
        let mut s = String::new();
        for (a, v) in &le.fixups {
            let _ = writeln!(s, "{a:08x} {v:08x}");
        }
        std::fs::write(&p, s).map_err(|e| format!("{p}: {e}"))?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("le2elf: {e}");
            ExitCode::FAILURE
        }
    }
}
