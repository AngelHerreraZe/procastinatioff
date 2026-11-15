// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Política de bloqueo, pura y probada: decide si un proceso se permite. No toca Windows.

use procastinacioff_core::model::{Config, DayState};

/// Cómo identificamos una app en Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    /// Ruta completa en minúsculas.
    pub path: String,
    /// Nombre del ejecutable en minúsculas (ej. "chrome.exe").
    pub file_name: String,
    /// Firmante Authenticode verificado (ej. "Google LLC"), si lo hay.
    pub signer: Option<String>,
    /// Package Family Name si es una app de la Microsoft Store.
    pub store_pfn: Option<String>,
}

impl AppIdentity {
    /// Identificadores con el formato de `Config::essential_apps` / `focus_apps`.
    pub fn ids(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(pfn) = &self.store_pfn {
            v.push(format!("windows:pfn:{pfn}"));
        }
        if let Some(s) = &self.signer {
            v.push(format!("windows:exe:{s}|{}", self.file_name));
        }
        v
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Block,
}

/// Subcarpetas de Windows en las que un usuario estándar puede escribir (bypass conocido de AppLocker).
const USER_WRITABLE: [&str; 8] = [
    "\\temp\\",
    "\\tasks\\",
    "\\tracing\\",
    "\\registration\\",
    "\\system32\\tasks\\",
    "\\syswow64\\tasks\\",
    "\\system32\\spool\\",
    "\\system32\\microsoft\\crypto\\",
];

/// Binario del sistema operativo: bajo `windows_dir` y fuera de las carpetas escribibles por el usuario.
/// ponytail: lista fija; si aparecen más rutas, verificar la firma de catálogo de Microsoft (WinVerifyTrust + catálogo).
pub fn is_system_path(path: &str, windows_dir: &str) -> bool {
    let root = windows_dir.to_lowercase();
    match path.strip_prefix(root.as_str()) {
        Some(rest) if rest.starts_with('\\') => !USER_WRITABLE.iter().any(|d| rest.starts_with(d)),
        _ => false,
    }
}

/// Package Family Name a partir de la carpeta de WindowsApps.
/// "c:\program files\windowsapps\5319275a.whatsappdesktop_2.24.1.0_x64__cv1g1gvanyjgm\whatsapp.exe"
///   → "5319275a.whatsappdesktop_cv1g1gvanyjgm"
/// WindowsApps solo la puede escribir TrustedInstaller: la ruta es una identidad fiable.
pub fn pfn_from_path(path: &str) -> Option<String> {
    let rest = path.split("\\windowsapps\\").nth(1)?;
    let folder = rest.split('\\').next()?;
    let parts: Vec<&str> = folder.split('_').collect();
    if parts.len() == 5 { Some(format!("{}_{}", parts[0], parts[4])) } else { None }
}

pub fn decide(state: &DayState, app: &AppIdentity, cfg: &Config, install_dir: &str, windows_dir: &str) -> Verdict {
    // 1. Estados permisivos (incluye los días libres: el núcleo los devuelve como LIBRE)
    if matches!(state, DayState::Free | DayState::Pass { .. } | DayState::Emergency { .. }) {
        return Verdict::Allow;
    }
    // 2. Nuestros binarios y los del sistema operativo
    if app.path.starts_with(&format!("{}\\", install_dir.to_lowercase())) || is_system_path(&app.path, windows_dir) {
        return Verdict::Allow;
    }
    // 3. DORMIR: nada más
    if matches!(state, DayState::Sleep) {
        return Verdict::Block;
    }
    // 4. BLOQUEADO / ENFOQUE: listas de la configuración (comparación sin mayúsculas)
    let lists: Vec<&String> = match state {
        DayState::Focus { .. } => cfg.essential_apps.iter().chain(cfg.focus_apps.iter()).collect(),
        _ => cfg.essential_apps.iter().collect(),
    };
    let ids = app.ids();
    if ids.iter().any(|id| lists.iter().any(|allowed| allowed.eq_ignore_ascii_case(id))) {
        Verdict::Allow
    } else {
        Verdict::Block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: &str = r"C:\Windows";
    const INSTALL: &str = r"C:\Program Files\ProcastinaciOFF";

    fn app(path: &str, signer: Option<&str>) -> AppIdentity {
        let path = path.to_lowercase();
        AppIdentity {
            file_name: path.rsplit('\\').next().unwrap().to_owned(),
            store_pfn: pfn_from_path(&path),
            signer: signer.map(String::from),
            path,
        }
    }

    const WHATSAPP: &str =
        r"C:\Program Files\WindowsApps\5319275A.WhatsAppDesktop_2.24.1.0_x64__cv1g1gvanyjgm\WhatsApp.exe";

    #[test]
    fn locked_allows_whatsapp_store_and_blocks_game() {
        let cfg = Config::example();
        let wa = app(WHATSAPP, None);
        let game = app(r"C:\Games\game.exe", Some("Game Studio"));
        assert_eq!(decide(&DayState::Locked, &wa, &cfg, INSTALL, WIN), Verdict::Allow);
        assert_eq!(decide(&DayState::Locked, &game, &cfg, INSTALL, WIN), Verdict::Block);
    }

    #[test]
    fn renamed_exe_without_right_signer_is_blocked() {
        let cfg = Config::example();
        // Alguien renombra un juego a chrome.exe: el firmante no es Google
        let fake = app(r"C:\Users\x\chrome.exe", Some("Game Studio"));
        assert_eq!(decide(&DayState::Focus { goal_id: "leer-30".into() }, &fake, &cfg, INSTALL, WIN), Verdict::Block);
        let real = app(r"C:\Program Files\Google\Chrome\Application\chrome.exe", Some("Google LLC"));
        assert_eq!(decide(&DayState::Focus { goal_id: "leer-30".into() }, &real, &cfg, INSTALL, WIN), Verdict::Allow);
    }

    #[test]
    fn sleep_blocks_even_essential_but_not_system() {
        let cfg = Config::example();
        let wa = app(WHATSAPP, None);
        let explorer = app(r"C:\Windows\explorer.exe", None);
        assert_eq!(decide(&DayState::Sleep, &wa, &cfg, INSTALL, WIN), Verdict::Block);
        assert_eq!(decide(&DayState::Sleep, &explorer, &cfg, INSTALL, WIN), Verdict::Allow);
    }

    #[test]
    fn user_writable_windows_folders_are_not_system() {
        let cfg = Config::example();
        for p in [
            r"C:\Windows\Temp\juego.exe",
            r"C:\Windows\Tasks\juego.exe",
            r"C:\Windows\System32\spool\drivers\color\j.exe",
        ] {
            assert_eq!(decide(&DayState::Locked, &app(p, None), &cfg, INSTALL, WIN), Verdict::Block, "{p}");
        }
        let notepad = app(r"C:\Windows\System32\notepad.exe", None);
        assert_eq!(decide(&DayState::Locked, &notepad, &cfg, INSTALL, WIN), Verdict::Allow);
    }

    #[test]
    fn a_folder_named_like_ours_is_not_ours() {
        let cfg = Config::example();
        // "C:\Program Files\ProcastinaciOFF-falso\juego.exe" empieza igual que la carpeta de instalación
        let fake = app(r"C:\Program Files\ProcastinaciOFF-falso\juego.exe", None);
        assert_eq!(decide(&DayState::Locked, &fake, &cfg, INSTALL, WIN), Verdict::Block);
    }
}
