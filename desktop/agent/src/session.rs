// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Launches the UI in the user's session (the service lives in session 0, without a screen).

use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{DuplicateTokenEx, SecurityImpersonation, TOKEN_ALL_ACCESS, TokenPrimary};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken};
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::core::{PCWSTR, PWSTR};

/// Starts `exe args` as the user of the active console session.
pub fn spawn_in_active_session(exe: &Path, args: &str) -> windows::core::Result<()> {
    unsafe {
        let session = WTSGetActiveConsoleSessionId();
        if session == u32::MAX {
            return Ok(()); // nobody is signed in
        }
        let mut user_token = HANDLE::default();
        WTSQueryUserToken(session, &mut user_token)?;
        let mut primary = HANDLE::default();
        let dup =
            DuplicateTokenEx(user_token, TOKEN_ALL_ACCESS, None, SecurityImpersonation, TokenPrimary, &mut primary);
        let _ = CloseHandle(user_token);
        dup?;

        let mut env = std::ptr::null_mut();
        if let Err(e) = CreateEnvironmentBlock(&mut env, primary, false) {
            let _ = CloseHandle(primary);
            return Err(e);
        }
        let mut desktop: Vec<u16> = "winsta0\\default\0".encode_utf16().collect();
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            lpDesktop: PWSTR(desktop.as_mut_ptr()),
            ..Default::default()
        };
        let mut pi = PROCESS_INFORMATION::default();
        let mut cmd: Vec<u16> = format!("\"{}\" {}\0", exe.display(), args).encode_utf16().collect();
        let r = CreateProcessAsUserW(
            primary,
            PCWSTR::null(),
            PWSTR(cmd.as_mut_ptr()),
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT,
            Some(env),
            PCWSTR::null(),
            &si,
            &mut pi,
        );
        let _ = DestroyEnvironmentBlock(env);
        let _ = CloseHandle(primary);
        r?;
        let _ = CloseHandle(pi.hThread);
        let _ = CloseHandle(pi.hProcess);
    }
    Ok(())
}
