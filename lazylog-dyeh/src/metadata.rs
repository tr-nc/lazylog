use std::{ffi::CString, io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeSpec {
    pub sec: i64,
    pub nsec: i64,
}

#[derive(Clone, Debug)]
pub struct MetaSnap {
    pub len: u64,
    pub mtime: TimeSpec,
}

#[cfg(target_os = "macos")]
pub fn stat_path(path: &Path) -> io::Result<MetaSnap> {
    use libc::{stat as stat_t, stat};
    use std::mem;

    let cpath = CString::new(path.to_str().unwrap())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let mut st: stat_t = unsafe { mem::zeroed() };
    if unsafe { stat(cpath.as_ptr(), &mut st) } != 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(MetaSnap {
        len: st.st_size as u64,
        mtime: TimeSpec {
            sec: st.st_mtime as i64,
            nsec: st.st_mtime_nsec as i64,
        },
    })
}

pub fn has_changed(prev: &Option<MetaSnap>, cur: &MetaSnap) -> bool {
    match prev {
        None => true,
        Some(p) => p.len != cur.len || p.mtime != cur.mtime,
    }
}
