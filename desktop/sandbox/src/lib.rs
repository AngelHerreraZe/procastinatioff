// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Ejecuta un proceso en un AppContainer con SOLO la capacidad "webcam": sin `internetClient`,
//! Windows le niega la red (también localhost). Lo usa la UI para lanzar el verificador.
//!
//! El ejecutable y sus modelos deben ser legibles por "ALL APPLICATION PACKAGES" (S-1-15-2-1):
//! `C:\Program Files\` ya lo es; otras rutas necesitan `icacls <ruta> /grant *S-1-15-2-1:(OI)(CI)RX`.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::os::windows::io::FromRawHandle;

    use windows::Win32::Foundation::{
        CloseHandle, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, SetHandleInformation, WAIT_TIMEOUT,
    };
    use windows::Win32::Security::Isolation::{CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName};
    use windows::Win32::Security::{
        DeriveCapabilitySidsFromName, FreeSid, PSID, SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES, SID_AND_ATTRIBUTES,
    };
    use windows::Win32::System::Pipes::CreatePipe;
    use windows::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
        InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
        TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
    };
    use windows::core::{PCWSTR, PWSTR, w};

    const CONTAINER: PCWSTR = w!("ProcastinaciOFF.Verifier");
    const SE_GROUP_ENABLED: u32 = 0x4; // winnt.h

    /// Salida del proceso aislado.
    #[derive(Debug)]
    pub struct Output {
        /// Primera línea de stdout (el `VerificationResult` en JSON).
        pub line: String,
        pub exit_code: u32,
    }

    unsafe fn container_sid() -> windows::core::Result<PSID> {
        unsafe {
            match CreateAppContainerProfile(CONTAINER, CONTAINER, w!("Verificador sin red"), None) {
                Ok(sid) => Ok(sid),
                Err(_) => DeriveAppContainerSidFromAppContainerName(CONTAINER), // ya existía
            }
        }
    }

    unsafe fn webcam_sid() -> windows::core::Result<PSID> {
        unsafe {
            let (mut groups, mut ngroups) = (std::ptr::null_mut::<PSID>(), 0u32);
            let (mut caps, mut ncaps) = (std::ptr::null_mut::<PSID>(), 0u32);
            DeriveCapabilitySidsFromName(w!("webcam"), &mut groups, &mut ngroups, &mut caps, &mut ncaps)?;
            if ncaps == 0 {
                return Err(windows::core::Error::from_win32());
            }
            // ponytail: los arrays de grupos (y el propio array de capacidades) no se liberan: una vez por sesión.
            Ok(*caps)
        }
    }

    /// Termina un proceso aislado (botón "Detener" de la UI).
    pub fn terminate(pid: u32) {
        unsafe {
            use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE};
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                let _ = TerminateProcess(h, 1);
                let _ = CloseHandle(h);
            }
        }
    }

    /// Ejecuta `cmdline` en el AppContainer. `on_start` recibe el pid (para poder detenerlo).
    /// Espera la primera línea de stdout y el final del proceso (hasta `timeout_ms`; si se pasa, lo termina).
    pub fn run_sandboxed(cmdline: &str, timeout_ms: u32, on_start: impl FnOnce(u32)) -> windows::core::Result<Output> {
        unsafe {
            let app_sid = container_sid()?;
            let mut caps = [SID_AND_ATTRIBUTES { Sid: webcam_sid()?, Attributes: SE_GROUP_ENABLED }];
            let sc = SECURITY_CAPABILITIES {
                AppContainerSid: app_sid,
                Capabilities: caps.as_mut_ptr(),
                CapabilityCount: caps.len() as u32,
                Reserved: 0,
            };

            // Tubería para leer el stdout del verificador (solo el extremo de escritura es heredable)
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                bInheritHandle: true.into(),
                ..Default::default()
            };
            let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
            CreatePipe(&mut read, &mut write, Some(&sa), 0)?;
            SetHandleInformation(read, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0))?;

            // Atributos: capacidades del contenedor + lista EXPLÍCITA de handles heredados (solo `write`)
            let mut size = 0usize;
            let _ = InitializeProcThreadAttributeList(LPPROC_THREAD_ATTRIBUTE_LIST::default(), 2, 0, &mut size);
            let mut buf = vec![0u8; size];
            let list = LPPROC_THREAD_ATTRIBUTE_LIST(buf.as_mut_ptr() as *mut c_void);
            InitializeProcThreadAttributeList(list, 2, 0, &mut size)?;
            let inherit = [write];
            let attrs = (|| {
                UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                    Some(&sc as *const _ as *const c_void),
                    std::mem::size_of::<SECURITY_CAPABILITIES>(),
                    None,
                    None,
                )?;
                UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    Some(inherit.as_ptr() as *const c_void),
                    std::mem::size_of_val(&inherit),
                    None,
                    None,
                )
            })();

            let mut si = STARTUPINFOEXW::default();
            si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            si.StartupInfo.hStdOutput = write;
            si.StartupInfo.hStdError = write;
            si.lpAttributeList = list;
            let mut pi = PROCESS_INFORMATION::default();
            let mut cmd: Vec<u16> = format!("{cmdline}\0").encode_utf16().collect();
            let created = attrs.and_then(|_| {
                CreateProcessW(
                    PCWSTR::null(),
                    PWSTR(cmd.as_mut_ptr()),
                    None,
                    None,
                    true,
                    EXTENDED_STARTUPINFO_PRESENT,
                    None,
                    PCWSTR::null(),
                    &si.StartupInfo,
                    &mut pi,
                )
            });
            let _ = CloseHandle(write);
            DeleteProcThreadAttributeList(list);
            let _ = FreeSid(app_sid);
            if let Err(e) = created {
                let _ = CloseHandle(read);
                return Err(e);
            }
            let _ = CloseHandle(pi.hThread);
            on_start(pi.dwProcessId);

            let mut line = String::new();
            // Límite de lectura: el verificador solo puede devolver una línea corta.
            let _ = BufReader::new(std::io::Read::take(File::from_raw_handle(read.0 as _), 4096)).read_line(&mut line);
            if WaitForSingleObject(pi.hProcess, timeout_ms) == WAIT_TIMEOUT {
                let _ = TerminateProcess(pi.hProcess, 1);
            }
            let mut exit_code = 1u32;
            let _ = GetExitCodeProcess(pi.hProcess, &mut exit_code);
            let _ = CloseHandle(pi.hProcess);
            Ok(Output { line: line.trim().to_owned(), exit_code })
        }
    }
}

#[cfg(windows)]
pub use imp::{Output, run_sandboxed, terminate};
