

fn rd_u16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn rd_u32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn rd_u64(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

const PT_LOAD: u32 = 1;
const SHT_DYNSYM: u32 = 11;

pub struct Elf<'a> {
    data: &'a [u8],

    pub min_vaddr: u64,
    dynsym_off: usize,
    dynsym_size: usize,
    dynsym_entsize: usize,
    dynstr_off: usize,
    dynstr_size: usize,
}

impl<'a> Elf<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Elf<'a>, String> {
        if data.len() < 64 || &data[0..4] != b"\x7fELF" {
            return Err("not an ELF file".into());
        }
        if data[4] != 2 {
            return Err("not ELF64".into());
        }
        if data[5] != 1 {
            return Err("not little-endian ELF".into());
        }

        let e_phoff = rd_u64(data, 0x20).ok_or("bad e_phoff")? as usize;
        let e_phentsize = rd_u16(data, 0x36).ok_or("bad e_phentsize")? as usize;
        let e_phnum = rd_u16(data, 0x38).ok_or("bad e_phnum")? as usize;
        let e_shoff = rd_u64(data, 0x28).ok_or("bad e_shoff")? as usize;
        let e_shentsize = rd_u16(data, 0x3a).ok_or("bad e_shentsize")? as usize;
        let e_shnum = rd_u16(data, 0x3c).ok_or("bad e_shnum")? as usize;


        let mut min_vaddr = u64::MAX;
        for i in 0..e_phnum {
            let o = e_phoff + i * e_phentsize;
            if rd_u32(data, o) == Some(PT_LOAD) {
                if let Some(v) = rd_u64(data, o + 16) {
                    min_vaddr = min_vaddr.min(v);
                }
            }
        }
        if min_vaddr == u64::MAX {
            min_vaddr = 0;
        }

        if e_shnum == 0 || e_shoff == 0 {
            return Err("no section headers (stripped); use fallback".into());
        }


        let mut dynsym = None;
        for i in 0..e_shnum {
            let o = e_shoff + i * e_shentsize;
            if rd_u32(data, o + 4) == Some(SHT_DYNSYM) {
                let sh_offset = rd_u64(data, o + 24).ok_or("sh_offset")? as usize;
                let sh_size = rd_u64(data, o + 32).ok_or("sh_size")? as usize;
                let sh_link = rd_u32(data, o + 40).ok_or("sh_link")? as usize;
                let sh_entsize = rd_u64(data, o + 56).ok_or("sh_entsize")? as usize;
                dynsym = Some((sh_offset, sh_size, sh_entsize, sh_link));
                break;
            }
        }
        let (dynsym_off, dynsym_size, dynsym_entsize, strtab_idx) =
            dynsym.ok_or("no .dynsym section")?;
        if dynsym_entsize == 0 {
            return Err("bad dynsym entsize".into());
        }

        let so = e_shoff + strtab_idx * e_shentsize;
        let dynstr_off = rd_u64(data, so + 24).ok_or("dynstr off")? as usize;
        let dynstr_size = rd_u64(data, so + 32).ok_or("dynstr size")? as usize;

        Ok(Elf {
            data,
            min_vaddr,
            dynsym_off,
            dynsym_size,
            dynsym_entsize,
            dynstr_off,
            dynstr_size,
        })
    }


    pub fn resolve(&self, name: &str) -> Option<u64> {
        let count = self.dynsym_size / self.dynsym_entsize;
        for i in 0..count {
            let o = self.dynsym_off + i * self.dynsym_entsize;
            let st_name = rd_u32(self.data, o)? as usize;
            let st_value = rd_u64(self.data, o + 8)?;
            if st_value == 0 {
                continue;
            }
            if self.name_matches(st_name, name) {
                return Some(st_value);
            }
        }
        None
    }

    fn name_matches(&self, st_name: usize, want: &str) -> bool {
        if st_name >= self.dynstr_size {
            return false;
        }
        let start = self.dynstr_off + st_name;
        let end = (self.dynstr_off + self.dynstr_size).min(self.data.len());
        if start >= end {
            return false;
        }
        let bytes = &self.data[start..end];
        let n = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        &bytes[..n] == want.as_bytes()
    }
}
