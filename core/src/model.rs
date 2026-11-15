// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tipos del dominio. 🔒 Los que viajan en eventos se firman y sincronizan:
//! nunca `HashMap`/`HashSet` (orden no determinista) ni `f32`/`f64`.
//! Los textos que escribe el usuario (`title`, `label`) son datos personales: jamás a logs.

use serde::{Deserialize, Serialize};

/// Máscara de días: bit 0 = lunes … bit 6 = domingo.
pub const ALL_DAYS: u8 = 0b0111_1111;
pub const SATURDAY: u8 = 1 << 5;
pub const SUNDAY: u8 = 1 << 6;

/// Minutos desde medianoche, 0..=1439. Ejemplo: 22:30 → 1350.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, uniffi::Record)]
pub struct TimeOfDay {
    pub minutes: u16,
}

impl TimeOfDay {
    pub const fn hm(hour: u16, minute: u16) -> Self {
        Self { minutes: hour * 60 + minute }
    }
}

/// Horario de sueño. Si `bedtime == wake` no hay periodo de sueño.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Schedule {
    pub wake: TimeOfDay,
    pub bedtime: TimeOfDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    /// Hay que cumplirla para pasar a LIBRE.
    Key,
    /// Solo se registra; si se incumple puede restar permisos mañana.
    Habit,
}

/// Métodos de verificación (sección 7.1 del plan maestro).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    FocusSession,
    QrAnchor,
    Health,
    HonorDeclaration,
    Witness,
    FoodPhoto,
    ReadingPresence,
    PageOcr,
    Summary,
    PoseReps,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Goal {
    /// Identificador estable (ej. "leer-30"): minúsculas, cifras y guiones. No cambia nunca.
    pub id: String,
    /// Texto del usuario. 🔒 Dato personal: nunca a logs.
    pub title: String,
    pub kind: GoalKind,
    /// Minutos necesarios (0 si la meta no se mide por tiempo).
    pub target_minutes: u16,
    /// Veces que hay que cumplirla en el día (mínimo 1). Ej. "comer sano 2 de 3" → 2.
    pub target_count: u8,
    /// Métodos aceptados. En Fase 1 basta con cumplir uno; las recetas completas llegan en Fase 2.
    pub methods: Vec<Method>,
    /// Días en los que la meta es fija (base del plan): máscara `ALL_DAYS`.
    pub weekdays: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Config {
    pub schedule: Schedule,
    pub permits_per_day: u8,
    pub permit_minutes: u16,
    /// Minutos mínimos entre el fin de un permiso y el inicio del siguiente.
    pub permit_cooldown_minutes: u16,
    pub emergency_minutes: u16,
    /// Permisos que se restan al día siguiente por cada emergencia.
    pub emergency_penalty: u8,
    /// Si es true, cada meta hábito incumplida resta 1 permiso al día siguiente.
    pub habit_penalty: bool,
    /// Metas fijas: forman la base del plan de cada mañana en los días marcados.
    pub goals: Vec<Goal>,
    /// Días libres (misma máscara que `Goal::weekdays`): ese día no hay metas ni bloqueo
    /// en ningún dispositivo, y la noche anterior tampoco hay DORMIR.
    pub free_days: u8,
    /// Apps permitidas siempre (salvo en DORMIR), por plataforma. Formatos:
    /// "android:<paquete>@<sha256 cert>", "windows:pfn:<PackageFamilyName>", "windows:exe:<firmante>|<archivo.exe>".
    pub essential_apps: Vec<String>,
    /// Apps permitidas además durante ENFOQUE (Word, navegador gestionado, lector de PDF…).
    pub focus_apps: Vec<String>,
    /// Dominios permitidos durante ENFOQUE (ej. "udemy.com").
    pub web_allowlist: Vec<String>,
    /// Permisos extra preaprobados por el tutor, a gastar cualquier día (ventana móvil de 7 días).
    /// 🔒 Se omite al serializar si es 0: así los eventos firmados antes de existir el campo
    /// se re-serializan igual y su firma sigue siendo válida.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub weekly_bonus_permits: u8,
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

impl Config {
    /// Configuración de ejemplo usada en pruebas y como punto de partida del onboarding.
    pub fn example() -> Self {
        Self {
            schedule: Schedule { wake: TimeOfDay::hm(6, 30), bedtime: TimeOfDay::hm(22, 30) },
            permits_per_day: 3,
            permit_minutes: 5,
            permit_cooldown_minutes: 60,
            emergency_minutes: 15,
            emergency_penalty: 1,
            habit_penalty: true,
            goals: vec![
                Goal {
                    id: "leer-30".into(),
                    title: "Leer 30 minutos".into(),
                    kind: GoalKind::Key,
                    target_minutes: 30,
                    target_count: 1,
                    methods: vec![Method::FocusSession, Method::HonorDeclaration],
                    weekdays: ALL_DAYS,
                },
                Goal {
                    id: "comer-sano".into(),
                    title: "Comer sano".into(),
                    kind: GoalKind::Habit,
                    target_minutes: 0,
                    target_count: 2,
                    methods: vec![Method::HonorDeclaration],
                    weekdays: ALL_DAYS,
                },
            ],
            free_days: SATURDAY | SUNDAY,
            essential_apps: vec![
                "android:com.whatsapp".into(),
                "windows:pfn:5319275A.WhatsAppDesktop_cv1g1gvanyjgm".into(),
            ],
            focus_apps: vec![
                "windows:exe:Microsoft Corporation|winword.exe".into(),
                "windows:exe:Google LLC|chrome.exe".into(),
            ],
            web_allowlist: vec!["udemy.com".into(), "coursera.org".into()],
            weekly_bonus_permits: 0,
        }
    }
}

/// Lo que puede pasar en un día. Cada variante se firma y sincroniza (ver events.rs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    ConfigChanged {
        config: Config,
        effective_from_ms: i64,
    },
    /// Metas del día, definidas al primer desbloqueo tras la hora de dormir.
    /// Solo cuenta la primera del día; las siguientes, si la endurecen.
    DayPlanned {
        goals: Vec<Goal>,
    },
    GoalCompleted {
        goal_id: String,
        method: Method,
        score: u8,
        minutes: u16,
    },
    PassStarted {
        minutes: u16,
    },
    FocusStarted {
        goal_id: String,
    },
    FocusEnded {
        goal_id: String,
    },
    EmergencyStarted,
    Tamper {
        what: TamperKind,
    },
    Heartbeat,
    /// Lista completa de dispositivos del buzón. Solo se cifra para esta lista.
    MembersChanged {
        members: Vec<Member>,
    },
    /// Solo en el buzón del compañero de rendición de cuentas: qué pasó y cuándo, sin más detalle.
    ForPartner {
        happened_at_ms: i64,
        alert: PartnerAlert,
    },
    // ── Modo familiar (Fase 3). Quién puede firmar cada uno: ledger.rs ──
    /// El menor pide algo al tutor.
    Request {
        request_id: String,
        what: RequestKind,
    },
    /// Respuesta del tutor. Si aprueba, el efecto va en un evento aparte (también del tutor).
    Decision {
        request_id: String,
        approved: bool,
    },
    /// Permiso extra solo para el día en que se concede; dura `minutes`.
    GrantPermit {
        minutes: u16,
    },
    /// Día de descanso: LIBRE hasta dormir, sin penalización por hábitos.
    RestDayGranted,
    /// Número de intentos bloqueados por el filtro DNS en el día. 🔒 Solo el número; nunca los dominios.
    BlockedAttempts {
        count: u16,
    },
}

/// Lo que un menor puede pedir a su tutor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "what", rename_all = "snake_case")]
pub enum RequestKind {
    /// Un permiso más hoy, de `minutes` minutos.
    ExtraPermit { minutes: u16 },
    /// Añadir un dominio a la lista blanca del enfoque (cambio permanente de la configuración).
    AddSite { domain: String },
    /// Retrasar la hora de dormir (cambio permanente del horario).
    LaterBedtime { minutes: u16 },
    /// Hoy, día de descanso.
    RestDay,
}

/// Lo ÚNICO que ve el compañero de rendición de cuentas: tipos y números, nunca metas ni textos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "alert", rename_all = "snake_case")]
pub enum PartnerAlert {
    Tamper {
        what: TamperKind,
    },
    Emergency,
    /// Metas cumplidas del día que empezó en `happened_at_ms` (todas, llave y hábitos).
    DailySummary {
        completed: u8,
        total: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum TamperKind {
    PermissionRevoked,
    ServiceStopped,
    ClockChanged,
    UninstallAttempt,
    SafeMode,
    DeviceSilent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Dueño del perfil (modo personal).
    Owner,
    /// Tutor (modo familiar, Fase 3).
    Guardian,
    /// Menor (modo familiar, Fase 3).
    Minor,
    /// Compañero de rendición de cuentas (buzón aparte, Fase 2).
    Partner,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Member {
    /// Clave pública Ed25519 en hex = ID del dispositivo.
    pub device_id: String,
    /// Destinatario age ("age1…") para cifrarle eventos.
    pub age_recipient: String,
    /// Nombre visible ("PC del estudio"). 🔒 Dato personal: nunca a logs.
    pub label: String,
    pub role: Role,
}

/// Un evento ya verificado (firma y autor comprobados en ledger.rs), listo para las reglas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub at_ms: i64,
    pub kind: EventKind,
    /// Lo firmó un tutor del perfil (sus cambios de configuración rigen de inmediato).
    pub by_guardian: bool,
}

impl Fact {
    pub fn new(at_ms: i64, kind: EventKind) -> Self {
        Self { at_ms, kind, by_guardian: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DayState {
    Sleep,
    Locked,
    Pass { until_ms: i64 },
    Focus { goal_id: String },
    Free,
    Emergency { until_ms: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct GoalProgress {
    pub goal_id: String,
    /// 🔒 Dato personal: se muestra en la UI, nunca a logs.
    pub title: String,
    pub kind: GoalKind,
    pub done_count: u8,
    pub target_count: u8,
    pub focus_minutes: u16,
    pub target_minutes: u16,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Status {
    pub state: DayState,
    pub permits_left: u8,
    pub day_start_ms: i64,
    pub next_wake_ms: i64,
    /// Próximo inicio de DORMIR (None si no hay sueño o esta noche es libre).
    pub sleep_at_ms: Option<i64>,
    /// Hoy es día libre: sin metas ni bloqueo.
    pub free_day: bool,
    /// Aún no se definieron las metas de hoy (la UI muestra el plan del día).
    pub needs_plan: bool,
    pub goals: Vec<GoalProgress>,
}
