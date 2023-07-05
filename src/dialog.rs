use std::path::PathBuf;
use std::string::FromUtf16Error;
use crate::dialog::OpenFileError::{Canceled, StringError};

pub(crate) enum OpenFileError {
    StringError(FromUtf16Error),
    Canceled,
}

impl From<FromUtf16Error> for OpenFileError {
    fn from(value: FromUtf16Error) -> Self {
        StringError(value)
    }
}

pub(crate) fn open_file() -> Result<PathBuf, OpenFileError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::{PWSTR, PCWSTR},
        w,
        Win32::Foundation::HWND,
        Win32::UI::Controls::Dialogs::{GetOpenFileNameW, OPENFILENAMEW, OFN_PATHMUSTEXIST, OFN_FILEMUSTEXIST},
    };
    use std::iter::once;
    use std::mem::size_of;
    use std::ptr::addr_of_mut;

    let mut bytes = [0u16; 260];
    let str = PWSTR::from_raw(bytes.as_mut_ptr());

    let current_dir_buffer = std::env::current_dir()
        .unwrap()
        .into_os_string()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();

    let mut ofn = OPENFILENAMEW::default();

    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = HWND::default();
    ofn.lpstrFile = str;
    ofn.nMaxFile = std::mem::size_of_val(&bytes) as u32;
    ofn.lpstrFilter = w!("ROM files\0*.gb\0");
    ofn.nFilterIndex = 1;
    ofn.lpstrFileTitle = PWSTR::null();
    ofn.nMaxFileTitle = 0;
    ofn.lpstrInitialDir = PCWSTR::from_raw(current_dir_buffer.as_ptr());
    ofn.Flags = OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST;

    unsafe {
        let result = GetOpenFileNameW(addr_of_mut!(ofn)).as_bool();
        if result {
            let str = String::from_utf16(&bytes)?;
            
            let index = str.find('\0').unwrap();

            return Ok(PathBuf::from(&str[0..index]));
        }
    }

    return Err(Canceled);
}
