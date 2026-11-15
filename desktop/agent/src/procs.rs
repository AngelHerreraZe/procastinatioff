// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Processes of the user sessions.
//! ponytail: polling every 300 ms (~1–2 ms per round); if spike C gives p95 > 500 ms, switch to ETW
//! process-creation events keeping the same `policy::decide`.

use std::path::PathBuf;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, QueryFullProcessImageNameW,
    TerminateProcess,
};
use windows::core::PWSTR;

use crate::policy::{AppIdentity, pfn_from_path};
use crate::signer::SignerCache;

pub struct Proc {
    pub pid: u32,
    pub path: PathBuf,
}

/// Processes of the user sessions (session > 0) with their full path.
pub fn user_processes() -> Vec<Proc> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let pid = entry.th32ProcessID;
            let mut session = 0u32;
            if pid > 4
                && ProcessIdToSessionId(pid, &mut session).is_ok()
                && session > 0
                && let Some(path) = image_path(pid)
            {
                out.push(Proc { pid, path });
            }
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

pub fn image_path(pid: u32) -> Option<PathBuf> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

pub fn identity(p: &Proc, cache: &mut SignerCache) -> AppIdentity {
    let path = p.path.to_string_lossy().to_lowercase();
    AppIdentity {
        file_name: path.rsplit('\\').next().unwrap_or_default().to_owned(),
        store_pfn: pfn_from_path(&path),
        signer: cache.get(&p.path),
        path,
    }
}

pub fn terminate(pid: u32) {
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(h, 1);
            let _ = CloseHandle(h);
        }
    }
}

pub fn is_running(file_name: &str) -> bool {
    user_processes()
        .iter()
        .any(|p| p.path.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case(file_name)).unwrap_or(false))
}
