use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;
use va_list::VaList;

static PRINT_BUF: Mutex<String> = Mutex::new(String::new());

extern "C" {
      fn strlen(s: *const u8) -> usize;

      fn vsnprintf(buf: *const u8, len: usize, fmt: *const u8, args: VaList) -> cty::c_int;
}

#[no_mangle]
unsafe extern "C" fn AcpiOsVprintf(format: *const u8, args: VaList) {
      let mut buf = PRINT_BUF.lock();

      let mut new_buf = {
            let len = strlen(format);
            let slice = core::slice::from_raw_parts(format, len as usize);
            let mut buf = Vec::with_capacity(256);
            buf.extend_from_slice(slice);
            {
                  let ptr = buf.as_mut_ptr();
                  vsnprintf(ptr, 256, format, args);
            }
            buf
      };

      let mut input: &[u8] = &new_buf;
      loop {
            match core::str::from_utf8(input) {
                  Ok(valid) => {
                        buf.push_str(valid);
                        break;
                  }
                  Err(error) => {
                        let (valid, after_valid) = input.split_at(error.valid_up_to());
                        unsafe { buf.push_str(core::str::from_utf8_unchecked(valid)) }
                        buf.push('\u{FFFD}');

                        if let Some(invalid_sequence_length) = error.error_len() {
                              input = &after_valid[invalid_sequence_length..]
                        } else {
                              break;
                        }
                  }
            }
      }

      if buf.ends_with('\n') {
            buf.pop();
            log::info!("{}", &buf);
            buf.clear();
      }
}
