extern crate libtw2_wireshark_dissector_sys as sys;

mod format;
mod intern;
mod spec;
mod tw;
mod tw7;

#[cfg(test)]
mod test {
    use lazy_static::lazy_static;
    use std::sync::Mutex;
    lazy_static! {
        pub static ref TEST_MUTEX: Mutex<()> = Mutex::new(());
    }
}

use intern::intern;
use intern::Interned;
use libtw2_gamenet_spec::Identifier;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::os::raw::c_int;
use std::process;
use uuid::Uuid;

#[allow(non_upper_case_globals)]
#[no_mangle]
pub static plugin_want_major: c_int = 4;

#[allow(non_upper_case_globals)]
#[no_mangle]
pub static plugin_want_minor: c_int = 4;

#[allow(non_upper_case_globals)]
#[no_mangle]
pub static plugin_version: [u8; 6] = *b"0.0.1\0";

#[inline]
fn c(s: &'static str) -> *const c_char {
    intern::intern_static_with_nul(s).c()
}

pub const HFRI_DEFAULT: sys::_header_field_info = sys::_header_field_info {
    name: 0 as _,
    abbrev: 0 as _,
    type_: 0,
    display: 0,
    strings: 0 as _,
    bitmask: 0,
    blurb: 0 as _,
    id: -1,
    parent: 0,
    ref_type: 0,
    same_name_prev_id: -1,
    same_name_next: 0 as _,
};

#[derive(Default)]
struct Counter(u64);

impl Counter {
    fn new() -> Counter {
        Default::default()
    }
    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl<W> warn::Warn<W> for Counter {
    fn warn(&mut self, _warning: W) {
        self.0 += 1;
    }
}

trait IdentifierEx {
    fn _identifier(&self) -> &Identifier;
    fn isnake(&self) -> Interned {
        intern(&self._identifier().snake())
    }
    fn idesc(&self) -> Interned {
        intern(&self._identifier().desc())
    }
}
impl IdentifierEx for Identifier {
    fn _identifier(&self) -> &Identifier {
        self
    }
}

fn to_guid(uuid: Uuid) -> sys::e_guid_t {
    let (data1, data2, data3, &data4) = uuid.as_fields();
    sys::e_guid_t {
        data1,
        data2,
        data3,
        data4,
    }
}

unsafe extern "C" fn proto_register() {
    tw::proto_register();
    tw7::proto_register();
}

unsafe extern "C" fn proto_reg_handoff() {
    tw::proto_reg_handoff();
    tw7::proto_reg_handoff();
}

#[no_mangle]
pub unsafe extern "C" fn plugin_register() {
    {
        let version = CStr::from_ptr(sys::epan_get_version()).to_bytes();
        if version == b"4.0.4" {
            eprintln!("libtw2: Wireshark 4.0.4 is ABI-incompatible with the 4.0 series.");
            eprintln!("libtw2: Use Wireshark 4.0.3 or Wireshark 4.0.5+ instead.");
            eprintln!("libtw2: https://gitlab.com/wireshark/wireshark/-/issues/18908");
            eprintln!("libtw2: https://github.com/heinrich5991/libtw2/issues/73");
            process::abort();
        }
    }
    sys::proto_register_plugin(&sys::proto_plugin {
        register_protoinfo: Some(proto_register),
        register_handoff: Some(proto_reg_handoff),
    });
}
