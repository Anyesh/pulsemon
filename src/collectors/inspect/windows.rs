use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ACCESS_DENIED, ERROR_MORE_DATA, HANDLE,
};
use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows_sys::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, LookupAccountSidW,
    TokenElevation, TokenIntegrityLevel, PSID, TOKEN_ELEVATION, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
use windows_sys::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTSClientProtocolType, WTSFreeMemory, WTSQuerySessionInformationW,
    WTS_CURRENT_SERVER_HANDLE,
};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, ENUM_SERVICE_STATUS_PROCESSW,
    SC_ENUM_PROCESS_INFO, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_ACTIVE, SERVICE_WIN32,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::{Field, Missing};

const REFRESH: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub display: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub id: u32,
    pub kind: &'static str,
}

#[derive(Debug, Clone)]
pub struct Extras {
    /// Services hosted by this process; an `svchost` usually hosts several.
    pub services: Field<Vec<Service>>,
    pub integrity: Field<&'static str>,
    pub elevated: Field<bool>,
    pub session: Field<Session>,
    pub collected: Instant,
}

/// Caches the pid-to-services map, which one `EnumServicesStatusExW` call returns for
/// every service on the machine.
#[derive(Default)]
pub struct ServiceMap {
    by_pid: HashMap<u32, Vec<Service>>,
    loaded: Option<(Instant, Result<(), Missing>)>,
}

impl ServiceMap {
    fn services(&mut self, pid: u32) -> Field<Vec<Service>> {
        let stale = self.loaded.is_none_or(|(at, _)| at.elapsed() >= REFRESH);
        if stale {
            let loaded = enum_services().map(|map| self.by_pid = map);
            self.loaded = Some((Instant::now(), loaded));
        }
        match self.loaded {
            Some((_, Err(missing))) => Err(missing),
            _ => Ok(self.by_pid.get(&pid).cloned().unwrap_or_default()),
        }
    }
}

pub fn read_extras(pid: u32, services: &mut ServiceMap) -> Extras {
    let token = Token::open(pid);
    Extras {
        services: services.services(pid),
        integrity: token.as_ref().map_err(|m| *m).and_then(Token::integrity),
        elevated: token.as_ref().map_err(|m| *m).and_then(Token::elevated),
        session: session(pid),
        collected: Instant::now(),
    }
}

pub fn is_stale(extras: &Extras) -> bool {
    extras.collected.elapsed() >= REFRESH
}

fn last_error() -> Missing {
    if unsafe { GetLastError() } == ERROR_ACCESS_DENIED {
        Missing::Denied
    } else {
        Missing::Unavailable
    }
}

struct Token(HANDLE);

impl Token {
    /// Protected processes refuse even the limited query right, which reads as denied.
    fn open(pid: u32) -> Field<Self> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return Err(last_error());
            }
            let mut token: HANDLE = ptr::null_mut();
            let ok = OpenProcessToken(process, TOKEN_QUERY, &mut token);
            let err = last_error();
            CloseHandle(process);
            if ok == 0 {
                return Err(err);
            }
            Ok(Self(token))
        }
    }

    fn query(&self, class: i32) -> Field<Vec<u64>> {
        let mut needed = 0u32;
        unsafe {
            GetTokenInformation(self.0, class, ptr::null_mut(), 0, &mut needed);
        }
        if needed == 0 {
            return Err(last_error());
        }
        // u64-backed so the structures read from it are suitably aligned.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let ok = unsafe {
            GetTokenInformation(self.0, class, buf.as_mut_ptr().cast(), needed, &mut needed)
        };
        if ok == 0 {
            return Err(last_error());
        }
        Ok(buf)
    }

    fn integrity(&self) -> Field<&'static str> {
        let buf = self.query(TokenIntegrityLevel)?;
        // SAFETY: the buffer holds a TOKEN_MANDATORY_LABEL whose SID points into it.
        let rid = unsafe {
            let label = &*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
            let sid = label.Label.Sid;
            let count = *GetSidSubAuthorityCount(sid);
            if count == 0 {
                return Err(Missing::Unavailable);
            }
            *GetSidSubAuthority(sid, u32::from(count) - 1)
        };
        Ok(integrity_name(rid))
    }

    fn elevated(&self) -> Field<bool> {
        let buf = self.query(TokenElevation)?;
        if buf.len() * 8 < size_of::<TOKEN_ELEVATION>() {
            return Err(Missing::Unavailable);
        }
        // SAFETY: GetTokenInformation filled at least a TOKEN_ELEVATION.
        let elevation = unsafe { &*buf.as_ptr().cast::<TOKEN_ELEVATION>() };
        Ok(elevation.TokenIsElevated != 0)
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub fn integrity_name(rid: u32) -> &'static str {
    match rid {
        0..0x1000 => "Untrusted",
        0x1000..0x2000 => "Low",
        0x2000..0x2100 => "Medium",
        0x2100..0x3000 => "Medium Plus",
        0x3000..0x4000 => "High",
        0x4000..0x5000 => "System",
        _ => "Protected",
    }
}

fn session(pid: u32) -> Field<Session> {
    let mut id = 0u32;
    if unsafe { ProcessIdToSessionId(pid, &mut id) } == 0 {
        return Err(last_error());
    }
    Ok(Session {
        id,
        kind: session_kind(id, client_protocol(id)),
    })
}

/// `WTSClientProtocolType`: 0 for the console, 2 for RDP.
fn client_protocol(session: u32) -> Option<u16> {
    let mut buf: *mut u16 = ptr::null_mut();
    let mut bytes = 0u32;
    let ok = unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            session,
            WTSClientProtocolType,
            &mut buf,
            &mut bytes,
        )
    };
    if ok == 0 || buf.is_null() {
        return None;
    }
    let protocol = (bytes as usize >= size_of::<u16>()).then(|| unsafe { *buf });
    unsafe { WTSFreeMemory(buf.cast::<c_void>()) };
    protocol
}

/// Session 0 is reserved for services since Vista, so a process there was started by
/// the service control manager or another service rather than by a signed-in user.
pub fn session_kind(id: u32, protocol: Option<u16>) -> &'static str {
    match (id, protocol) {
        (0, _) => "services (session 0)",
        (_, Some(0)) => "console",
        (_, Some(2)) => "remote desktop",
        _ => "interactive",
    }
}

fn enum_services() -> Result<HashMap<u32, Vec<Service>>, Missing> {
    let scm = unsafe { OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_ENUMERATE_SERVICE) };
    if scm.is_null() {
        return Err(last_error());
    }
    let mut map: HashMap<u32, Vec<Service>> = HashMap::new();
    let mut buf: Vec<u64> = Vec::new();
    let mut resume = 0u32;
    loop {
        let mut needed = 0u32;
        let mut returned = 0u32;
        let ok = unsafe {
            EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_ACTIVE,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                &mut needed,
                &mut returned,
                &mut resume,
                ptr::null(),
            )
        };
        let more = ok == 0 && unsafe { GetLastError() } == ERROR_MORE_DATA;
        if ok == 0 && !more {
            let err = last_error();
            unsafe { CloseServiceHandle(scm) };
            return Err(err);
        }
        // SAFETY: the call wrote `returned` entries at the start of the buffer, and
        // their string pointers point into the same buffer.
        let entries = unsafe {
            std::slice::from_raw_parts(
                buf.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>(),
                returned as usize,
            )
        };
        for entry in entries {
            let pid = entry.ServiceStatusProcess.dwProcessId;
            if pid != 0 {
                map.entry(pid).or_default().push(Service {
                    name: unsafe { wide_to_string(entry.lpServiceName) },
                    display: unsafe { wide_to_string(entry.lpDisplayName) },
                });
            }
        }
        if !more {
            break;
        }
        // The buffer was too small for the rest: grow it and continue from `resume`.
        buf = vec![0u64; (needed as usize).div_ceil(8).max(buf.len())];
    }
    unsafe { CloseServiceHandle(scm) };
    Ok(map)
}

/// # Safety
/// `p` must be null or point to a NUL-terminated UTF-16 string.
unsafe fn wide_to_string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// `DOMAIN\name` for a SID string such as `S-1-5-18`. Covers accounts that are not
/// in the local user list, such as SYSTEM, LOCAL SERVICE and per-service SIDs.
pub fn account_name(sid: &str) -> Option<String> {
    let wide: Vec<u16> = sid.encode_utf16().chain(Some(0)).collect();
    let mut psid: PSID = ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut psid) } == 0 {
        return None;
    }
    let mut name = [0u16; 256];
    let mut domain = [0u16; 256];
    let (mut name_len, mut domain_len) = (name.len() as u32, domain.len() as u32);
    let mut use_ = 0;
    let ok = unsafe {
        LookupAccountSidW(
            ptr::null(),
            psid,
            name.as_mut_ptr(),
            &mut name_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut use_,
        )
    };
    unsafe { LocalFree(psid) };
    if ok == 0 {
        return None;
    }
    let name = String::from_utf16_lossy(&name[..name_len as usize]);
    let domain = String::from_utf16_lossy(&domain[..domain_len as usize]);
    Some(if domain.is_empty() {
        name
    } else {
        format!("{domain}\\{name}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_levels_by_rid() {
        assert_eq!(integrity_name(0x0000), "Untrusted");
        assert_eq!(integrity_name(0x1000), "Low");
        assert_eq!(integrity_name(0x2000), "Medium");
        assert_eq!(integrity_name(0x2100), "Medium Plus");
        assert_eq!(integrity_name(0x3000), "High");
        assert_eq!(integrity_name(0x4000), "System");
        assert_eq!(integrity_name(0x5000), "Protected");
    }

    #[test]
    fn session_kinds() {
        assert_eq!(session_kind(0, Some(0)), "services (session 0)");
        assert_eq!(session_kind(1, Some(0)), "console");
        assert_eq!(session_kind(2, Some(2)), "remote desktop");
        assert_eq!(session_kind(3, None), "interactive");
    }

    #[test]
    fn system_sid_resolves() {
        let name = account_name("S-1-5-18").expect("SYSTEM resolves");
        assert!(name.ends_with("SYSTEM"), "{name}");
        assert_eq!(account_name("not-a-sid"), None);
    }
}
