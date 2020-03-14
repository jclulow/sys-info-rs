#![cfg(target_os = "solaris")]

use std::os::raw::c_int;
use std::os::raw::c_uint;
use std::os::raw::c_char;
use std::os::raw::c_uchar;
use std::os::raw::c_void;
use std::os::raw::c_long;
use std::os::raw::c_ulong;
use std::os::raw::c_longlong;
use std::ptr::{null, null_mut};
use std::ffi::CStr;
use std::ffi::CString;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;


const KSTAT_STRLEN: usize = 31;

const MODULE_CPU_INFO: &str = "cpu_info";

const STAT_CLOCK_MHZ: &str = "clock_MHz";

const MODULE_UNIX: &str = "unix";

const NAME_SYSTEM_MISC: &str = "system_misc";
const STAT_BOOT_TIME: &str = "boot_time";
const STAT_NPROC: &str = "nproc";

const NAME_SYSTEM_PAGES: &str = "system_pages";
const STAT_FREEMEM: &str = "freemem";
const STAT_PHYSMEM: &str = "physmem";


#[repr(C)]
struct Kstat {
    ks_crtime: c_longlong,
    ks_next: *mut Kstat,
    ks_kid: c_uint,
    ks_module: [c_char; KSTAT_STRLEN],
    ks_resv: c_uchar,
    ks_instance: c_int,
    ks_name: [c_char; KSTAT_STRLEN],
    ks_type: c_uchar,
    ks_class: [c_char; KSTAT_STRLEN],
    ks_flags: c_uchar,
    ks_data: *mut c_void,
    ks_ndata: c_uint,
    ks_data_size: usize,
    ks_snaptime: c_longlong,
}

impl Kstat {
    unsafe fn name(&self) -> String {
        CStr::from_ptr(self.ks_name.as_ptr()).to_str().unwrap().to_string()
    }

    unsafe fn module(&self) -> String {
        CStr::from_ptr(self.ks_module.as_ptr()).to_str().unwrap().to_string()
    }
}

#[repr(C)]
struct KstatCtl {
    kc_chain_id: c_uint,
    kc_chain: *mut Kstat,
    kc_kd: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
union KstatValue {
    c: [c_char; 16],
    l: c_long,
    ul: c_ulong,
    ui32: u32,
}

#[repr(C)]
struct KstatNamed {
    name: [c_char; KSTAT_STRLEN],
    data_type: c_uchar,
    value: KstatValue,
}

extern "C" {
    fn kstat_open() -> *mut KstatCtl;
    fn kstat_close(kc: *mut KstatCtl) -> c_int;
    fn kstat_lookup(kc: *mut KstatCtl, module: *const c_char,
        instance: c_int, name: *const c_char) -> *mut Kstat;
    fn kstat_read(kc: *mut KstatCtl, ksp: *mut Kstat, buf: *mut c_void)
        -> c_int;
    fn kstat_data_lookup(ksp: *mut Kstat, name: *const c_char) -> *mut c_void;
}

/// Minimal wrapper around libkstat(3LIB) on illumos and Solaris systems.
struct KstatWrapper {
    kc: *mut KstatCtl,
    ksp: *mut Kstat,
    stepping: bool,
}

/// Turn an optional string reference into an optional owned C string.
fn cstr(s: Option<&str>) -> Option<std::ffi::CString> {
    s.map_or(None, |s| Some(CString::new(s.to_owned()).unwrap()))
}

/// Turn an optional C string into a (const char *) for Some, or NULL for None.
fn cstrp(p: &Option<CString>) -> *const c_char {
    if let Some(p) = p.as_ref() {
        p.as_ptr()
    } else {
        null()
    }
}

impl KstatWrapper {
    fn open() -> Result<Self> {
        let kc = unsafe { kstat_open() };
        if kc == null_mut() {
            return Err("kstat_open(3KSTAT) failed".into());
        }
        Ok(KstatWrapper {
            kc: kc,
            ksp: null_mut(),
            stepping: false,
        })
    }

    /// Call kstat_lookup(3KSTAT) and store the result, if there is a match.
    fn lookup(&mut self, module: Option<&str>, name: Option<&str>) {
        let module = cstr(module);
        let name = cstr(name);

        unsafe {
            self.ksp = kstat_lookup(self.kc, cstrp(&module), -1, cstrp(&name));
        }

        self.stepping = false;
    }

    /// Call once to start iterating, and then repeatedly for each additional
    /// kstat in the chain.  Returns false once there are no more kstat entries.
    fn step(&mut self) -> bool {
        if !self.stepping {
            self.stepping = true;
        } else {
            self.ksp = unsafe { (*self.ksp).ks_next };
        }

        if self.ksp == null_mut() {
            self.stepping = false;
            false
        } else {
            true
        }
    }

    /// Return the module name of the current kstat.
    fn module(&self) -> Option<String> {
        if self.ksp == null_mut() {
            None
        } else {
            Some(unsafe { (*self.ksp).module() })
        }
    }

    /// Return the name of the current kstat.
    fn name(&self) -> Option<String> {
        if self.ksp == null_mut() {
            None
        } else {
            Some(unsafe { (*self.ksp).name() })
        }
    }

    /// Look up a named kstat value.  For internal use by typed accessors.
    fn data_value(&self, statistic: &str) -> Option<*const KstatNamed> {
        if self.ksp == null_mut() {
            return None;
        }

        if unsafe { kstat_read(self.kc, self.ksp, null_mut()) } == -1 {
            return None;
        }

        let stat = cstr(Some(statistic));

        unsafe {
            let knp = kstat_data_lookup(self.ksp, cstrp(&stat));
            if knp == null_mut() {
                return None;
            }

            Some(knp as *const KstatNamed)
        }
    }

    /// Look up a named kstat value and interpret it as a "long_t".
    fn data_long(&self, statistic: &str) -> Option<i64> {
        match self.data_value(statistic) {
            Some(knp) => unsafe { Some((*knp).value.l) },
            None => None,
        }
    }

    /// Look up a named kstat value and interpret it as a "ulong_t".
    fn data_ulong(&self, statistic: &str) -> Option<u64> {
        match self.data_value(statistic) {
            Some(knp) => unsafe { Some((*knp).value.ul) },
            None => None,
        }
    }

    /// Look up a named kstat value and interpret it as a "uint32_t".
    fn data_u32(&self, statistic: &str) -> Option<u32> {
        match self.data_value(statistic) {
            Some(knp) => unsafe { Some((*knp).value.ui32) },
            None => None,
        }
    }
}

impl Drop for KstatWrapper {
    fn drop(&mut self) {
        unsafe { kstat_close(self.kc); };
    }
}

pub fn cpu_mhz() -> Result<u64> {
    let mut k = KstatWrapper::open()?;

    k.lookup(Some(MODULE_CPU_INFO), None);
    while k.step() {
        if k.module().unwrap() != MODULE_CPU_INFO {
            continue;
        }

        if let Some(mhz) = k.data_long(STAT_CLOCK_MHZ) {
            return Ok(mhz as u64);
        }
    }

    return Err("cpu speed kstat not found".into());
}

pub fn boot_time() -> Result<u64> {
    let mut k = KstatWrapper::open()?;

    k.lookup(Some(MODULE_UNIX), Some(NAME_SYSTEM_MISC));
    while k.step() {
        if k.module().unwrap() != MODULE_UNIX ||
            k.name().unwrap() != NAME_SYSTEM_MISC
        {
            continue;
        }

        if let Some(boot_time) = k.data_u32(STAT_BOOT_TIME) {
            return Ok(boot_time as u64);
        }
    }

    return Err("boot time kstat not found".into());
}

pub fn nproc() -> Result<u64> {
    let mut k = KstatWrapper::open()?;

    k.lookup(Some(MODULE_UNIX), Some(NAME_SYSTEM_MISC));
    while k.step() {
        if k.module().unwrap() != MODULE_UNIX ||
            k.name().unwrap() != NAME_SYSTEM_MISC
        {
            continue;
        }

        if let Some(nproc) = k.data_u32(STAT_NPROC) {
            return Ok(nproc as u64);
        }
    }

    return Err("process count kstat not found".into());
}

pub struct Pages {
    pub freemem: u64,
    pub physmem: u64,
}

pub fn pages() -> Result<Pages> {
    let mut k = KstatWrapper::open()?;

    k.lookup(Some(MODULE_UNIX), Some(NAME_SYSTEM_PAGES));
    while k.step() {
        if k.module().unwrap() != MODULE_UNIX ||
            k.name().unwrap() != NAME_SYSTEM_PAGES
        {
            continue;
        }

        let freemem = k.data_ulong(STAT_FREEMEM);
        let physmem = k.data_ulong(STAT_PHYSMEM);

        if freemem.is_some() && physmem.is_some() {
            return Ok(Pages {
                freemem: freemem.unwrap(),
                physmem: physmem.unwrap(),
            });
        }
    }

    return Err("system pages kstat not available".into());
}
